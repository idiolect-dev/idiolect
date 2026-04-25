//! Caching [`Resolver`] wrapper.
//!
//! A thin TTL cache that wraps any [`Resolver`] and absorbs repeat
//! resolutions of the same at-uri within the TTL window. Useful when
//! the same lens is applied many times in a row (the observer's
//! correction-rate method resolves the same lens on every encounter,
//! for instance) or when running against a rate-limited PDS.
//!
//! The cache is deliberately small (a `HashMap`, no LRU bound) because
//! idiolect workloads resolve a handful of lenses per session, not
//! thousands. A caller that needs a bounded cache wraps this in their
//! own LRU.
//!
//! # Invariants
//!
//! - Entries expire after `ttl` wall time. No background expiry task;
//!   entries are pruned on the next `resolve` call for the same uri.
//! - Errors are NOT cached: a `LensError::NotFound` on one call does
//!   not poison the slot for subsequent calls. This matches the
//!   convention that atproto records can appear after a deletion, and
//!   it keeps the cache from turning transient transport failures
//!   into durable misses.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use idiolect_records::PanprotoLens;

use crate::AtUri;
use crate::error::LensError;
use crate::resolver::Resolver;

/// TTL-cached resolver.
pub struct CachingResolver<R> {
    /// The underlying resolver.
    inner: R,
    /// Cache entries keyed by at-uri string. Plain `Mutex` rather than
    /// `RwLock` because atproto records are small and the `clone` on
    /// hit is cheap enough that contention is not a concern.
    entries: Mutex<HashMap<String, CacheEntry>>,
    /// How long a cached entry is considered fresh.
    ttl: Duration,
}

struct CacheEntry {
    lens: PanprotoLens,
    inserted_at: Instant,
}

impl<R> CachingResolver<R> {
    /// Wrap a resolver with a TTL cache.
    ///
    /// `ttl = Duration::ZERO` disables caching (every call reaches the
    /// inner resolver). Useful for tests that exercise the wrapper
    /// without adding a time-based flake.
    pub fn new(inner: R, ttl: Duration) -> Self {
        Self {
            inner,
            entries: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// Borrow the wrapped resolver, e.g. to reach its own configuration.
    pub const fn inner(&self) -> &R {
        &self.inner
    }

    /// Current number of cached entries, including expired ones that
    /// have not yet been pruned. Intended for tests and debug output.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex has been poisoned.
    #[must_use]
    pub fn cache_size(&self) -> usize {
        self.entries.lock().expect("cache mutex poisoned").len()
    }

    /// Drop every cached entry. Useful for tests and for callers that
    /// know an upstream mutation has invalidated every entry at once
    /// (e.g. a fresh PDS session).
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex has been poisoned.
    pub fn clear(&self) {
        self.entries.lock().expect("cache mutex poisoned").clear();
    }
}

impl<R: Resolver> Resolver for CachingResolver<R> {
    async fn resolve(&self, uri: &AtUri) -> Result<PanprotoLens, LensError> {
        let key = uri.to_string();

        // Fast path: fresh hit.
        if self.ttl > Duration::ZERO {
            let entries = self
                .entries
                .lock()
                .map_err(|e| LensError::Transport(format!("cache poisoned: {e}")))?;
            if let Some(entry) = entries.get(&key)
                && entry.inserted_at.elapsed() < self.ttl
            {
                return Ok(entry.lens.clone());
            }
        }

        // Miss or expired: resolve, then insert.
        let lens = self.inner.resolve(uri).await?;

        if self.ttl > Duration::ZERO {
            let mut entries = self
                .entries
                .lock()
                .map_err(|e| LensError::Transport(format!("cache poisoned: {e}")))?;
            entries.insert(
                key,
                CacheEntry {
                    lens: lens.clone(),
                    inserted_at: Instant::now(),
                },
            );
        }

        Ok(lens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Counts calls to the inner resolver.
    struct CountingResolver {
        calls: Arc<AtomicUsize>,
        response: PanprotoLens,
    }

    impl Resolver for CountingResolver {
        async fn resolve(&self, _uri: &AtUri) -> Result<PanprotoLens, LensError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.response.clone())
        }
    }

    /// Always errors.
    struct FailingResolver {
        calls: Arc<AtomicUsize>,
    }

    impl Resolver for FailingResolver {
        async fn resolve(&self, uri: &AtUri) -> Result<PanprotoLens, LensError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(LensError::NotFound(uri.to_string()))
        }
    }

    fn sample_lens() -> PanprotoLens {
        PanprotoLens {
            blob: None,
            created_at: "2026-04-19T00:00:00.000Z".into(),
            laws_verified: None,
            object_hash: "sha256:deadbeef".into(),
            round_trip_class: None,
            source_schema: "sha256:a".into(),
            target_schema: "sha256:b".into(),
        }
    }

    #[tokio::test]
    async fn second_call_within_ttl_is_a_cache_hit() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = CountingResolver {
            calls: Arc::clone(&calls),
            response: sample_lens(),
        };
        let caching = CachingResolver::new(inner, Duration::from_secs(60));

        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/l1").unwrap();
        caching.resolve(&uri).await.unwrap();
        caching.resolve(&uri).await.unwrap();
        caching.resolve(&uri).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(caching.cache_size(), 1);
    }

    #[tokio::test]
    async fn ttl_zero_disables_cache() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = CountingResolver {
            calls: Arc::clone(&calls),
            response: sample_lens(),
        };
        let caching = CachingResolver::new(inner, Duration::ZERO);

        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/l1").unwrap();
        caching.resolve(&uri).await.unwrap();
        caching.resolve(&uri).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(caching.cache_size(), 0);
    }

    #[tokio::test]
    async fn distinct_uris_share_no_slot() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = CountingResolver {
            calls: Arc::clone(&calls),
            response: sample_lens(),
        };
        let caching = CachingResolver::new(inner, Duration::from_secs(60));

        let u1 = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/l1").unwrap();
        let u2 = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/l2").unwrap();
        caching.resolve(&u1).await.unwrap();
        caching.resolve(&u2).await.unwrap();
        caching.resolve(&u1).await.unwrap();
        caching.resolve(&u2).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn errors_are_not_cached() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = FailingResolver {
            calls: Arc::clone(&calls),
        };
        let caching = CachingResolver::new(inner, Duration::from_secs(60));

        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/absent").unwrap();
        caching.resolve(&uri).await.unwrap_err();
        caching.resolve(&uri).await.unwrap_err();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(caching.cache_size(), 0);
    }

    #[tokio::test]
    async fn clear_drops_entries() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = CountingResolver {
            calls: Arc::clone(&calls),
            response: sample_lens(),
        };
        let caching = CachingResolver::new(inner, Duration::from_secs(60));
        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/l1").unwrap();
        caching.resolve(&uri).await.unwrap();
        caching.clear();
        caching.resolve(&uri).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn ttl_expiry_causes_refetch() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = CountingResolver {
            calls: Arc::clone(&calls),
            response: sample_lens(),
        };
        // 1ms TTL — we can sleep past it cheaply.
        let caching = CachingResolver::new(inner, Duration::from_millis(1));
        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/l1").unwrap();
        caching.resolve(&uri).await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        caching.resolve(&uri).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
