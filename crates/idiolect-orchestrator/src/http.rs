//! Read-only HTTP query API over the catalog.
//!
//! Exposes the pure functions in [`crate::query`] as JSON endpoints.
//! The API is read-only (GET only) by design: the orchestrator never
//! accepts records over HTTP — records enter via the firehose (P2).
//!
//! # Layout
//!
//! - `/healthz`, `/readyz`, `/metrics` — ops
//! - `/v1/stats` — aggregate counts across every kind
//! - `/v1/verifications/sufficient` — scalar bool
//!
//! List endpoints (`/v1/bounties/open`, `/v1/adapters`, …) are
//! emitted into [`crate::generated::http`] from
//! `orchestrator-spec/queries.json`. The [`router`] fn composes
//! them with the routes above via
//! [`crate::generated::http::register_routes`].
//!
//! [`AppState`], [`ApiError`], [`EnvelopedEntry`], [`Page`], and
//! [`Paged`] are `pub(crate)` so the generated module can use them
//! without exposing them on the public API surface.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::{Deserialize, Serialize};

use idiolect_records::generated::dev::idiolect::defs::LensRef;
use idiolect_records::generated::dev::idiolect::recommendation::RecommendationRequiredVerifications;

use crate::catalog::{Catalog, Entry};
use crate::query;

/// Shared application state: catalog handle + readiness flag.
#[derive(Clone)]
pub struct AppState {
    /// Shared catalog. The `Arc<Mutex<Catalog>>` is the same handle
    /// `CatalogHandler::catalog()` returns; the daemon binary passes
    /// that value here directly.
    pub catalog: Arc<Mutex<Catalog>>,
    /// Readiness flag. `true` means the catalog has been warmed from
    /// its persistent backing store (if any) and the process is ready
    /// to serve traffic. Driven by the daemon binary.
    pub ready: Arc<std::sync::atomic::AtomicBool>,
}

impl AppState {
    /// Construct an app state that is immediately ready. Suitable for
    /// tests and for daemon deployments without a warmup phase.
    #[must_use]
    pub fn ready(catalog: Arc<Mutex<Catalog>>) -> Self {
        Self {
            catalog,
            ready: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        }
    }

    /// Construct an app state that starts NOT ready. The caller must
    /// flip the flag to `true` once warmup finishes.
    #[must_use]
    pub fn with_readiness(
        catalog: Arc<Mutex<Catalog>>,
        ready: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self { catalog, ready }
    }
}

/// Build the axum router with every query endpoint wired up.
///
/// Pass the result to `axum::serve(listener, router.into_make_service())`
/// in the binary.
pub fn router(state: AppState) -> Router {
    let mut r = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/v1/stats", get(stats))
        .route(
            "/v1/verifications/sufficient",
            get(verifications_sufficient),
        );
    // Splice the generated list endpoints onto the router.
    r = crate::generated::http::register_routes(r);
    r.with_state(state)
}

// -----------------------------------------------------------------
// Error type shared with the generated module.
// -----------------------------------------------------------------

/// Structured error body returned on non-2xx responses. Mirrors the
/// atproto XRPC error convention: `{ error, message }`.
#[derive(Debug, Serialize)]
pub(crate) struct ErrorBody {
    /// Short, stable error code (`invalid_request`, `internal`, …).
    pub error: String,
    /// Human-readable message; safe to log but not to show to end users.
    pub message: String,
}

/// Rich error type backing every list endpoint's failure case.
/// `pub(crate)` so the generated HTTP handlers can construct it.
pub(crate) struct ApiError {
    pub status: StatusCode,
    pub body: ErrorBody,
}

impl ApiError {
    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: ErrorBody {
                error: "internal".into(),
                message: msg.into(),
            },
        }
    }

    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            body: ErrorBody {
                error: "invalid_request".into(),
                message: msg.into(),
            },
        }
    }
}

impl From<String> for ApiError {
    fn from(s: String) -> Self {
        Self::invalid_request(s)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, axum::Json(self.body)).into_response()
    }
}

impl From<std::sync::PoisonError<std::sync::MutexGuard<'_, Catalog>>> for ApiError {
    fn from(e: std::sync::PoisonError<std::sync::MutexGuard<'_, Catalog>>) -> Self {
        Self::internal(format!("catalog lock poisoned: {e}"))
    }
}

// -----------------------------------------------------------------
// Envelope + pagination — shared with the generated module.
// -----------------------------------------------------------------

/// Shape every endpoint returns for records: `(uri, author, rev, record)`.
#[derive(Debug, Serialize)]
pub(crate) struct EnvelopedEntry<R> {
    pub uri: String,
    pub author: String,
    pub rev: String,
    pub record: R,
}

impl<R: Clone> From<&Entry<R>> for EnvelopedEntry<R> {
    fn from(e: &Entry<R>) -> Self {
        Self {
            uri: e.uri.clone(),
            author: e.author.clone(),
            rev: e.rev.clone(),
            record: e.record.clone(),
        }
    }
}

/// Paged list response.
#[derive(Debug, Serialize)]
pub(crate) struct Paged<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

impl<T> Paged<T> {
    pub fn from_collected(mut items: Vec<T>, page: &Page) -> Result<Self, ApiError> {
        let total = items.len();
        page.apply(&mut items)?;
        let (limit, offset) = page.validated()?;
        Ok(Self {
            items,
            total,
            limit,
            offset,
        })
    }
}

/// Pagination query parameters. `pub(crate)` so generated handlers
/// can splice it into their `Params` struct via `#[serde(flatten)]`.
#[derive(Debug, Deserialize)]
pub(crate) struct Page {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            limit: default_limit(),
            offset: 0,
        }
    }
}

const fn default_limit() -> usize {
    100
}

const MAX_LIMIT: usize = 1000;

impl Page {
    fn validated(&self) -> Result<(usize, usize), ApiError> {
        if self.limit == 0 {
            return Err(ApiError::invalid_request("limit must be > 0"));
        }
        if self.limit > MAX_LIMIT {
            return Err(ApiError::invalid_request(format!(
                "limit exceeds max of {MAX_LIMIT}"
            )));
        }
        Ok((self.limit, self.offset))
    }

    pub fn apply<T>(&self, items: &mut Vec<T>) -> Result<(), ApiError> {
        let (limit, offset) = self.validated()?;
        if offset >= items.len() {
            items.clear();
        } else {
            items.drain(..offset);
            items.truncate(limit);
        }
        Ok(())
    }
}

// -----------------------------------------------------------------
// Hand-written endpoints.
// -----------------------------------------------------------------

async fn healthz() -> &'static str {
    "ok"
}

async fn readyz(State(s): State<AppState>) -> Response {
    if s.ready.load(std::sync::atomic::Ordering::SeqCst) {
        (StatusCode::OK, "ready").into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "warming").into_response()
    }
}

async fn stats(State(s): State<AppState>) -> Result<axum::Json<query::CatalogStats>, ApiError> {
    let catalog = s.catalog.lock()?;
    Ok(axum::Json(query::catalog_stats(&catalog)))
}

async fn metrics(State(s): State<AppState>) -> Result<Response, ApiError> {
    let stats = {
        let catalog = s.catalog.lock()?;
        query::catalog_stats(&catalog)
    };
    let ready = u8::from(s.ready.load(std::sync::atomic::Ordering::SeqCst));
    let body = format!(
        "# HELP idiolect_orchestrator_ready Whether the orchestrator has warmed its catalog and is serving traffic.\n\
         # TYPE idiolect_orchestrator_ready gauge\n\
         idiolect_orchestrator_ready {ready}\n\
         # HELP idiolect_orchestrator_catalog_records Number of cataloged records by kind.\n\
         # TYPE idiolect_orchestrator_catalog_records gauge\n\
         idiolect_orchestrator_catalog_records{{kind=\"adapter\"}} {}\n\
         idiolect_orchestrator_catalog_records{{kind=\"belief\"}} {}\n\
         idiolect_orchestrator_catalog_records{{kind=\"bounty\"}} {}\n\
         idiolect_orchestrator_catalog_records{{kind=\"community\"}} {}\n\
         idiolect_orchestrator_catalog_records{{kind=\"dialect\"}} {}\n\
         idiolect_orchestrator_catalog_records{{kind=\"recommendation\"}} {}\n\
         idiolect_orchestrator_catalog_records{{kind=\"verification\"}} {}\n\
         idiolect_orchestrator_catalog_records{{kind=\"vocab\"}} {}\n\
         # HELP idiolect_orchestrator_catalog_total Sum of records across every kind.\n\
         # TYPE idiolect_orchestrator_catalog_total gauge\n\
         idiolect_orchestrator_catalog_total {}\n",
        stats.adapters,
        stats.beliefs,
        stats.bounties,
        stats.communities,
        stats.dialects,
        stats.recommendations,
        stats.verifications,
        stats.vocabularies,
        stats.total(),
    );
    Ok((
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        body,
    )
        .into_response())
}

#[derive(Debug, Deserialize)]
struct SufficientParams {
    lens_uri: String,
    kinds: String,
    #[serde(default = "default_true")]
    hold: bool,
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
struct SufficientResponse {
    sufficient: bool,
    required_kinds: Vec<String>,
    require_holds: bool,
}

async fn verifications_sufficient(
    State(s): State<AppState>,
    Query(p): Query<SufficientParams>,
) -> Result<axum::Json<SufficientResponse>, ApiError> {
    let catalog = s.catalog.lock()?;
    let lens = LensRef {
        uri: Some(p.lens_uri),
        cid: None,
        direction: None,
    };
    let (required, kind_strs) =
        parse_required_kinds(&p.kinds).map_err(ApiError::invalid_request)?;
    let sufficient = query::sufficient_verifications_for(&catalog, &lens, &required, p.hold);
    Ok(axum::Json(SufficientResponse {
        sufficient,
        required_kinds: kind_strs,
        require_holds: p.hold,
    }))
}

fn parse_required_kinds(
    raw: &str,
) -> Result<(Vec<RecommendationRequiredVerifications>, Vec<String>), String> {
    use idiolect_records::generated::dev::idiolect::defs::{
        LpChecker, LpConformance, LpConvergence, LpGenerator, LpRoundtrip, LpTheorem,
    };

    let mut kinds = Vec::new();
    let mut names = Vec::new();
    for token in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        // `required_kinds` is a convenience HTTP surface that lets an
        // operator ask "is there any verification of this kind?" using
        // coarse v0.1-style kind names. We wrap each kind in the
        // corresponding LensProperty variant with empty details; the
        // sufficiency check only dispatches on the variant.
        let kind = match token {
            "roundtrip-test" => RecommendationRequiredVerifications::LpRoundtrip(LpRoundtrip {
                domain: String::new(),
                generator: None,
            }),
            "property-test" => RecommendationRequiredVerifications::LpGenerator(LpGenerator {
                spec: String::new(),
                runner: None,
                seed: None,
            }),
            "formal-proof" => RecommendationRequiredVerifications::LpTheorem(LpTheorem {
                statement: String::new(),
                system: None,
                free_variables: None,
            }),
            "conformance-test" => {
                RecommendationRequiredVerifications::LpConformance(LpConformance {
                    standard: String::new(),
                    version: String::new(),
                    clauses: None,
                })
            }
            "static-check" => RecommendationRequiredVerifications::LpChecker(LpChecker {
                checker: String::new(),
                ruleset: None,
                version: None,
            }),
            "convergence-preserving" => {
                RecommendationRequiredVerifications::LpConvergence(LpConvergence {
                    property: String::new(),
                    bound_steps: None,
                })
            }
            other => return Err(format!("unknown verification kind: {other}")),
        };
        kinds.push(kind);
        names.push(token.to_owned());
    }
    Ok((kinds, names))
}
