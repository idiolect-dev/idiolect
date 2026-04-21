//! Errors raised by identity resolvers.

use crate::did::DidError;

/// Errors raised by an [`IdentityResolver`](crate::IdentityResolver)
/// during DID resolution.
#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    /// The backend returned `NotFound` (or equivalent) for this DID.
    /// Includes the raw DID string for diagnostics.
    #[error("identity not found: {0}")]
    NotFound(String),

    /// The DID document was fetched but had no `#atproto_pds` service
    /// entry.
    #[error("no atproto_pds service endpoint in document for {0}")]
    NoPdsEndpoint(String),

    /// The DID document was fetched but had no `at://`-shaped handle
    /// in its `alsoKnownAs`.
    #[error("no handle in document for {0}")]
    NoHandle(String),

    /// Backend-specific transport or decode failure. The inner string
    /// is implementation-defined; callers should surface it to
    /// operators but not to end users.
    #[error("identity transport: {0}")]
    Transport(String),

    /// The resolver received a response but could not parse it as a
    /// DID document.
    #[error("invalid DID document: {0}")]
    InvalidDocument(String),

    /// Upstream surfaced a malformed DID. Reified because callers may
    /// want to retry with a different DID, not just log.
    #[error("malformed DID: {0}")]
    MalformedDid(#[from] DidError),
}
