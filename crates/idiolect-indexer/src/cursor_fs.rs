//! Filesystem-backed [`CursorStore`](crate::CursorStore).
//!
//! Persists cursors as a single json file at a caller-chosen path.
//! Each commit rewrites the file atomically via a temp-file + rename
//! dance so a crash mid-write does not leave a half-written cursor.
//!
//! Intended for small single-node indexers. Concurrent writers against
//! the same file are not supported: the internal `Mutex` serializes
//! access within the process, but a second process writing the same
//! path will clobber. For multi-process deployments use
//! [`SqliteCursorStore`](crate::SqliteCursorStore) or roll your own.
//!
//! Feature-gated under `cursor-filesystem`.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::cursor::CursorStore;
use crate::error::IndexerError;

/// Cursor store persisted to a single json file.
#[derive(Debug)]
pub struct FilesystemCursorStore {
    /// Path the store writes to. The parent directory must exist.
    path: PathBuf,
    /// In-memory cache. Loaded from disk on construction; mutated on
    /// every commit and flushed back to disk atomically.
    cache: Mutex<HashMap<String, u64>>,
}

impl FilesystemCursorStore {
    /// Open (or create) a cursor store at `path`.
    ///
    /// If the file exists and is parseable, its contents are loaded as
    /// the initial state. If it does not exist, the store starts empty
    /// and the file is created on the first commit. Parent directories
    /// are NOT created — the caller is expected to have prepared them.
    ///
    /// # Errors
    ///
    /// Returns [`IndexerError::Cursor`] if the file exists but is not
    /// parseable as a `HashMap<String, u64>` json object.
    pub fn open<P: Into<PathBuf>>(path: P) -> Result<Self, IndexerError> {
        let path = path.into();
        let cache = if path.exists() {
            let bytes = fs::read(&path)
                .map_err(|e| IndexerError::Cursor(format!("read {}: {e}", path.display())))?;
            if bytes.is_empty() {
                HashMap::new()
            } else {
                serde_json::from_slice::<HashMap<String, u64>>(&bytes)
                    .map_err(|e| IndexerError::Cursor(format!("parse {}: {e}", path.display())))?
            }
        } else {
            HashMap::new()
        };
        Ok(Self {
            path,
            cache: Mutex::new(cache),
        })
    }

    /// Borrow the path this store is writing to.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Write the current cache to disk atomically.
    ///
    /// Strategy: serialize into a sibling temp file in the same
    /// directory, then `rename` over the target path. `rename` is
    /// atomic on posix for same-filesystem moves.
    fn flush(&self, cache: &HashMap<String, u64>) -> Result<(), IndexerError> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        // unique-enough temp name: pid + target filename stem.
        let tmp = parent.join(format!(
            ".{}.tmp.{}",
            self.path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("cursors"),
            std::process::id(),
        ));
        let bytes = serde_json::to_vec(cache)
            .map_err(|e| IndexerError::Cursor(format!("serialize cursors: {e}")))?;
        {
            let mut f = fs::File::create(&tmp)
                .map_err(|e| IndexerError::Cursor(format!("open temp {}: {e}", tmp.display())))?;
            f.write_all(&bytes)
                .map_err(|e| IndexerError::Cursor(format!("write temp {}: {e}", tmp.display())))?;
            f.sync_all()
                .map_err(|e| IndexerError::Cursor(format!("fsync temp {}: {e}", tmp.display())))?;
        }
        fs::rename(&tmp, &self.path).map_err(|e| {
            IndexerError::Cursor(format!(
                "rename {} -> {}: {e}",
                tmp.display(),
                self.path.display()
            ))
        })?;
        Ok(())
    }
}

impl CursorStore for FilesystemCursorStore {
    async fn load(&self, subscription_id: &str) -> Result<Option<u64>, IndexerError> {
        let cache = self
            .cache
            .lock()
            .map_err(|e| IndexerError::Cursor(format!("cache mutex poisoned: {e}")))?;
        Ok(cache.get(subscription_id).copied())
    }

    async fn commit(&self, subscription_id: &str, seq: u64) -> Result<(), IndexerError> {
        // single critical section: update the cache and flush under
        // the same guard so concurrent commits linearize.
        let mut cache = self
            .cache
            .lock()
            .map_err(|e| IndexerError::Cursor(format!("cache mutex poisoned: {e}")))?;
        cache.insert(subscription_id.to_owned(), seq);
        self.flush(&cache)?;
        Ok(())
    }

    async fn list(&self) -> Result<Vec<(String, u64)>, IndexerError> {
        let cache = self
            .cache
            .lock()
            .map_err(|e| IndexerError::Cursor(format!("cache mutex poisoned: {e}")))?;
        let mut out: Vec<(String, u64)> = cache.iter().map(|(k, v)| (k.clone(), *v)).collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }
}
