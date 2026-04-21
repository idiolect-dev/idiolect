//! DID-rooted composition helpers.
//!
//! Every caller that authors records under a specific DID ends up
//! writing the same four-step chain:
//!
//! 1. parse the DID,
//! 2. resolve it through an [`IdentityResolver`] to the repo's PDS
//!    base URL,
//! 3. wrap that URL in a [`ReqwestPdsClient`], optionally
//!    pre-configured with auth headers,
//! 4. hand the client to a [`RecordPublisher`] pinned to the DID.
//!
//! [`publisher_for_did`] does the whole chain in one call.
//! [`fetcher_for_did`] is the read-side twin.
//!
//! Callers who need authenticated writes build the `reqwest::Client`
//! themselves (adding bearer / `DPoP` headers) and pass it to
//! [`publisher_for_did_with_client`].
//!
//! Feature-gated under `pds-resolve` (which pulls in `pds-reqwest`
//! and `idiolect-identity`).

use idiolect_identity::{Did, IdentityResolver};

use crate::error::LensError;
use crate::fetcher::RecordFetcher;
use crate::pds_reqwest::ReqwestPdsClient;
use crate::publisher::RecordPublisher;

/// Resolve `did` to a PDS base URL and wrap a fresh
/// [`ReqwestPdsClient`] in a [`RecordPublisher`] pinned to the DID.
///
/// Uses a default reqwest client (rustls, no auth). For authenticated
/// flows, build the client via [`reqwest::Client::builder`] with
/// bearer-token default headers and call
/// [`publisher_for_did_with_client`] instead.
///
/// # Errors
///
/// - [`LensError::Transport`] wraps any underlying
///   [`IdentityError`](idiolect_identity::IdentityError) (`NotFound`,
///   `NoPdsEndpoint`, Transport, …). The original DID and identity
///   error message survive in the transport string so operators can
///   diagnose from a log line.
pub async fn publisher_for_did<R: IdentityResolver>(
    resolver: &R,
    did: &Did,
) -> Result<RecordPublisher<ReqwestPdsClient>, LensError> {
    publisher_for_did_with_client(resolver, did, reqwest::Client::new()).await
}

/// Same as [`publisher_for_did`], but uses a caller-supplied
/// [`reqwest::Client`] — the canonical way to attach auth headers.
///
/// # Errors
///
/// See [`publisher_for_did`].
pub async fn publisher_for_did_with_client<R: IdentityResolver>(
    resolver: &R,
    did: &Did,
    http: reqwest::Client,
) -> Result<RecordPublisher<ReqwestPdsClient>, LensError> {
    let pds_url = resolve_pds(resolver, did).await?;
    let client = ReqwestPdsClient::with_client(pds_url, http);
    Ok(RecordPublisher::new(client, did.as_str().to_owned()))
}

/// Read-side twin: build a [`RecordFetcher`] pointed at the DID's PDS.
///
/// # Errors
///
/// See [`publisher_for_did`].
pub async fn fetcher_for_did<R: IdentityResolver>(
    resolver: &R,
    did: &Did,
) -> Result<RecordFetcher<ReqwestPdsClient>, LensError> {
    let pds_url = resolve_pds(resolver, did).await?;
    let client = ReqwestPdsClient::with_service_url(pds_url);
    Ok(RecordFetcher::new(client))
}

async fn resolve_pds<R: IdentityResolver>(
    resolver: &R,
    did: &Did,
) -> Result<String, LensError> {
    resolver
        .resolve_pds_url(did)
        .await
        .map_err(|e| LensError::Transport(format!("resolve PDS for {did}: {e}")))
}
