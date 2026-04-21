//! Error types for the indexer crate.
//!
//! The indexer loop pulls from three pluggable boundaries (stream,
//! handler, cursor store) and owns the dispatch of firehose commits
//! into strongly-typed records. Each boundary has its own failure
//! modes; [`IndexerError`] flattens them for the orchestrator.

use idiolect_records::DecodeError;

/// Every failure the indexer can raise while pulling events and
/// driving handlers.
#[derive(Debug, thiserror::Error)]
pub enum IndexerError {
    /// The [`EventStream`](crate::EventStream) produced an error
    /// pulling the next event. Transport-level failures bubble up
    /// here so the caller can decide whether to reconnect or exit.
    #[error("event stream error: {0}")]
    Stream(String),

    /// The cursor store failed to load or commit a cursor position.
    #[error("cursor store error: {0}")]
    Cursor(String),

    /// Decoding the record body json into an
    /// [`idiolect_records::AnyRecord`] failed. Includes unknown
    /// `dev.idiolect.*` nsids and serde type mismatches.
    #[error("record decode error: {0}")]
    Decode(#[from] DecodeError),

    /// A [`RecordHandler`](crate::RecordHandler) returned an error
    /// while processing a decoded record. The error message is
    /// handler-defined; by the time this variant is constructed the
    /// record has already been decoded and is valid.
    #[error("record handler error: {0}")]
    Handler(String),

    /// The record body json was missing or malformed on the
    /// firehose event. Tap produces this for delete actions (no
    /// record body) and for events whose embedded record failed to
    /// round-trip through `serde_json::RawValue`.
    #[error("missing or malformed record body: {0}")]
    MissingBody(String),
}
