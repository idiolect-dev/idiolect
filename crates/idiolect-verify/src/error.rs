//! Errors raised by the verification runners.

use idiolect_lens::LensError;

/// Errors a [`VerificationRunner`](crate::VerificationRunner) can
/// raise.
#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    /// The underlying lens runtime (resolver, schema loader, apply)
    /// raised an error.
    #[error(transparent)]
    Lens(#[from] LensError),

    /// The input was malformed for the runner (e.g. empty corpus,
    /// unreadable proof artifact).
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// The runner could not reach a clear verdict. Distinct from
    /// `Holds` / `Falsified`: the verification record should be
    /// emitted with `result=inconclusive`.
    #[error("inconclusive: {0}")]
    Inconclusive(String),
}

/// Convenience alias.
pub type VerifyResult<T> = Result<T, VerifyError>;
