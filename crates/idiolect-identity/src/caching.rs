//! Caching [`IdentityResolver`] wrapper.
//!
//! TTL-based memoization of DID → [`DidDocument`] resolutions.
//! Structurally identical to `idiolect_lens::CachingResolver`: a plain
//! `HashMap` guarded by a `Mutex`, entries pruned lazily on the next
//! call for the same DID after expiry.
//!
//! # Why cache here?
//!
//! Many idiolect subsystems translate a DID to a PDS URL once per
//! request / per event. A firehose consumer that writes one record
//! per commit and wants to fan out to the author's PDS can otherwise
//! issue one plc.directory round-trip per event. Even a 60-second TTL
//! collapses that into a single resolve per active DID per minute.
//!
//! # Invariants
//!
//! - TTL controls freshness. `Duration::ZERO` disables caching.
//! - Errors are NOT cached: a `NotFound` on one call does not poison
//!   the slot, because atproto DIDs can become valid after tombstoning
//!   (e.g. plc operation re-registration), and transient transport
//!   failures should not turn into durable misses.
//! - No LRU bound. Idiolect workloads see a bounded set of distinct
//!   DIDs; callers that need an LRU wrap this.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::Did;
use crate::document::DidDocument;
use crate::error::IdentityError;
use crate::resolver::IdentityResolver;

/// TTL-cached identity resolver.
pub struct CachingIdentityResolver<R> {
    inner: R,
    entries: Mutex<HashMap<String, CacheEntry>>,
    ttl: Duration,
}

struct CacheEntry {
    doc: DidDocument,
    inserted_at: Instant,
}

impl<R> CachingIdentityResolver<R> {
    /// Wrap a resolver with a TTL cache.
    pub fn new(inner: R, ttl: Duration) -> Self {
        Self {
            inner,
            entries: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// Borrow the wrapped resolver.
    pub const fn inner(&self) -> &R {
        &self.inner
    }

    /// Number of cached entries (possibly including expired ones that
    /// have not yet been pruned).
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex has been poisoned.
    #[must_use]
    pub fn cache_size(&self) -> usize {
        self.entries.lock().expect("cache mutex poisoned").len()
    }

    /// Drop every cached entry.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex has been poisoned.
    pub fn clear(&self) {
        self.entries.lock().expect("cache mutex poisoned").clear();
    }
}

impl<R: IdentityResolver> IdentityResolver for CachingIdentityResolver<R> {
    async fn resolve(&self, did: &Did) -> Result<DidDocument, IdentityError> {
        let key = did.as_str().to_owned();

        if self.ttl > Duration::ZERO {
            let entries = self
                .entries
                .lock()
                .map_err(|e| IdentityError::Transport(format!("cache poisoned: {e}")))?;
            if let Some(entry) = entries.get(&key)
                && entry.inserted_at.elapsed() < self.ttl
            {
                return Ok(entry.doc.clone());
            }
        }

        let doc = self.inner.resolve(did).await?;

        if self.ttl > Duration::ZERO {
            let mut entries = self
                .entries
                .lock()
                .map_err(|e| IdentityError::Transport(format!("cache poisoned: {e}")))?;
            entries.insert(
                key,
                CacheEntry {
                    doc: doc.clone(),
                    inserted_at: Instant::now(),
                },
            );
        }

        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Service;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingResolver {
        calls: Arc<AtomicUsize>,
        response: DidDocument,
    }

    impl IdentityResolver for CountingResolver {
        async fn resolve(&self, _did: &Did) -> Result<DidDocument, IdentityError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.response.clone())
        }
    }

    struct FailingResolver {
        calls: Arc<AtomicUsize>,
    }

    impl IdentityResolver for FailingResolver {
        async fn resolve(&self, did: &Did) -> Result<DidDocument, IdentityError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(IdentityError::NotFound(did.to_string()))
        }
    }

    fn doc(id: &str) -> DidDocument {
        DidDocument {
            id: id.to_owned(),
            also_known_as: vec!["at://alice.test".into()],
            service: vec![Service {
                id: "#atproto_pds".into(),
                service_type: "AtprotoPersonalDataServer".into(),
                service_endpoint: "https://pds.example".into(),
            }],
            verification_method: vec![],
            extras: serde_json::Map::default(),
        }
    }

    #[tokio::test]
    async fn second_call_within_ttl_is_cache_hit() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = CountingResolver {
            calls: Arc::clone(&calls),
            response: doc("did:plc:alice"),
        };
        let caching = CachingIdentityResolver::new(inner, Duration::from_secs(60));
        let did = Did::parse("did:plc:alice").unwrap();
        caching.resolve(&did).await.unwrap();
        caching.resolve(&did).await.unwrap();
        caching.resolve(&did).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(caching.cache_size(), 1);
    }

    #[tokio::test]
    async fn ttl_zero_disables_cache() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = CountingResolver {
            calls: Arc::clone(&calls),
            response: doc("did:plc:alice"),
        };
        let caching = CachingIdentityResolver::new(inner, Duration::ZERO);
        let did = Did::parse("did:plc:alice").unwrap();
        caching.resolve(&did).await.unwrap();
        caching.resolve(&did).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(caching.cache_size(), 0);
    }

    #[tokio::test]
    async fn errors_are_not_cached() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = FailingResolver {
            calls: Arc::clone(&calls),
        };
        let caching = CachingIdentityResolver::new(inner, Duration::from_secs(60));
        let did = Did::parse("did:plc:absent").unwrap();
        caching.resolve(&did).await.unwrap_err();
        caching.resolve(&did).await.unwrap_err();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn distinct_dids_cache_independently() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = CountingResolver {
            calls: Arc::clone(&calls),
            response: doc("did:plc:alice"),
        };
        let caching = CachingIdentityResolver::new(inner, Duration::from_secs(60));
        let a = Did::parse("did:plc:alice").unwrap();
        let b = Did::parse("did:plc:bob").unwrap();
        caching.resolve(&a).await.unwrap();
        caching.resolve(&b).await.unwrap();
        caching.resolve(&a).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn ttl_expiry_causes_refetch() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = CountingResolver {
            calls: Arc::clone(&calls),
            response: doc("did:plc:alice"),
        };
        let caching = CachingIdentityResolver::new(inner, Duration::from_millis(1));
        let did = Did::parse("did:plc:alice").unwrap();
        caching.resolve(&did).await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        caching.resolve(&did).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn clear_drops_entries() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = CountingResolver {
            calls: Arc::clone(&calls),
            response: doc("did:plc:alice"),
        };
        let caching = CachingIdentityResolver::new(inner, Duration::from_secs(60));
        let did = Did::parse("did:plc:alice").unwrap();
        caching.resolve(&did).await.unwrap();
        caching.clear();
        caching.resolve(&did).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
