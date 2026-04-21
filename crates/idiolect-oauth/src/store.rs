//! The [`OAuthTokenStore`] boundary — where session state lives
//! across process restarts.
//!
//! Sessions are ephemeral in the sense that they never reach a PDS,
//! but they still need to persist locally between requests and
//! restarts (otherwise the user would re-authenticate on every page
//! load). The storage backend is pluggable: a browser client plugs
//! in `localStorage` or `IndexedDB`, a server plugs in sqlite or
//! Redis, tests plug in [`InMemoryOAuthTokenStore`].
//!
//! The trait uses the same narrow async shape as the indexer's
//! cursor store and the lens runtime's resolver: `Send + Sync`,
//! three methods (save / load / delete), and an in-memory impl
//! bundled alongside the trait for tests.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::session::OAuthSession;

/// Persistence boundary for OAuth sessions.
///
/// Implementations are keyed by the session's DID. `load` returning
/// `Ok(None)` means "no session for this account", not "storage
/// unreachable"; storage failures go through [`StoreError`].
#[allow(async_fn_in_trait)]
pub trait OAuthTokenStore: Send + Sync {
    /// Save `session` under `session.did`, replacing any previous
    /// entry for the same DID.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the backing store fails.
    async fn save(&self, session: &OAuthSession) -> Result<(), StoreError>;

    /// Load the session for `did`, if any.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the backing store fails.
    async fn load(&self, did: &str) -> Result<Option<OAuthSession>, StoreError>;

    /// Delete the session for `did`. A no-op if no session is
    /// stored.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the backing store fails.
    async fn delete(&self, did: &str) -> Result<(), StoreError>;

    /// Enumerate every DID that currently has a session stored.
    ///
    /// Operator endpoints that need to list active sessions, and
    /// refresh-daemons that iterate stored sessions on a schedule,
    /// both depend on this. The default impl returns an empty vec so
    /// stores that predate this method continue to compile; every
    /// shipped store overrides it.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the backing store fails.
    async fn list_dids(&self) -> Result<Vec<String>, StoreError> {
        Ok(Vec::new())
    }
}

/// Errors raised by an [`OAuthTokenStore`] implementation.
///
/// The variant taxonomy is deliberately flat: storage errors are
/// opaque strings because production backends (localStorage,
/// sqlite, Redis, …) each have their own error types and the
/// appview cannot usefully dispatch on them.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The backing store failed to read or write. The inner string
    /// is implementation-defined; callers should surface it in
    /// operator logs but not in end-user errors.
    #[error("oauth token store: {0}")]
    Backend(String),
}

/// `HashMap`-backed [`OAuthTokenStore`] for tests and fixtures.
///
/// Entries live only for the lifetime of the process. Lock holds
/// are sub-microsecond `HashMap` operations, so a sync mutex inside
/// an async trait method is the right trade for avoiding a tokio
/// dependency in the default build.
#[derive(Debug, Default)]
pub struct InMemoryOAuthTokenStore {
    /// Sessions keyed by did.
    sessions: Mutex<HashMap<String, OAuthSession>>,
}

impl InMemoryOAuthTokenStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of sessions currently stored.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex has been poisoned by a prior
    /// panic in another thread.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sessions.lock().expect("sessions mutex poisoned").len()
    }

    /// Whether the store is empty.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex has been poisoned.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sessions
            .lock()
            .expect("sessions mutex poisoned")
            .is_empty()
    }
}

/// Forward [`OAuthTokenStore`] through a shared `Arc<T>`. The
/// token-refresh flow and the HTTP handler side of an OAuth client
/// need to share exactly one store instance; `Arc<dyn OAuthTokenStore>`
/// is the canonical way to plumb that without cloning.
impl<T: OAuthTokenStore + ?Sized> OAuthTokenStore for std::sync::Arc<T> {
    async fn save(&self, session: &OAuthSession) -> Result<(), StoreError> {
        (**self).save(session).await
    }

    async fn load(&self, did: &str) -> Result<Option<OAuthSession>, StoreError> {
        (**self).load(did).await
    }

    async fn delete(&self, did: &str) -> Result<(), StoreError> {
        (**self).delete(did).await
    }

    async fn list_dids(&self) -> Result<Vec<String>, StoreError> {
        (**self).list_dids().await
    }
}

impl OAuthTokenStore for InMemoryOAuthTokenStore {
    async fn save(&self, session: &OAuthSession) -> Result<(), StoreError> {
        {
            let mut sessions = self.sessions.lock().expect("sessions mutex poisoned");
            sessions.insert(session.did.clone(), session.clone());
        }

        Ok(())
    }

    async fn load(&self, did: &str) -> Result<Option<OAuthSession>, StoreError> {
        let session = {
            let sessions = self.sessions.lock().expect("sessions mutex poisoned");
            sessions.get(did).cloned()
        };

        Ok(session)
    }

    async fn delete(&self, did: &str) -> Result<(), StoreError> {
        {
            let mut sessions = self.sessions.lock().expect("sessions mutex poisoned");
            sessions.remove(did);
        }

        Ok(())
    }

    async fn list_dids(&self) -> Result<Vec<String>, StoreError> {
        let sessions = self.sessions.lock().expect("sessions mutex poisoned");
        let mut out: Vec<String> = sessions.keys().cloned().collect();
        out.sort();
        Ok(out)
    }
}
