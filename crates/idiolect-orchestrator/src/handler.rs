//! [`RecordHandler`] that folds firehose commits into a [`Catalog`].
//!
//! One shared `Arc<Mutex<Catalog>>` is handed to this handler and to
//! the query surface. Commits advance the catalog; queries read it.
//! The handler does not gate or rank records: P3 is upheld elsewhere
//! (recommendations and verifications are records, not registrations).

use std::sync::{Arc, Mutex};

use idiolect_indexer::{IndexerError, IndexerEvent, IndexerAction, RecordHandler};

use crate::catalog::Catalog;

/// Optional persistent-store hook invoked alongside the in-memory
/// [`Catalog`] on every upsert / remove. Exists so the sqlite-backed
/// store (under `catalog-sqlite`) can mirror ingest without the
/// handler knowing anything concrete about the backend.
pub trait CatalogPersist: Send + Sync {
    /// Mirror an upsert to the persistent layer.
    ///
    /// # Errors
    ///
    /// Implementations return an [`IndexerError::Handler`]-shaped
    /// string when the persist fails; the handler then halts the loop
    /// so the operator notices.
    fn persist_upsert(
        &self,
        uri: &str,
        author: &str,
        rev: &str,
        record: &idiolect_records::AnyRecord,
    ) -> Result<(), String>;

    /// Mirror a delete to the persistent layer.
    ///
    /// # Errors
    ///
    /// See [`persist_upsert`](Self::persist_upsert).
    fn persist_remove(&self, uri: &str) -> Result<(), String>;
}

/// Record handler that maintains the catalog.
pub struct CatalogHandler {
    /// Shared catalog. Callers hold `Arc::clone(&handler.catalog())`
    /// to run queries concurrently with ingest.
    catalog: Arc<Mutex<Catalog>>,
    /// Optional persistence mirror.
    persist: Option<Arc<dyn CatalogPersist>>,
}

impl CatalogHandler {
    /// Construct a handler over a fresh empty catalog.
    #[must_use]
    pub fn new() -> Self {
        Self {
            catalog: Arc::new(Mutex::new(Catalog::new())),
            persist: None,
        }
    }

    /// Construct a handler around an externally owned catalog.
    ///
    /// Use this when the caller wants to seed the catalog with fixture
    /// data (tests, backfill from another store) before driving the
    /// firehose.
    #[must_use]
    pub const fn with_catalog(catalog: Arc<Mutex<Catalog>>) -> Self {
        Self {
            catalog,
            persist: None,
        }
    }

    /// Attach a persistence mirror. Every ingest is mirrored to the
    /// supplied store before the in-memory catalog commits; a store
    /// error halts the indexer loop.
    #[must_use]
    pub fn with_persist(mut self, persist: Arc<dyn CatalogPersist>) -> Self {
        self.persist = Some(persist);
        self
    }

    /// Clone the shared catalog handle.
    #[must_use]
    pub fn catalog(&self) -> Arc<Mutex<Catalog>> {
        Arc::clone(&self.catalog)
    }
}

impl Default for CatalogHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordHandler for CatalogHandler {
    async fn handle(&self, event: &IndexerEvent) -> Result<(), IndexerError> {
        let uri = event.at_uri();
        {
            let mut catalog = self
                .catalog
                .lock()
                .map_err(|e| IndexerError::Handler(format!("catalog mutex poisoned: {e}")))?;

            match (event.action, &event.record) {
                (IndexerAction::Delete, _) => {
                    if let Some(p) = &self.persist {
                        p.persist_remove(&uri).map_err(IndexerError::Handler)?;
                    }
                    catalog.remove(&uri);
                }
                (_, Some(record)) => {
                    if let Some(p) = &self.persist {
                        p.persist_upsert(&uri, &event.did, &event.rev, record)
                            .map_err(IndexerError::Handler)?;
                    }
                    catalog.upsert(uri, event.did.clone(), event.rev.clone(), record.clone());
                }
                (_, None) => {
                    // create/update with no decoded record is an odd case
                    // the indexer would normally have rejected upstream.
                    // Silently ignore rather than halt.
                }
            }
        }
        Ok(())
    }
}
