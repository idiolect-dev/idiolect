//! Sqlite-backed persistent catalog store.
//!
//! The orchestrator's in-memory [`Catalog`] is fast and correct but
//! loses all state on restart. `SqliteCatalogStore` persists every
//! upsert/remove so a restarted orchestrator re-loads its view in one
//! query and keeps advancing the firehose from the committed cursor.
//!
//! Schema (one table, one row per record):
//!
//! ```sql
//! CREATE TABLE records (
//!     uri        TEXT PRIMARY KEY NOT NULL,   -- at-uri of the record
//!     kind       TEXT NOT NULL,               -- "adapter" | "bounty" | ...
//!     author     TEXT NOT NULL,               -- DID of the repo
//!     rev        TEXT NOT NULL,               -- commit rev
//!     body_json  TEXT NOT NULL                -- serde_json::to_string(record)
//! );
//! CREATE INDEX idx_records_kind ON records(kind);
//! ```
//!
//! The kind string maps to `AnyRecord` variants so the loader can
//! re-discriminate on startup. `body_json` is the `AnyRecord`'s inner
//! value serialized via serde.
//!
//! Feature-gated under `catalog-sqlite`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use idiolect_records::AnyRecord;
use rusqlite::{Connection, params};

use crate::catalog::Catalog;
use crate::error::{OrchestratorError, OrchestratorResult};

/// Persistent catalog store backed by a sqlite database.
pub struct SqliteCatalogStore {
    /// Path to the sqlite file; retained for diagnostics.
    path: PathBuf,
    /// Serialized connection.
    conn: Mutex<Connection>,
}

impl std::fmt::Debug for SqliteCatalogStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteCatalogStore")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl SqliteCatalogStore {
    /// Open (or create) the catalog database at `path` and ensure the
    /// `records` table + `kind` index exist.
    ///
    /// Configures WAL journaling and `normal` synchronous, matching
    /// the indexer's cursor-store defaults.
    ///
    /// # Errors
    ///
    /// Returns [`OrchestratorError::Ingest`] on schema / open failure.
    pub fn open<P: AsRef<Path>>(path: P) -> OrchestratorResult<Self> {
        let path = path.as_ref().to_path_buf();
        let conn = Connection::open(&path)
            .map_err(|e| OrchestratorError::Ingest(format!("open sqlite {}: {e}", path.display())))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| OrchestratorError::Ingest(format!("pragma journal_mode: {e}")))?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|e| OrchestratorError::Ingest(format!("pragma synchronous: {e}")))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS records (
                uri        TEXT PRIMARY KEY NOT NULL,
                kind       TEXT NOT NULL,
                author     TEXT NOT NULL,
                rev        TEXT NOT NULL,
                body_json  TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_records_kind ON records(kind);",
        )
        .map_err(|e| OrchestratorError::Ingest(format!("create schema: {e}")))?;
        Ok(Self {
            path,
            conn: Mutex::new(conn),
        })
    }

    /// Backing file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Persist an upsert.
    ///
    /// # Errors
    ///
    /// Returns [`OrchestratorError::Ingest`] on serialization or sqlite
    /// failure.
    pub fn upsert(
        &self,
        uri: &str,
        author: &str,
        rev: &str,
        record: &AnyRecord,
    ) -> OrchestratorResult<()> {
        let kind = kind_tag(record);
        let body = serialize_record_body(record)?;
        let conn = self
            .conn
            .lock()
            .map_err(|e| OrchestratorError::Ingest(format!("sqlite mutex poisoned: {e}")))?;
        conn.execute(
            "INSERT INTO records (uri, kind, author, rev, body_json)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(uri) DO UPDATE SET
                kind = excluded.kind,
                author = excluded.author,
                rev = excluded.rev,
                body_json = excluded.body_json",
            params![uri, kind, author, rev, body],
        )
        .map_err(|e| OrchestratorError::Ingest(format!("upsert {uri}: {e}")))?;
        Ok(())
    }

    /// Persist a delete.
    ///
    /// # Errors
    ///
    /// Returns [`OrchestratorError::Ingest`] on sqlite failure.
    pub fn remove(&self, uri: &str) -> OrchestratorResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OrchestratorError::Ingest(format!("sqlite mutex poisoned: {e}")))?;
        conn.execute("DELETE FROM records WHERE uri = ?1", params![uri])
            .map_err(|e| OrchestratorError::Ingest(format!("delete {uri}: {e}")))?;
        Ok(())
    }

    /// Load the full catalog from disk into a fresh in-memory
    /// [`Catalog`].
    ///
    /// Used at startup to warm the in-memory view from the persistent
    /// store; downstream ingest writes to both layers.
    ///
    /// # Errors
    ///
    /// Returns [`OrchestratorError::Ingest`] on sqlite or
    /// deserialization failure. A row whose `kind` does not match a
    /// known variant is skipped with a tracing warn; future schema
    /// additions that predate this version of the code continue to load.
    pub fn load_catalog(&self) -> OrchestratorResult<Catalog> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OrchestratorError::Ingest(format!("sqlite mutex poisoned: {e}")))?;
        let mut stmt = conn
            .prepare("SELECT uri, kind, author, rev, body_json FROM records")
            .map_err(|e| OrchestratorError::Ingest(format!("prepare select: {e}")))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|e| OrchestratorError::Ingest(format!("query_map: {e}")))?;

        let mut catalog = Catalog::new();
        for row in rows {
            let (uri, kind, author, rev, body_json) = row
                .map_err(|e| OrchestratorError::Ingest(format!("row: {e}")))?;
            match deserialize_record_body(&kind, &body_json) {
                Ok(record) => catalog.upsert(uri, author, rev, record),
                Err(e) => {
                    tracing::warn!(
                        %uri,
                        %kind,
                        error = %e,
                        "skipping catalog row that failed to deserialize"
                    );
                }
            }
        }
        Ok(catalog)
    }

    /// Number of stored records (all kinds).
    ///
    /// # Errors
    ///
    /// Returns [`OrchestratorError::Ingest`] on sqlite failure.
    pub fn len(&self) -> OrchestratorResult<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OrchestratorError::Ingest(format!("sqlite mutex poisoned: {e}")))?;
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM records", [], |r| r.get(0))
            .map_err(|e| OrchestratorError::Ingest(format!("count: {e}")))?;
        #[allow(clippy::cast_sign_loss)]
        Ok(n.max(0) as usize)
    }
}

impl crate::handler::CatalogPersist for SqliteCatalogStore {
    fn persist_upsert(
        &self,
        uri: &str,
        author: &str,
        rev: &str,
        record: &AnyRecord,
    ) -> Result<(), String> {
        Self::upsert(self, uri, author, rev, record).map_err(|e| e.to_string())
    }

    fn persist_remove(&self, uri: &str) -> Result<(), String> {
        Self::remove(self, uri).map_err(|e| e.to_string())
    }
}

/// Short kind tag used for the `kind` column. Must match the variant
/// names in [`AnyRecord`] so `deserialize_record_body` can round-trip.
const fn kind_tag(record: &AnyRecord) -> &'static str {
    match record {
        AnyRecord::Adapter(_) => "adapter",
        AnyRecord::Bounty(_) => "bounty",
        AnyRecord::Community(_) => "community",
        AnyRecord::Dialect(_) => "dialect",
        AnyRecord::Recommendation(_) => "recommendation",
        AnyRecord::Verification(_) => "verification",
        AnyRecord::Encounter(_) => "encounter",
        AnyRecord::Correction(_) => "correction",
        AnyRecord::Observation(_) => "observation",
        AnyRecord::Retrospection(_) => "retrospection",
    }
}

/// Serialize just the record body (without a $type envelope).
fn serialize_record_body(record: &AnyRecord) -> OrchestratorResult<String> {
    let value = match record {
        AnyRecord::Adapter(r) => serde_json::to_string(r),
        AnyRecord::Bounty(r) => serde_json::to_string(r),
        AnyRecord::Community(r) => serde_json::to_string(r),
        AnyRecord::Dialect(r) => serde_json::to_string(r),
        AnyRecord::Recommendation(r) => serde_json::to_string(r),
        AnyRecord::Verification(r) => serde_json::to_string(r),
        AnyRecord::Encounter(r) => serde_json::to_string(r),
        AnyRecord::Correction(r) => serde_json::to_string(r),
        AnyRecord::Observation(r) => serde_json::to_string(r),
        AnyRecord::Retrospection(r) => serde_json::to_string(r),
    };
    value.map_err(|e| OrchestratorError::Ingest(format!("serialize: {e}")))
}

/// Deserialize a record body into the `AnyRecord` variant selected by
/// `kind`.
fn deserialize_record_body(kind: &str, body: &str) -> OrchestratorResult<AnyRecord> {
    fn from<R: idiolect_records::Record>(body: &str) -> OrchestratorResult<R> {
        serde_json::from_str(body).map_err(|e| OrchestratorError::Ingest(format!("deserialize: {e}")))
    }
    Ok(match kind {
        "adapter" => AnyRecord::Adapter(from(body)?),
        "bounty" => AnyRecord::Bounty(from(body)?),
        "community" => AnyRecord::Community(from(body)?),
        "dialect" => AnyRecord::Dialect(from(body)?),
        "recommendation" => AnyRecord::Recommendation(from(body)?),
        "verification" => AnyRecord::Verification(from(body)?),
        "encounter" => AnyRecord::Encounter(from(body)?),
        "correction" => AnyRecord::Correction(from(body)?),
        "observation" => AnyRecord::Observation(from(body)?),
        "retrospection" => AnyRecord::Retrospection(from(body)?),
        other => return Err(OrchestratorError::Ingest(format!("unknown kind `{other}`"))),
    })
}
