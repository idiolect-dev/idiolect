//! `refresh_if_needed` — the standard "look up a session, refresh if
//! the access token is past expiry, persist the refreshed value, hand
//! the live session back" helper.
//!
//! The refresh HTTP call itself lives in `atrium-oauth-client`; this
//! module owns the storage and timing decision around it. Callers
//! who want to drive their own refresh path read
//! [`OAuthSession::needs_refresh`] / [`OAuthSession::is_expired`]
//! directly.

use crate::session::{OAuthSession, SessionError};
use crate::store::{OAuthTokenStore, StoreError};

/// Wall-clock minus this many seconds is the "now" threshold for the
/// refresh decision: if the access token's `expires_at` is within
/// this window we refresh ahead of actual expiry. Sixty seconds is
/// a reasonable buffer against clock skew and in-flight requests.
pub const DEFAULT_REFRESH_THRESHOLD_SECS: i64 = 60;

/// Error surface for [`refresh_if_needed`]. Boxes the per-source
/// errors so callers can match without naming every source crate.
#[derive(Debug, thiserror::Error)]
pub enum RefreshError {
    /// The token store failed reading or writing the session.
    #[error("session store error: {0}")]
    Store(#[from] StoreError),
    /// The stored session was malformed (corrupt expiry timestamp,
    /// missing fields).
    #[error("session shape error: {0}")]
    Session(#[from] SessionError),
    /// No session is stored for the given `did`.
    #[error("no session stored for {0}")]
    NoSession(String),
    /// The caller-supplied refresh closure failed.
    #[error("refresh callback failed: {0}")]
    Callback(String),
}

/// Trait the caller implements to perform the actual refresh against
/// whatever transport (atrium-oauth-client, hand-rolled reqwest,
/// in-memory fake for tests) it has on hand.
///
/// Kept narrow so the `idiolect-oauth` crate stays
/// transport-agnostic. Concrete impls live in `idiolect-cli` or
/// downstream consumers.
#[allow(async_fn_in_trait)]
pub trait Refresher: Send + Sync {
    /// Given an expired session, return a fresh one with new
    /// `access_jwt`, `expires_at`, and (usually) `refresh_jwt`.
    async fn refresh(&self, session: &OAuthSession) -> Result<OAuthSession, RefreshError>;
}

/// Look up the session for `did`, decide whether it needs to refresh
/// based on the current wall clock plus a 60-second buffer, drive
/// `refresher` if so, persist the result, and return the live
/// session.
///
/// `store` is the persistence boundary; `refresher` is the transport.
///
/// # Errors
///
/// [`RefreshError::NoSession`] when no session is stored for the
/// DID; [`RefreshError::Store`] on backing-store failures;
/// [`RefreshError::Callback`] when the refresher's HTTP call fails;
/// [`RefreshError::Session`] when the stored session's timestamps
/// are malformed.
pub async fn refresh_if_needed<S, R>(
    store: &S,
    refresher: &R,
    did: &str,
) -> Result<OAuthSession, RefreshError>
where
    S: OAuthTokenStore,
    R: Refresher,
{
    let Some(session) = store.load(did).await? else {
        return Err(RefreshError::NoSession(did.to_owned()));
    };

    let now = time::OffsetDateTime::now_utc();
    let threshold = time::Duration::seconds(DEFAULT_REFRESH_THRESHOLD_SECS);
    if !session.needs_refresh(now, threshold) {
        return Ok(session);
    }

    let updated = refresher.refresh(&session).await?;
    store.save(&updated).await?;
    Ok(updated)
}
