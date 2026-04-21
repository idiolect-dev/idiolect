//! Errors raised by the migration tooling.

use idiolect_lens::LensError;

/// Top-level error for operations in this crate.
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    /// The lens runtime raised an error during record translation.
    #[error(transparent)]
    Lens(#[from] LensError),

    /// Planning failed.
    #[error(transparent)]
    Planner(#[from] PlannerError),

    /// An input was malformed.
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

/// Convenience alias.
pub type MigrateResult<T> = Result<T, MigrateError>;

/// Errors raised by [`plan_auto`](crate::plan_auto).
#[derive(Debug, thiserror::Error)]
pub enum PlannerError {
    /// The diff contains at least one breaking change that the
    /// auto-planner cannot synthesize. The `Vec<String>` carries one
    /// entry per non-auto breaking change.
    #[error("{} breaking change(s) are not auto-derivable: {}", .0.len(), .0.join(", "))]
    NotAutoDerivable(Vec<String>),

    /// The schemas are identical; no migration is required.
    #[error("schemas are identical; no migration needed")]
    NoChange,

    /// The schemas differ only in non-breaking ways; callers should
    /// continue reading records under the old schema without
    /// migration.
    #[error("only non-breaking changes; no migration needed")]
    OnlyNonBreaking,
}
