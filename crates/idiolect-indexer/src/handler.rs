//! The [`RecordHandler`] boundary — what an appview does with a
//! decoded record.
//!
//! The indexer calls exactly one handler method per decoded commit.
//! Implementations dispatch by
//! [`IndexerEvent::record`](crate::IndexerEvent) variant; the
//! family's `AnyRecord` enum ensures every record type has a match
//! arm, and a new record type added to the family shows up as a
//! missing arm at compile time.
//!
//! This crate ships a [`NoopRecordHandler`] for tests that only care
//! about the stream / decode pipeline.

use std::sync::atomic::{AtomicUsize, Ordering};

use idiolect_records::{IdiolectFamily, RecordFamily};

use crate::error::IndexerError;
use crate::event::{IndexerAction, IndexerEvent};

/// Appview entry point for processing a decoded firehose commit.
///
/// Generic over `F: RecordFamily`. Implementations typically
/// destructure [`IndexerEvent::record`](crate::IndexerEvent) to
/// dispatch on the record kind, then side-effect into storage or a
/// queue.
///
/// `Send + Sync` bounds match the other boundaries; the orchestrator
/// awaits handler calls sequentially per stream, but many deployments
/// fan out across storage workers.
#[allow(async_fn_in_trait)]
pub trait RecordHandler<F: RecordFamily = IdiolectFamily>: Send + Sync {
    /// Process one decoded commit.
    ///
    /// For delete actions, `event.record` is `None` and the handler
    /// should remove any indexed state keyed on
    /// `(did, collection, rkey)`.
    ///
    /// # Errors
    ///
    /// Return [`IndexerError::Handler`] with a descriptive message
    /// when a downstream write fails. The orchestrator does not
    /// retry automatically; retry policy is a deployment concern.
    async fn handle(&self, event: &IndexerEvent<F>) -> Result<(), IndexerError>;
}

/// A handler that counts events and drops them on the floor.
///
/// Useful as a null implementation in tests or as a baseline for
/// measuring stream throughput without storage overhead. Counts are
/// kept behind an [`AtomicUsize`], so the handler is cheap to share
/// across tasks via `&NoopRecordHandler`.
#[derive(Debug, Default)]
pub struct NoopRecordHandler {
    /// Total events observed by this handler.
    observed: AtomicUsize,
    /// Events whose action was `Delete`.
    deletes: AtomicUsize,
}

impl NoopRecordHandler {
    /// Construct a fresh handler with zero counts.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Total events observed (across all kinds and actions).
    #[must_use]
    pub fn observed(&self) -> usize {
        self.observed.load(Ordering::SeqCst)
    }

    /// Delete events observed.
    #[must_use]
    pub fn deletes(&self) -> usize {
        self.deletes.load(Ordering::SeqCst)
    }
}

/// Forward [`RecordHandler`] through a shared `Arc<T>`. Lets an
/// `Arc<dyn RecordHandler<F>>` or `Arc<MyHandler>` be handed to
/// `drive_indexer` (and any other consumer) without a manual delegate
/// impl.
impl<F: RecordFamily, T: RecordHandler<F> + ?Sized> RecordHandler<F> for std::sync::Arc<T> {
    async fn handle(&self, event: &IndexerEvent<F>) -> Result<(), IndexerError> {
        (**self).handle(event).await
    }
}

impl<F: RecordFamily> RecordHandler<F> for NoopRecordHandler {
    async fn handle(&self, event: &IndexerEvent<F>) -> Result<(), IndexerError> {
        self.observed.fetch_add(1, Ordering::SeqCst);

        if event.action == IndexerAction::Delete {
            self.deletes.fetch_add(1, Ordering::SeqCst);
        }

        Ok(())
    }
}
