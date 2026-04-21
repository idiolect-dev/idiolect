//! The [`CursorStore`] boundary — where the firehose sequence number
//! lives across indexer restarts.
//!
//! The indexer commits a cursor after each handled live event so that
//! a restart can resume from the last processed commit instead of
//! replaying the relay's retention floor. Cursor commits do not
//! advance on backfill events: those are already replayed by tap
//! whenever a repo is resynced, so committing them would produce
//! misleading "we are caught up" telemetry.
//!
//! The shape is the same ephemeral-state adapter pattern used for
//! OAuth tokens and identity resolution: one narrow async trait with
//! `Send + Sync` bounds, an in-memory impl for tests, production
//! backends plugged in behind the same signature.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::error::IndexerError;

/// Durable store for firehose cursor positions.
///
/// One cursor per subscription id; the subscription id is whatever
/// identifies the connection (tapped instance url, jetstream endpoint,
/// self-hosted relay, etc.). The orchestrator treats
/// `Ok(None)` from `load` as "start from the beginning", which
/// implementations may interpret as "latest cursor the server offers"
/// for live streams.
#[allow(async_fn_in_trait)]
pub trait CursorStore: Send + Sync {
    /// Load the last committed cursor for `subscription_id`, if any.
    ///
    /// # Errors
    ///
    /// Returns [`IndexerError::Cursor`] when the backing store fails.
    async fn load(&self, subscription_id: &str) -> Result<Option<u64>, IndexerError>;

    /// Commit `seq` as the new cursor for `subscription_id`.
    ///
    /// # Errors
    ///
    /// Returns [`IndexerError::Cursor`] when the backing store fails.
    async fn commit(&self, subscription_id: &str, seq: u64) -> Result<(), IndexerError>;

    /// Enumerate every committed `(subscription_id, seq)` pair.
    ///
    /// Used by operator tooling (a `/v1/cursors` admin endpoint, a
    /// health-check that sanity-checks cursor freshness) and by
    /// multi-subscription indexers that want to display the cursor
    /// fleet at a glance. The default implementation returns an
    /// empty vec so custom backends can opt in without breaking
    /// existing callers; every shipped store overrides it with its
    /// native enumeration.
    ///
    /// # Errors
    ///
    /// Returns [`IndexerError::Cursor`] when the backing store fails.
    async fn list(&self) -> Result<Vec<(String, u64)>, IndexerError> {
        Ok(Vec::new())
    }
}

/// `HashMap`-backed [`CursorStore`] for fixtures and unit tests.
///
/// Commits are in-memory only and are lost across process restarts;
/// not suitable for production. A sqlite-backed impl is the usual
/// next step for single-node indexers; Postgres or Redis for HA.
#[derive(Debug, Default)]
pub struct InMemoryCursorStore {
    /// Cursor per subscription id, protected by a standard mutex.
    /// Lock holds are sub-microsecond — a simple `HashMap::insert`
    /// / `HashMap::get` — so a sync mutex inside an async trait
    /// method is the right trade for avoiding a tokio dependency on
    /// the default build.
    cursors: Mutex<HashMap<String, u64>>,
}

impl InMemoryCursorStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed the store with an initial cursor, e.g. for test replay.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex has been poisoned by a previous
    /// panic in another thread.
    pub fn insert(&self, subscription_id: &str, seq: u64) {
        let mut cursors = self.cursors.lock().expect("cursors mutex poisoned");
        cursors.insert(subscription_id.to_owned(), seq);
    }
}

/// Forward [`CursorStore`] through a shared `Arc<T>` so an `Arc<dyn
/// CursorStore>` (or any concrete `Arc<SqliteCursorStore>`, etc.) can
/// be passed anywhere a `CursorStore: Send + Sync` is expected. The
/// `?Sized` bound lets the impl cover trait objects directly.
impl<T: CursorStore + ?Sized> CursorStore for std::sync::Arc<T> {
    async fn load(&self, subscription_id: &str) -> Result<Option<u64>, IndexerError> {
        (**self).load(subscription_id).await
    }

    async fn commit(&self, subscription_id: &str, seq: u64) -> Result<(), IndexerError> {
        (**self).commit(subscription_id, seq).await
    }

    async fn list(&self) -> Result<Vec<(String, u64)>, IndexerError> {
        (**self).list().await
    }
}

impl CursorStore for InMemoryCursorStore {
    async fn load(&self, subscription_id: &str) -> Result<Option<u64>, IndexerError> {
        // scope the lock guard so clippy's significant-drop-tightening
        // sees the guard drop before the return point.
        let seq = {
            let cursors = self.cursors.lock().expect("cursors mutex poisoned");
            cursors.get(subscription_id).copied()
        };

        Ok(seq)
    }

    async fn commit(&self, subscription_id: &str, seq: u64) -> Result<(), IndexerError> {
        {
            let mut cursors = self.cursors.lock().expect("cursors mutex poisoned");
            cursors.insert(subscription_id.to_owned(), seq);
        }

        Ok(())
    }

    async fn list(&self) -> Result<Vec<(String, u64)>, IndexerError> {
        let cursors = self.cursors.lock().expect("cursors mutex poisoned");
        let mut out: Vec<(String, u64)> = cursors
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        // Deterministic ordering so tests can compare directly.
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }
}
