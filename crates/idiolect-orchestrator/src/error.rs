//! Error types for the orchestrator crate.

use idiolect_indexer::IndexerError;

/// Every failure the orchestrator can raise while folding the firehose
/// into its catalog or answering queries.
#[derive(Debug, thiserror::Error)]
pub enum OrchestratorError {
    /// The underlying indexer layer raised an error. Transport
    /// failures, decode drift, and cursor-store issues all surface
    /// through here.
    #[error(transparent)]
    Indexer(#[from] IndexerError),

    /// A record the catalog tried to ingest is missing a field the
    /// lexicon marks required. Should only fire on non-lexicon-validated
    /// upstream traffic.
    #[error("catalog ingest: {0}")]
    Ingest(String),
}

/// Convenience alias.
pub type OrchestratorResult<T> = Result<T, OrchestratorError>;
