//! Sqlite-backed [`CursorStore`](crate::CursorStore).
//!
//! One row per subscription id in a `cursors(subscription_id TEXT
//! PRIMARY KEY, seq INTEGER NOT NULL)` table. Commits are single
//! `INSERT OR REPLACE` statements inside WAL-mode sqlite, which gives
//! multi-process readers a consistent snapshot against a single
//! committing writer.
//!
//! Feature-gated under `cursor-sqlite`. The underlying `rusqlite` crate
//! uses bundled sqlite (`bundled` feature), so the crate builds on
//! platforms without system sqlite.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};

use crate::cursor::CursorStore;
use crate::error::IndexerError;

/// Single-process sqlite-backed cursor store.
///
/// Owns a [`rusqlite::Connection`] behind a `Mutex`. `Connection` is
/// not `Sync`, so the mutex is how the store satisfies the
/// `CursorStore: Send + Sync` bound. Operations are short (one UPSERT
/// or SELECT); contention is not expected to matter.
pub struct SqliteCursorStore {
    /// Path to the sqlite file. Retained for diagnostics.
    path: PathBuf,
    /// Serialized connection to the sqlite file.
    conn: Mutex<Connection>,
}

impl std::fmt::Debug for SqliteCursorStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteCursorStore")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl SqliteCursorStore {
    /// Open or create the sqlite database at `path` and ensure the
    /// `cursors` table exists.
    ///
    /// Configures WAL journaling and a `normal` synchronous level to
    /// trade a little crash-durability for commit throughput —
    /// appropriate for cursor state, where losing a single commit
    /// means re-replaying a few firehose events at worst.
    ///
    /// # Errors
    ///
    /// Returns [`IndexerError::Cursor`] if the sqlite file cannot be
    /// opened or the schema migration fails.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, IndexerError> {
        let path = path.as_ref().to_path_buf();
        let conn = Connection::open(&path)
            .map_err(|e| IndexerError::Cursor(format!("open sqlite {}: {e}", path.display())))?;
        // WAL: durable commits with less write amplification; `normal`
        // sync: fsync on commit but not on every page write.
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| IndexerError::Cursor(format!("pragma journal_mode: {e}")))?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|e| IndexerError::Cursor(format!("pragma synchronous: {e}")))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS cursors (
                subscription_id TEXT PRIMARY KEY NOT NULL,
                seq INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| IndexerError::Cursor(format!("create cursors table: {e}")))?;
        Ok(Self {
            path,
            conn: Mutex::new(conn),
        })
    }

    /// Borrow the backing file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl CursorStore for SqliteCursorStore {
    async fn load(&self, subscription_id: &str) -> Result<Option<u64>, IndexerError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| IndexerError::Cursor(format!("sqlite mutex poisoned: {e}")))?;
        let seq: Option<i64> = conn
            .query_row(
                "SELECT seq FROM cursors WHERE subscription_id = ?1",
                params![subscription_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| IndexerError::Cursor(format!("select cursor: {e}")))?;
        // sqlite integers are i64; cast-safe because we only insert u64
        // values that actually fit. Negative values in the table are a
        // data-corruption signal, which we surface as an error rather
        // than silently re-interpreting as u64.
        match seq {
            Some(v) if v < 0 => Err(IndexerError::Cursor(format!(
                "negative cursor in sqlite for {subscription_id}: {v}"
            ))),
            #[allow(clippy::cast_sign_loss)]
            Some(v) => Ok(Some(v as u64)),
            None => Ok(None),
        }
    }

    async fn commit(&self, subscription_id: &str, seq: u64) -> Result<(), IndexerError> {
        if seq > i64::MAX as u64 {
            return Err(IndexerError::Cursor(format!(
                "cursor seq {seq} exceeds sqlite i64 range"
            )));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|e| IndexerError::Cursor(format!("sqlite mutex poisoned: {e}")))?;
        #[allow(clippy::cast_possible_wrap)]
        conn.execute(
            "INSERT INTO cursors (subscription_id, seq) VALUES (?1, ?2)
             ON CONFLICT(subscription_id) DO UPDATE SET seq = excluded.seq",
            params![subscription_id, seq as i64],
        )
        .map_err(|e| IndexerError::Cursor(format!("upsert cursor: {e}")))?;
        Ok(())
    }

    async fn list(&self) -> Result<Vec<(String, u64)>, IndexerError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| IndexerError::Cursor(format!("sqlite mutex poisoned: {e}")))?;
        let mut stmt = conn
            .prepare("SELECT subscription_id, seq FROM cursors ORDER BY subscription_id")
            .map_err(|e| IndexerError::Cursor(format!("prepare list: {e}")))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| IndexerError::Cursor(format!("query list: {e}")))?;
        let mut out = Vec::new();
        for r in rows {
            let (id, seq) = r.map_err(|e| IndexerError::Cursor(format!("row: {e}")))?;
            if seq < 0 {
                return Err(IndexerError::Cursor(format!(
                    "negative cursor in sqlite for {id}: {seq}"
                )));
            }
            #[allow(clippy::cast_sign_loss)]
            out.push((id, seq as u64));
        }
        Ok(out)
    }
}
