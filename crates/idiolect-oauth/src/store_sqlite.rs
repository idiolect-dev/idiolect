//! Sqlite-backed [`OAuthTokenStore`](crate::OAuthTokenStore).
//!
//! One row per session; body stored as serialized json. WAL journaling
//! plus a `normal` synchronous level match the cursor-store conventions
//! in `idiolect-indexer`.
//!
//! Feature-gated under `store-sqlite`. Uses bundled sqlite so no system
//! libsqlite is required.
//!
//! # Security
//!
//! The rows carry `accessJwt`, `refreshJwt`, and the `dpopPrivateKeyJwk`
//! fields. Callers are responsible for the sqlite file's filesystem
//! permissions; a multi-tenant server should restrict access to the
//! service account that owns the process.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};

use crate::session::OAuthSession;
use crate::store::{OAuthTokenStore, StoreError};

/// Sqlite-backed token store.
pub struct SqliteOAuthTokenStore {
    path: PathBuf,
    conn: Mutex<Connection>,
}

impl std::fmt::Debug for SqliteOAuthTokenStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteOAuthTokenStore")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl SqliteOAuthTokenStore {
    /// Open or create the sqlite database at `path` and ensure the
    /// `sessions` table exists.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Backend`] if the file cannot be opened
    /// or the schema creation fails.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        let conn = Connection::open(&path)
            .map_err(|e| StoreError::Backend(format!("open sqlite {}: {e}", path.display())))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| StoreError::Backend(format!("pragma journal_mode: {e}")))?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|e| StoreError::Backend(format!("pragma synchronous: {e}")))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sessions (
                did       TEXT PRIMARY KEY NOT NULL,
                body_json TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| StoreError::Backend(format!("create sessions table: {e}")))?;
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
}

impl OAuthTokenStore for SqliteOAuthTokenStore {
    async fn save(&self, session: &OAuthSession) -> Result<(), StoreError> {
        let body = serde_json::to_string(session)
            .map_err(|e| StoreError::Backend(format!("serialize session: {e}")))?;
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::Backend(format!("sqlite mutex poisoned: {e}")))?;
        conn.execute(
            "INSERT INTO sessions (did, body_json) VALUES (?1, ?2)
             ON CONFLICT(did) DO UPDATE SET body_json = excluded.body_json",
            params![session.did, body],
        )
        .map_err(|e| StoreError::Backend(format!("upsert session: {e}")))?;
        Ok(())
    }

    async fn load(&self, did: &str) -> Result<Option<OAuthSession>, StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::Backend(format!("sqlite mutex poisoned: {e}")))?;
        let row: Option<String> = conn
            .query_row(
                "SELECT body_json FROM sessions WHERE did = ?1",
                params![did],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| StoreError::Backend(format!("select session: {e}")))?;
        match row {
            Some(body) => {
                let session: OAuthSession = serde_json::from_str(&body)
                    .map_err(|e| StoreError::Backend(format!("deserialize {did}: {e}")))?;
                Ok(Some(session))
            }
            None => Ok(None),
        }
    }

    async fn delete(&self, did: &str) -> Result<(), StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::Backend(format!("sqlite mutex poisoned: {e}")))?;
        conn.execute("DELETE FROM sessions WHERE did = ?1", params![did])
            .map_err(|e| StoreError::Backend(format!("delete session: {e}")))?;
        Ok(())
    }

    async fn list_dids(&self) -> Result<Vec<String>, StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::Backend(format!("sqlite mutex poisoned: {e}")))?;
        let mut stmt = conn
            .prepare("SELECT did FROM sessions ORDER BY did")
            .map_err(|e| StoreError::Backend(format!("prepare list: {e}")))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| StoreError::Backend(format!("query list: {e}")))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| StoreError::Backend(format!("row: {e}")))?);
        }
        Ok(out)
    }
}
