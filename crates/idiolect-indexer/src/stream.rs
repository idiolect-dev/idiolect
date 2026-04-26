//! The [`EventStream`] boundary — where firehose events come from.
//!
//! The indexer's orchestrator pulls from this trait; any implementation
//! that can hand back `(action, did, rev, collection, rkey, body)`
//! tuples in firehose order satisfies it. Ships with
//! [`InMemoryEventStream`] for fixtures and tests; the tapped-backed
//! adapter lives behind the `firehose-tapped` feature in
//! [`crate::tapped`].

use std::collections::VecDeque;

use idiolect_records::Nsid;

use crate::error::IndexerError;
use crate::event::IndexerAction;

/// A single firehose commit as seen by the stream layer, before the
/// indexer decodes the record body.
///
/// Equivalent to `tapped::RecordEvent` but without the lifetime on
/// the record body: the body is materialized as an owned
/// `serde_json::Value` so the indexer does not hold a reference into
/// the stream's buffer across handler awaits.
#[derive(Debug, Clone)]
pub struct RawEvent {
    /// Sequence id assigned by the stream implementation. Must be
    /// monotonically non-decreasing per connection.
    pub seq: u64,
    /// True if from the live firehose; false for backfill / resync.
    pub live: bool,
    /// DID of the repo that produced this commit.
    pub did: String,
    /// Repository revision the commit advanced to (TID).
    pub rev: String,
    /// Collection NSID, e.g. `dev.idiolect.encounter`.
    pub collection: Nsid,
    /// Record key within the collection.
    pub rkey: String,
    /// Commit action.
    pub action: IndexerAction,
    /// CID of the record body. `None` for delete events.
    pub cid: Option<String>,
    /// The record body json. `None` for delete events or when the
    /// stream could not parse the embedded body.
    pub body: Option<serde_json::Value>,
}

/// Async source of firehose commits.
///
/// Implementations return `Ok(Some(event))` on success, `Ok(None)`
/// when the stream is cleanly exhausted (useful for bounded mocks),
/// and [`IndexerError::Stream`] for transport-level failures.
///
/// Implementations are `Send + Sync` so the orchestrator can share
/// them across tokio tasks.
#[allow(async_fn_in_trait)]
pub trait EventStream: Send + Sync {
    /// Await the next firehose event.
    ///
    /// # Errors
    ///
    /// Returns [`IndexerError::Stream`] on transport errors.
    async fn next_event(&mut self) -> Result<Option<RawEvent>, IndexerError>;
}

/// `VecDeque`-backed [`EventStream`] for tests and offline fixtures.
///
/// Pre-seed events with [`InMemoryEventStream::push`] or from an
/// iterator; [`EventStream::next_event`] drains the queue in push
/// order and returns `Ok(None)` once it is empty.
#[derive(Debug, Default)]
pub struct InMemoryEventStream {
    /// The internal queue; events are drained in push order.
    queue: VecDeque<RawEvent>,
}

impl InMemoryEventStream {
    /// Construct an empty stream.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push one event onto the tail of the queue.
    pub fn push(&mut self, event: RawEvent) {
        self.queue.push_back(event);
    }

    /// Extend the queue with an iterator of events.
    pub fn extend<I>(&mut self, events: I)
    where
        I: IntoIterator<Item = RawEvent>,
    {
        self.queue.extend(events);
    }

    /// Number of events waiting to be consumed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Whether the queue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

impl EventStream for InMemoryEventStream {
    async fn next_event(&mut self) -> Result<Option<RawEvent>, IndexerError> {
        Ok(self.queue.pop_front())
    }
}
