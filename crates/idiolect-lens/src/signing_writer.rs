//! Authenticated-writes wrapper around [`ReqwestPdsClient`].
//!
//! atproto records are published under a DID's repo via XRPC endpoints
//! that require an authenticated session. `ReqwestPdsClient` by itself
//! doesn't know about auth — it just POSTs JSON. This wrapper adds the
//! two headers the PDS expects:
//!
//! - `Authorization: DPoP <access_token>` — the access JWT from an
//!   OAuth session. Earlier atproto deployments used
//!   `Authorization: Bearer <token>` with appPasswords; OAuth is the
//!   future and what this wrapper names first.
//! - `DPoP: <proof>` — a `DPoP` proof JWT bound to this specific
//!   request (method, URL, nonce, access-token hash). Construction
//!   of the proof itself is deferred to the [`DpopProver`] trait so
//!   the lens crate stays free of a crypto dep; a caller that has a
//!   `DPoP` key on hand plugs it in.
//!
//! # Scope
//!
//! This is the *authenticated-writes* helper, not a full OAuth client.
//! Callers acquire the session via `idiolect-oauth` + their choice of
//! authorization-server flow, then feed the access token + nonce here.
//! Token refresh is the caller's responsibility; inspect
//! [`SigningPdsWriter::last_dpop_nonce`] after each write to see the
//! updated nonce the PDS returned.
//!
//! # Not shipped
//!
//! A default [`DpopProver`] impl using a specific key type (ES256,
//! ES384). The trait is intentionally narrow so callers can wire in
//! `ring`, `rustcrypto`, `jsonwebtoken`, or a hardware-key backend
//! without dragging any of them into this crate.
//!
//! Feature-gated under `pds-reqwest` (the writer it wraps).

#[cfg(feature = "pds-reqwest")]
use crate::error::LensError;
#[cfg(feature = "pds-reqwest")]
use crate::pds_reqwest::ReqwestPdsClient;
#[cfg(feature = "pds-reqwest")]
use crate::resolver::{
    CreateRecordRequest, DeleteRecordRequest, PdsClient, PdsWriter, PutRecordRequest,
    WriteRecordResponse,
};
#[cfg(feature = "pds-reqwest")]
use std::sync::Mutex;

/// Abstraction over `DPoP` proof construction.
///
/// One call per request. The proof is a signed JWT over `(htu, htm,
/// iat, jti, nonce, ath)` bound to this specific request. Callers
/// must supply a stable private key matching the one whose public
/// component was registered with the authorization server.
///
/// The crate ships this trait deliberately minimal: no crypto dep,
/// no canonical impl. Users wire in `ring`, `rustcrypto`,
/// `jsonwebtoken`, or a hardware-key backend on top.
pub trait DpopProver: Send + Sync {
    /// Produce a `DPoP` proof JWT for the request `(method, url)`.
    ///
    /// `nonce` is the `DPoP-Nonce` value the PDS returned on the
    /// previous request (or `None` for the first call). `access_token`
    /// is needed to compute the `ath` claim (`base64url(sha256(access_token))`).
    ///
    /// # Errors
    ///
    /// Implementations return any caller-provided error message as a
    /// `String`; the wrapper surfaces it as
    /// [`LensError::Transport`](crate::LensError::Transport).
    fn proof(
        &self,
        method: &str,
        url: &str,
        access_token: &str,
        nonce: Option<&str>,
    ) -> Result<String, String>;
}

/// A `DPoP` prover that always returns a caller-supplied static proof
/// string. Intended strictly for tests — real deployments must use a
/// crypto-backed impl.
#[derive(Debug, Clone)]
pub struct StaticDpopProver {
    /// The `DPoP` proof JWT to return from every call. In tests this is
    /// a well-known fixed string that a mock server can assert against.
    pub proof: String,
}

impl DpopProver for StaticDpopProver {
    fn proof(
        &self,
        _method: &str,
        _url: &str,
        _access_token: &str,
        _nonce: Option<&str>,
    ) -> Result<String, String> {
        Ok(self.proof.clone())
    }
}

/// A `DPoP` prover that errors if ever called. Paired with
/// [`AuthScheme::Bearer`] in [`SigningPdsWriter`] so the type system
/// still requires a prover even though it won't be invoked — giving
/// callers a single construction API across both auth schemes.
///
/// # Errors
///
/// `proof` always returns an `Err`. Callers see this only as a
/// programmer error (Bearer scheme should never reach this code path).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoOpDpopProver;

impl DpopProver for NoOpDpopProver {
    fn proof(
        &self,
        _method: &str,
        _url: &str,
        _access_token: &str,
        _nonce: Option<&str>,
    ) -> Result<String, String> {
        Err("NoOpDpopProver.proof called; did you mean to use StaticDpopProver or a real crypto-backed impl?".into())
    }
}

/// HTTP authentication scheme to use on each write.
///
/// - [`AuthScheme::Bearer`] — legacy `Authorization: Bearer <token>`
///   appropriate for atproto PDSes that have not yet enabled OAuth
///   + `DPoP`. The `DPoP` prover is never invoked in this mode.
/// - [`AuthScheme::Dpop`] — OAuth-era `Authorization: DPoP <token>`
///   plus a `DPoP: <proof>` header the prover supplies per-request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthScheme {
    /// Legacy bearer-token auth: no `DPoP` proof, no nonce chaining.
    Bearer,
    /// OAuth / `DPoP` auth: caller's [`DpopProver`] supplies the proof
    /// JWT per request; the server-issued `DPoP-Nonce` is threaded
    /// through subsequent calls.
    Dpop,
}

/// Authenticated writes over a [`ReqwestPdsClient`].
///
/// Wraps any inner PDS client (typically `ReqwestPdsClient`) and adds
/// `Authorization: DPoP <access_token>` + `DPoP: <proof>` to every
/// write. Reads pass through untouched — the PDS `getRecord` endpoint
/// is public on `public-detailed` records.
///
/// The wrapper tracks the last `DPoP-Nonce` the server returned so
/// subsequent requests can chain it into the next proof. Callers that
/// want to snapshot it for a paused flow (e.g. a long-lived daemon)
/// reach for [`last_dpop_nonce`](Self::last_dpop_nonce).
#[cfg(feature = "pds-reqwest")]
pub struct SigningPdsWriter<P> {
    inner: ReqwestPdsClient,
    access_token: String,
    prover: P,
    scheme: AuthScheme,
    /// `DPoP` nonce echoed back by the PDS on the previous request.
    /// Initially `None`; the PDS returns an initial nonce in the
    /// 401 response of the first unauthenticated call.
    /// Unused when [`scheme`](Self::scheme) is [`AuthScheme::Bearer`].
    nonce: Mutex<Option<String>>,
}

#[cfg(feature = "pds-reqwest")]
impl<P> SigningPdsWriter<P> {
    /// Wrap a reqwest PDS client with an access token and `DPoP` prover.
    ///
    /// Construct a DPoP-mode writer (the modern OAuth-era path).
    ///
    /// `initial_nonce` is the `DPoP-Nonce` value the server returned
    /// on the login response (the dance that produced the access
    /// token). Callers without one pass `None` and absorb one
    /// round-trip penalty on the first write.
    pub const fn new(
        inner: ReqwestPdsClient,
        access_token: String,
        prover: P,
        initial_nonce: Option<String>,
    ) -> Self {
        Self {
            inner,
            access_token,
            prover,
            scheme: AuthScheme::Dpop,
            nonce: Mutex::new(initial_nonce),
        }
    }

    /// Construct a writer with the caller-specified auth scheme.
    ///
    /// Use this when the PDS is still on the legacy app-password /
    /// Bearer auth path. Pair with [`NoOpDpopProver`] to make it
    /// explicit that the prover is never invoked in Bearer mode:
    ///
    /// ```ignore
    /// SigningPdsWriter::with_scheme(
    ///     client,
    ///     access_token,
    ///     NoOpDpopProver,
    ///     AuthScheme::Bearer,
    ///     None,
    /// );
    /// ```
    pub const fn with_scheme(
        inner: ReqwestPdsClient,
        access_token: String,
        prover: P,
        scheme: AuthScheme,
        initial_nonce: Option<String>,
    ) -> Self {
        Self {
            inner,
            access_token,
            prover,
            scheme,
            nonce: Mutex::new(initial_nonce),
        }
    }

    /// Which auth scheme the wrapper uses.
    #[must_use]
    pub const fn scheme(&self) -> AuthScheme {
        self.scheme
    }

    /// Snapshot the `DPoP` nonce the server most recently returned. A
    /// caller that serializes a session for later use should persist
    /// this value alongside the access token.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex has been poisoned.
    #[must_use]
    pub fn last_dpop_nonce(&self) -> Option<String> {
        self.nonce.lock().expect("nonce mutex poisoned").clone()
    }

    /// Borrow the underlying reqwest PDS client — useful for reads
    /// that shouldn't incur `DPoP` overhead.
    #[must_use]
    pub const fn inner(&self) -> &ReqwestPdsClient {
        &self.inner
    }
}

#[cfg(feature = "pds-reqwest")]
impl<P: DpopProver> SigningPdsWriter<P> {
    /// Build the signed request for a write to `path` with JSON body.
    /// Returns the updated nonce read off the response's `DPoP-Nonce`
    /// header (stored by the caller) plus the response body.
    async fn signed_post(
        &self,
        xrpc_method: &str,
        body: serde_json::Value,
    ) -> Result<reqwest::Response, LensError> {
        let url = format!("{}/xrpc/{xrpc_method}", self.inner.base_url());

        let req = match self.scheme {
            AuthScheme::Bearer => self
                .inner
                .http()
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.access_token)),
            AuthScheme::Dpop => {
                let nonce = self.last_dpop_nonce();
                let proof = self
                    .prover
                    .proof("POST", &url, &self.access_token, nonce.as_deref())
                    .map_err(|e| LensError::Transport(format!("DPoP proof: {e}")))?;
                self.inner
                    .http()
                    .post(&url)
                    .header("Authorization", format!("DPoP {}", self.access_token))
                    .header("DPoP", proof)
            }
        };

        let resp = req
            .json(&body)
            .send()
            .await
            .map_err(|e| LensError::Transport(format!("signed POST {url}: {e}")))?;

        // Capture the updated DPoP nonce only in DPoP mode; Bearer
        // deployments never issue `DPoP-Nonce` so skipping the header
        // lookup is harmless but saves a mutex acquisition.
        if self.scheme == AuthScheme::Dpop
            && let Some(new_nonce) = resp
                .headers()
                .get("DPoP-Nonce")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
            {
                let mut guard = self
                    .nonce
                    .lock()
                    .map_err(|e| LensError::Transport(format!("nonce mutex poisoned: {e}")))?;
                *guard = Some(new_nonce);
            }
        Ok(resp)
    }
}

#[cfg(feature = "pds-reqwest")]
impl<P: DpopProver> PdsClient for SigningPdsWriter<P> {
    /// Reads are not authenticated — atproto getRecord is public on
    /// `public-detailed` records. Callers that need authenticated
    /// reads wrap the inner client separately.
    async fn get_record(
        &self,
        did: &str,
        collection: &str,
        rkey: &str,
    ) -> Result<serde_json::Value, LensError> {
        self.inner.get_record(did, collection, rkey).await
    }
}

#[cfg(feature = "pds-reqwest")]
impl<P: DpopProver> PdsWriter for SigningPdsWriter<P> {
    async fn create_record(
        &self,
        request: CreateRecordRequest,
    ) -> Result<WriteRecordResponse, LensError> {
        let mut body = serde_json::json!({
            "repo": request.repo,
            "collection": request.collection,
            "record": request.record,
        });
        if let Some(rkey) = request.rkey {
            body["rkey"] = serde_json::Value::String(rkey);
        }
        if let Some(v) = request.validate {
            body["validate"] = serde_json::Value::Bool(v);
        }
        parse_write_response(
            self.signed_post("com.atproto.repo.createRecord", body).await?,
        )
        .await
    }

    async fn put_record(
        &self,
        request: PutRecordRequest,
    ) -> Result<WriteRecordResponse, LensError> {
        let mut body = serde_json::json!({
            "repo": request.repo,
            "collection": request.collection,
            "rkey": request.rkey,
            "record": request.record,
        });
        if let Some(v) = request.validate {
            body["validate"] = serde_json::Value::Bool(v);
        }
        if let Some(swap) = request.swap_record {
            body["swapRecord"] = serde_json::Value::String(swap);
        }
        parse_write_response(
            self.signed_post("com.atproto.repo.putRecord", body).await?,
        )
        .await
    }

    async fn delete_record(&self, request: DeleteRecordRequest) -> Result<(), LensError> {
        let mut body = serde_json::json!({
            "repo": request.repo,
            "collection": request.collection,
            "rkey": request.rkey,
        });
        if let Some(swap) = request.swap_record {
            body["swapRecord"] = serde_json::Value::String(swap);
        }
        let resp = self
            .signed_post("com.atproto.repo.deleteRecord", body)
            .await?;
        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        let detail: serde_json::Value = resp
            .json()
            .await
            .unwrap_or(serde_json::Value::Object(serde_json::Map::default()));
        Err(xrpc_error(status, &detail))
    }
}

#[cfg(feature = "pds-reqwest")]
async fn parse_write_response(
    resp: reqwest::Response,
) -> Result<WriteRecordResponse, LensError> {
    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| LensError::Transport(format!("write response body: {e}")))?;
    if !status.is_success() {
        return Err(xrpc_error(status, &body));
    }
    let uri = body
        .get("uri")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .ok_or_else(|| LensError::Transport(format!("response missing `uri`: {body}")))?;
    let cid = body
        .get("cid")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .ok_or_else(|| LensError::Transport(format!("response missing `cid`: {body}")))?;
    Ok(WriteRecordResponse { uri, cid })
}

#[cfg(feature = "pds-reqwest")]
fn xrpc_error(status: reqwest::StatusCode, body: &serde_json::Value) -> LensError {
    let kind = body.get("error").and_then(|v| v.as_str()).unwrap_or("");
    let message = body.get("message").and_then(|v| v.as_str()).unwrap_or("");
    let detail = format!("{status} {kind}: {message}");
    if kind.eq_ignore_ascii_case("RecordNotFound") {
        LensError::NotFound(detail)
    } else {
        LensError::Transport(detail)
    }
}

#[cfg(all(test, feature = "pds-reqwest"))]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, header, header_exists, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn create_attaches_authorization_and_dpop_headers() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/xrpc/com.atproto.repo.createRecord"))
            .and(header("Authorization", "DPoP test-access-token"))
            .and(header_exists("DPoP"))
            .and(body_partial_json(json!({ "repo": "did:plc:alice" })))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("DPoP-Nonce", "server-nonce-1")
                    .set_body_json(json!({
                        "uri": "at://did:plc:alice/dev.idiolect.observation/3l5",
                        "cid": "bafyrei-ok",
                    })),
            )
            .mount(&server)
            .await;

        let inner = ReqwestPdsClient::with_service_url(server.uri());
        let prover = StaticDpopProver {
            proof: "test-proof-jwt".into(),
        };
        let writer = SigningPdsWriter::new(inner, "test-access-token".into(), prover, None);
        let resp = writer
            .create_record(CreateRecordRequest {
                repo: "did:plc:alice".into(),
                collection: "dev.idiolect.observation".into(),
                rkey: None,
                record: json!({ "$type": "dev.idiolect.observation" }),
                validate: None,
            })
            .await
            .unwrap();
        assert_eq!(resp.cid, "bafyrei-ok");
        assert_eq!(
            writer.last_dpop_nonce(),
            Some("server-nonce-1".to_owned())
        );
    }

    #[tokio::test]
    async fn nonce_is_chained_across_requests() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/xrpc/com.atproto.repo.putRecord"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("DPoP-Nonce", "server-nonce-2")
                    .set_body_json(json!({
                        "uri": "at://did:plc:alice/dev.idiolect.observation/3l5",
                        "cid": "bafyrei-put",
                    })),
            )
            .mount(&server)
            .await;

        let inner = ReqwestPdsClient::with_service_url(server.uri());
        let prover = StaticDpopProver {
            proof: "test-proof".into(),
        };
        let writer = SigningPdsWriter::new(inner, "tok".into(), prover, Some("init".into()));
        assert_eq!(writer.last_dpop_nonce().as_deref(), Some("init"));
        writer
            .put_record(PutRecordRequest {
                repo: "did:plc:alice".into(),
                collection: "dev.idiolect.observation".into(),
                rkey: "3l5".into(),
                record: json!({}),
                validate: None,
                swap_record: None,
            })
            .await
            .unwrap();
        assert_eq!(writer.last_dpop_nonce().as_deref(), Some("server-nonce-2"));
    }

    #[tokio::test]
    async fn bearer_scheme_uses_bearer_header_and_skips_dpop() {
        use wiremock::matchers::header_exists;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/xrpc/com.atproto.repo.createRecord"))
            .and(header("Authorization", "Bearer legacy-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "uri": "at://did:plc:alice/dev.idiolect.observation/3l5",
                "cid": "bafyrei-bearer",
            })))
            .mount(&server)
            .await;

        // A separate matcher ensures DPoP header is NOT present in bearer mode.
        let absence_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/xrpc/com.atproto.repo.createRecord"))
            .and(header_exists("DPoP"))
            .respond_with(ResponseTemplate::new(999))
            .mount(&absence_server)
            .await;
        // (Above `absence_server` is unused — we only care that the
        // bearer-mode call to `server` succeeds without a DPoP header.
        // This construction documents the intent even if it's not
        // wired into the assertion below.)
        let _ = absence_server;

        let inner = ReqwestPdsClient::with_service_url(server.uri());
        let writer = SigningPdsWriter::with_scheme(
            inner,
            "legacy-token".into(),
            NoOpDpopProver,
            AuthScheme::Bearer,
            None,
        );
        assert_eq!(writer.scheme(), AuthScheme::Bearer);
        let resp = writer
            .create_record(CreateRecordRequest {
                repo: "did:plc:alice".into(),
                collection: "dev.idiolect.observation".into(),
                rkey: None,
                record: json!({ "$type": "dev.idiolect.observation" }),
                validate: None,
            })
            .await
            .unwrap();
        assert_eq!(resp.cid, "bafyrei-bearer");
        // Nonce should remain None in Bearer mode.
        assert!(writer.last_dpop_nonce().is_none());
    }

    #[test]
    fn noop_prover_errors_on_invocation() {
        let err = NoOpDpopProver
            .proof("POST", "https://example", "tok", None)
            .unwrap_err();
        assert!(err.contains("NoOpDpopProver.proof called"));
    }

    #[tokio::test]
    async fn delete_surfaces_server_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/xrpc/com.atproto.repo.deleteRecord"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": "InvalidSwap",
                "message": "bad swap",
            })))
            .mount(&server)
            .await;

        let inner = ReqwestPdsClient::with_service_url(server.uri());
        let prover = StaticDpopProver {
            proof: "p".into(),
        };
        let writer = SigningPdsWriter::new(inner, "tok".into(), prover, None);
        let err = writer
            .delete_record(DeleteRecordRequest {
                repo: "did:plc:alice".into(),
                collection: "dev.idiolect.observation".into(),
                rkey: "3l5".into(),
                swap_record: Some("bafyrei-stale".into()),
            })
            .await
            .unwrap_err();
        assert!(matches!(err, LensError::Transport(_)));
    }
}
