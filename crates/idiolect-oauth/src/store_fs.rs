//! Filesystem-backed [`OAuthTokenStore`](crate::OAuthTokenStore).
//!
//! Stores each session as a json file under a caller-chosen directory.
//! One file per DID; the filename is the DID with any `:` or `/`
//! characters replaced so the result is a legal filename on the major
//! filesystems (`did:plc:abc` → `did_plc_abc.json`).
//!
//! Intended for single-tenant local daemons and development where a
//! full database is overkill. Concurrent writers across processes are
//! NOT supported: the store has no inter-process locking. Use
//! [`InMemoryOAuthTokenStore`](crate::InMemoryOAuthTokenStore) or a
//! real database for multi-tenant deployments.
//!
//! Feature-gated under `store-filesystem`.
//!
//! # Security
//!
//! The on-disk file contains `accessJwt`, `refreshJwt`, and the
//! `dpopPrivateKeyJwk`. The store sets `0o600` permissions on each
//! file on posix platforms so that the session is readable only by
//! the owning user. Callers are responsible for the directory's own
//! mode (`0o700` recommended).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::session::OAuthSession;
use crate::store::{OAuthTokenStore, StoreError};

/// Filesystem-backed token store.
///
/// Each session lives in `<dir>/<sanitized-did>.json`. The directory
/// must exist and be writable; the store does not create it.
#[derive(Debug)]
pub struct FilesystemOAuthTokenStore {
    /// Directory that holds one json file per session.
    dir: PathBuf,
    /// Serialize in-process writes so two concurrent `save`s for the
    /// same DID do not race. This does NOT cover cross-process
    /// concurrency; a second process writing the same dir may clobber.
    lock: Mutex<()>,
}

impl FilesystemOAuthTokenStore {
    /// Construct a store rooted at `dir`. The directory must already
    /// exist and be writable.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Backend`] if `dir` does not exist or is
    /// not a directory.
    pub fn new<P: Into<PathBuf>>(dir: P) -> Result<Self, StoreError> {
        let dir = dir.into();
        if !dir.is_dir() {
            return Err(StoreError::Backend(format!(
                "{} is not a directory",
                dir.display()
            )));
        }
        Ok(Self {
            dir,
            lock: Mutex::new(()),
        })
    }

    /// Borrow the backing directory path.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Sanitize a DID into a legal filename. Replaces `:` and `/` with
    /// `_` so `did:plc:abc` becomes `did_plc_abc`. DIDs never contain
    /// unprintable characters so the remainder passes through.
    fn filename_for(did: &str) -> String {
        let mut out = String::with_capacity(did.len() + 5);
        for ch in did.chars() {
            match ch {
                ':' | '/' | '\\' | '\0' => out.push('_'),
                c => out.push(c),
            }
        }
        out.push_str(".json");
        out
    }

    fn path_for(&self, did: &str) -> PathBuf {
        self.dir.join(Self::filename_for(did))
    }
}

impl OAuthTokenStore for FilesystemOAuthTokenStore {
    async fn save(&self, session: &OAuthSession) -> Result<(), StoreError> {
        let _g = self
            .lock
            .lock()
            .map_err(|e| StoreError::Backend(format!("lock poisoned: {e}")))?;
        let path = self.path_for(&session.did);
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec(session)
            .map_err(|e| StoreError::Backend(format!("serialize session: {e}")))?;
        fs::write(&tmp, &bytes)
            .map_err(|e| StoreError::Backend(format!("write temp {}: {e}", tmp.display())))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o600);
            fs::set_permissions(&tmp, perms)
                .map_err(|e| StoreError::Backend(format!("chmod {}: {e}", tmp.display())))?;
        }
        fs::rename(&tmp, &path).map_err(|e| {
            StoreError::Backend(format!(
                "rename {} -> {}: {e}",
                tmp.display(),
                path.display()
            ))
        })?;
        Ok(())
    }

    async fn load(&self, did: &str) -> Result<Option<OAuthSession>, StoreError> {
        let path = self.path_for(did);
        match fs::read(&path) {
            Ok(bytes) => {
                let session: OAuthSession = serde_json::from_slice(&bytes).map_err(|e| {
                    StoreError::Backend(format!("parse {}: {e}", path.display()))
                })?;
                Ok(Some(session))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(StoreError::Backend(format!(
                "read {}: {e}",
                path.display()
            ))),
        }
    }

    async fn delete(&self, did: &str) -> Result<(), StoreError> {
        let _g = self
            .lock
            .lock()
            .map_err(|e| StoreError::Backend(format!("lock poisoned: {e}")))?;
        let path = self.path_for(did);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StoreError::Backend(format!(
                "remove {}: {e}",
                path.display()
            ))),
        }
    }

    async fn list_dids(&self) -> Result<Vec<String>, StoreError> {
        // Read every *.json in the directory, parse each, and extract
        // its `did`. We cannot invert the filename-sanitization
        // because `:` → `_` is non-injective (a real DID containing
        // `_` would round-trip incorrectly), so we re-read the file
        // and trust the `did` field inside.
        let entries = fs::read_dir(&self.dir)
            .map_err(|e| StoreError::Backend(format!("read_dir {}: {e}", self.dir.display())))?;
        let mut out = Vec::new();
        for entry in entries {
            let entry = entry
                .map_err(|e| StoreError::Backend(format!("read_dir entry: {e}")))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let bytes = match fs::read(&path) {
                Ok(b) => b,
                // Race: file deleted between read_dir and open. Skip.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => {
                    return Err(StoreError::Backend(format!("read {}: {e}", path.display())));
                }
            };
            let session: OAuthSession = match serde_json::from_slice(&bytes) {
                Ok(s) => s,
                // Skip files that aren't valid session json — they
                // may be tmp files or orphaned payloads; don't let a
                // single bad file break enumeration.
                Err(_) => continue,
            };
            out.push(session.did);
        }
        out.sort();
        Ok(out)
    }
}
