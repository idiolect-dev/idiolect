//! Error type surfaced by the observer runtime.
//!
//! Keeps the observer-specific failure modes (method errors,
//! publisher errors) apart from the firehose-consumer errors produced
//! by [`idiolect_indexer`]. The `#[from]` bridge for
//! [`idiolect_indexer::IndexerError`] keeps the top-level
//! [`crate::drive_observer`] orchestrator ergonomic: a handler error
//! surfaced by the indexer becomes an
//! [`ObserverError::Indexer`] without manual conversion.

use idiolect_indexer::IndexerError;
use thiserror::Error;

/// Fallible result alias used throughout the observer crate.
pub type ObserverResult<T> = Result<T, ObserverError>;

/// Everything the observer runtime can go wrong at.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ObserverError {
    /// An [`ObservationMethod`](crate::ObservationMethod) rejected an
    /// event it was asked to fold in, or produced an error during
    /// snapshotting. Carries a method-supplied message.
    #[error("observation method error: {0}")]
    Method(String),

    /// The [`ObservationPublisher`](crate::ObservationPublisher) failed
    /// to store or transmit an observation record. Carries a
    /// publisher-supplied message.
    #[error("observation publisher error: {0}")]
    Publisher(String),

    /// The indexer-level pipeline (stream, decode, cursor, or a
    /// [`RecordHandler`](idiolect_indexer::RecordHandler) we embed)
    /// surfaced an error. The observer does not retry automatically;
    /// the outer binary or supervisor decides the policy.
    #[error("indexer error: {0}")]
    Indexer(#[from] IndexerError),
}
