//! Auto-reconnecting [`EventStream`] wrapper.
//!
//! Production firehose transports (tapped, jetstream) exit cleanly
//! when the websocket closes. In practice the websocket closes for
//! many reasons that are not "we are done" — transient network drops,
//! remote restarts, idle-timeout disconnects. A production indexer
//! needs:
//!
//! 1. reconnect with backoff,
//! 2. cursor replay on reconnect (so no events are lost),
//! 3. a cap on reconnect attempts so a permanent upstream outage does
//!    not consume unbounded time.
//!
//! [`ReconnectingEventStream`] supplies all three over any
//! `EventStream`-returning factory.
//!
//! # Shape
//!
//! - `connect: impl Fn(Option<u64>) -> impl Future<Output = ...>` is
//!   the reconnect closure. Receives the last observed seq (to pass
//!   to the upstream as a cursor) and returns a fresh `EventStream`.
//!   Errors abort the current reconnect attempt but the wrapper keeps
//!   trying until the backoff policy gives up.
//! - [`BackoffPolicy`] controls the sleep between attempts and the
//!   maximum attempt count.
//!
//! # Semantics
//!
//! - On inner stream's `Ok(None)` (clean close): reconnect.
//! - On inner stream's `Err(Stream)`: emit a warn log, reconnect.
//! - On reconnect attempts exceeded: return `Err(Stream)` with the
//!   last connection error.
//! - `next_event` is cancel-safe *as long as the underlying
//!   `connect` closure is cancel-safe*.

use std::future::Future;
use std::time::Duration;

use crate::error::IndexerError;
use crate::stream::{EventStream, RawEvent};

/// Backoff policy for reconnect attempts.
///
/// `initial` is the first sleep after a disconnect; each subsequent
/// attempt multiplies by `multiplier` up to `max`. Once `max_attempts`
/// attempts have failed, the stream gives up and returns an error.
#[derive(Debug, Clone, Copy)]
pub struct BackoffPolicy {
    /// Delay before the first reconnect attempt.
    pub initial: Duration,
    /// Upper bound on the delay between attempts.
    pub max: Duration,
    /// Multiplier applied after each failed attempt.
    pub multiplier: f64,
    /// Maximum number of consecutive failed reconnect attempts before
    /// giving up. `None` means "retry forever".
    pub max_attempts: Option<u32>,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            initial: Duration::from_millis(500),
            max: Duration::from_secs(30),
            multiplier: 2.0,
            max_attempts: Some(10),
        }
    }
}

impl BackoffPolicy {
    /// Policy that never sleeps and never gives up — for tests.
    #[must_use]
    pub const fn instant_forever() -> Self {
        Self {
            initial: Duration::ZERO,
            max: Duration::ZERO,
            multiplier: 1.0,
            max_attempts: None,
        }
    }

    /// Policy that gives up after `max_attempts` with no backoff.
    #[must_use]
    pub const fn instant_capped(max_attempts: u32) -> Self {
        Self {
            initial: Duration::ZERO,
            max: Duration::ZERO,
            multiplier: 1.0,
            max_attempts: Some(max_attempts),
        }
    }

    /// Compute the sleep duration for attempt index `attempt` (0-based).
    #[must_use]
    pub fn delay_for(&self, attempt: u32) -> Duration {
        if self.initial.is_zero() {
            return Duration::ZERO;
        }
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_wrap)]
        let secs = self.initial.as_secs_f64() * self.multiplier.powi(attempt as i32);
        let capped = secs.min(self.max.as_secs_f64()).max(0.0);
        Duration::from_secs_f64(capped)
    }
}

/// Reconnecting [`EventStream`] wrapper.
///
/// Holds the current inner stream (if connected) and the last observed
/// seq (so the reconnect closure can resume from that point). The
/// connect closure is called lazily on the first `next_event` call and
/// again after every disconnect.
pub struct ReconnectingEventStream<C, F, S> {
    connect: C,
    backoff: BackoffPolicy,
    inner: Option<S>,
    last_seq: Option<u64>,
    _marker: std::marker::PhantomData<fn() -> F>,
}

impl<C, F, S> ReconnectingEventStream<C, F, S>
where
    C: Fn(Option<u64>) -> F + Send + Sync,
    F: Future<Output = Result<S, IndexerError>> + Send,
    S: EventStream + Send,
{
    /// Construct a reconnecting stream around a factory closure.
    ///
    /// The first reconnect sees `last_seq = None` — appropriate for a
    /// fresh start. To resume from a committed cursor, use
    /// [`with_cursor`](Self::with_cursor) or
    /// [`from_cursor_store`](Self::from_cursor_store) instead.
    pub fn new(connect: C, backoff: BackoffPolicy) -> Self {
        Self::with_cursor(connect, backoff, None)
    }

    /// Construct a reconnecting stream seeded with an initial cursor.
    ///
    /// The first reconnect call receives `Some(seq)` in the connect
    /// closure's argument so the upstream stream can request events
    /// from that point forward. Use when the caller already holds a
    /// cursor value (e.g. from a bespoke persistence store).
    pub fn with_cursor(connect: C, backoff: BackoffPolicy, initial_cursor: Option<u64>) -> Self {
        Self {
            connect,
            backoff,
            inner: None,
            last_seq: initial_cursor,
            _marker: std::marker::PhantomData,
        }
    }

    /// Construct a reconnecting stream seeded from a [`CursorStore`]
    /// at the given subscription id.
    ///
    /// Performs one `store.load(subscription_id)` call to recover the
    /// last-committed seq, then hands that value to the connect
    /// closure on first reconnect. A `None` return from `load` is
    /// treated as "no cursor yet" — matching the indexer's semantics.
    ///
    /// # Errors
    ///
    /// Propagates any [`IndexerError`] the cursor store raises on
    /// `load`.
    pub async fn from_cursor_store<Store: crate::cursor::CursorStore>(
        connect: C,
        backoff: BackoffPolicy,
        store: &Store,
        subscription_id: &str,
    ) -> Result<Self, IndexerError> {
        let seq = store.load(subscription_id).await?;
        Ok(Self::with_cursor(connect, backoff, seq))
    }

    /// Snapshot the cursor the wrapper would hand to the next
    /// reconnect attempt. Useful for tests and for operator tooling
    /// that wants to mirror the reconnect cursor to a separate
    /// dashboard.
    #[must_use]
    pub const fn last_seq(&self) -> Option<u64> {
        self.last_seq
    }

    /// Attempt to establish a fresh inner stream, respecting the
    /// backoff policy. Returns the last error if every attempt
    /// fails.
    async fn reconnect(&mut self) -> Result<(), IndexerError> {
        let mut attempt: u32 = 0;
        loop {
            let delay = self.backoff.delay_for(attempt);
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            match (self.connect)(self.last_seq).await {
                Ok(stream) => {
                    tracing::info!(
                        last_seq = ?self.last_seq,
                        attempt,
                        "firehose reconnected"
                    );
                    self.inner = Some(stream);
                    return Ok(());
                }
                Err(err) => {
                    tracing::warn!(
                        attempt,
                        error = %err,
                        "firehose reconnect attempt failed"
                    );
                    attempt = attempt.saturating_add(1);
                    if let Some(max) = self.backoff.max_attempts
                        && attempt >= max
                    {
                        return Err(IndexerError::Stream(format!(
                            "reconnect gave up after {max} attempts: {err}"
                        )));
                    }
                }
            }
        }
    }
}

impl<C, F, S> EventStream for ReconnectingEventStream<C, F, S>
where
    C: Fn(Option<u64>) -> F + Send + Sync,
    F: Future<Output = Result<S, IndexerError>> + Send,
    S: EventStream + Send,
{
    async fn next_event(&mut self) -> Result<Option<RawEvent>, IndexerError> {
        loop {
            // Establish inner on first call and after any disconnect.
            if self.inner.is_none() {
                self.reconnect().await?;
            }

            // The unwrap is safe: reconnect populates self.inner or
            // returns Err above.
            let inner = self.inner.as_mut().expect("reconnect must populate inner");
            match inner.next_event().await {
                Ok(Some(event)) => {
                    // Remember the live cursor so the next reconnect
                    // can resume from it. Backfill events (live=false)
                    // do not advance the recorded seq — matches the
                    // indexer's cursor-store policy.
                    if event.live {
                        self.last_seq = Some(event.seq);
                    }
                    // Synthetic events with no live flag still
                    // progress the cursor for the reconnect
                    // resumption purpose.
                    return Ok(Some(event));
                }
                Ok(None) => {
                    tracing::info!(
                        last_seq = ?self.last_seq,
                        "firehose closed cleanly, reconnecting"
                    );
                    self.inner = None;
                }
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        "firehose stream errored, reconnecting"
                    );
                    self.inner = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::IndexerAction;
    use crate::stream::{InMemoryEventStream, RawEvent};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn event(seq: u64, live: bool) -> RawEvent {
        RawEvent {
            seq,
            live,
            did: "did:plc:x".into(),
            rev: format!("r{seq}"),
            collection: "dev.idiolect.encounter".into(),
            rkey: format!("r{seq}"),
            action: IndexerAction::Create,
            cid: None,
            body: Some(serde_json::json!({})),
        }
    }

    #[tokio::test]
    async fn reconnect_on_clean_close_resumes_from_cursor() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let seen_cursors: Arc<std::sync::Mutex<Vec<Option<u64>>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));

        let a2 = Arc::clone(&attempts);
        let sc = Arc::clone(&seen_cursors);
        let connect = move |last_seq: Option<u64>| {
            let attempts = Arc::clone(&a2);
            let sc = Arc::clone(&sc);
            async move {
                let n = attempts.fetch_add(1, Ordering::SeqCst);
                sc.lock().unwrap().push(last_seq);
                let mut stream = InMemoryEventStream::new();
                if n == 0 {
                    stream.push(event(1, true));
                    stream.push(event(2, true));
                } else if n == 1 {
                    stream.push(event(3, true));
                } else {
                    // terminate the test loop by returning a closing stream
                }
                Ok::<_, IndexerError>(stream)
            }
        };

        let mut wrapper = ReconnectingEventStream::new(connect, BackoffPolicy::instant_capped(1));
        // Drain: event 1, event 2 (first stream exhausts), reconnect,
        // event 3. The wrapper tracks last-observed seq as it goes.
        assert_eq!(wrapper.next_event().await.unwrap().unwrap().seq, 1);
        assert_eq!(wrapper.next_event().await.unwrap().unwrap().seq, 2);
        assert_eq!(wrapper.next_event().await.unwrap().unwrap().seq, 3);

        let cursors = seen_cursors.lock().unwrap().clone();
        // First connect: no cursor known yet. After seq=2 disconnect
        // the wrapper reconnects and hands the cursor Some(2) through.
        assert_eq!(cursors[0], None);
        assert_eq!(cursors[1], Some(2));
    }

    #[tokio::test]
    async fn gives_up_after_max_attempts() {
        let counter = Arc::new(AtomicUsize::new(0));
        let c2 = Arc::clone(&counter);
        let connect = move |_last_seq: Option<u64>| {
            let c2 = Arc::clone(&c2);
            async move {
                c2.fetch_add(1, Ordering::SeqCst);
                Err::<InMemoryEventStream, _>(IndexerError::Stream("upstream down".into()))
            }
        };
        let mut wrapper = ReconnectingEventStream::new(connect, BackoffPolicy::instant_capped(3));
        let err = wrapper.next_event().await.unwrap_err();
        assert!(
            matches!(err, IndexerError::Stream(msg) if msg.contains("reconnect gave up after 3 attempts"))
        );
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn transient_error_triggers_reconnect_without_returning() {
        // First stream errors on first read; second stream yields a good event.
        let attempts = Arc::new(AtomicUsize::new(0));
        let a2 = Arc::clone(&attempts);

        struct OneShotError {
            fired: bool,
        }

        impl EventStream for OneShotError {
            async fn next_event(&mut self) -> Result<Option<RawEvent>, IndexerError> {
                if self.fired {
                    Ok(None)
                } else {
                    self.fired = true;
                    Err(IndexerError::Stream("mid-stream hiccup".into()))
                }
            }
        }

        enum Either {
            Err(OneShotError),
            Ok(InMemoryEventStream),
        }

        impl EventStream for Either {
            async fn next_event(&mut self) -> Result<Option<RawEvent>, IndexerError> {
                match self {
                    Self::Err(e) => e.next_event().await,
                    Self::Ok(o) => o.next_event().await,
                }
            }
        }

        let connect = move |_last_seq: Option<u64>| {
            let n = a2.fetch_add(1, Ordering::SeqCst);
            async move {
                if n == 0 {
                    Ok::<Either, IndexerError>(Either::Err(OneShotError { fired: false }))
                } else {
                    let mut s = InMemoryEventStream::new();
                    s.push(event(42, true));
                    Ok(Either::Ok(s))
                }
            }
        };

        let mut wrapper = ReconnectingEventStream::new(connect, BackoffPolicy::instant_forever());
        let ev = wrapper.next_event().await.unwrap().unwrap();
        assert_eq!(ev.seq, 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn with_cursor_seeds_initial_last_seq() {
        // The connect closure errors so the wrapper gives up after
        // one attempt — otherwise an empty stream would loop forever
        // reconnecting. What we care about is that `last_seq`
        // observed by the first connect call is the seeded value.
        let seen = Arc::new(std::sync::Mutex::new(Vec::<Option<u64>>::new()));
        let s2 = Arc::clone(&seen);
        let connect = move |last_seq: Option<u64>| {
            let s2 = Arc::clone(&s2);
            async move {
                s2.lock().unwrap().push(last_seq);
                Err::<InMemoryEventStream, _>(IndexerError::Stream("upstream down".into()))
            }
        };
        let mut wrapper = ReconnectingEventStream::with_cursor(
            connect,
            BackoffPolicy::instant_capped(1),
            Some(42),
        );
        assert_eq!(wrapper.last_seq(), Some(42));
        // `next_event` triggers one connect attempt, which fails, and
        // the max-attempts cap of 1 makes the wrapper give up.
        let err = wrapper.next_event().await.unwrap_err();
        assert!(matches!(err, IndexerError::Stream(_)));
        let observed = seen.lock().unwrap().clone();
        assert_eq!(observed[0], Some(42));
    }

    #[tokio::test]
    async fn from_cursor_store_recovers_committed_seq() {
        use crate::cursor::{CursorStore, InMemoryCursorStore};
        let store = InMemoryCursorStore::new();
        store.commit("firehose-x", 1234).await.unwrap();

        let connect = |_last: Option<u64>| async move {
            Ok::<InMemoryEventStream, _>(InMemoryEventStream::new())
        };
        let wrapper = ReconnectingEventStream::from_cursor_store(
            connect,
            BackoffPolicy::default(),
            &store,
            "firehose-x",
        )
        .await
        .unwrap();
        assert_eq!(wrapper.last_seq(), Some(1234));
    }

    #[tokio::test]
    async fn from_cursor_store_absent_seq_is_none() {
        use crate::cursor::InMemoryCursorStore;
        let store = InMemoryCursorStore::new();
        let connect = |_last: Option<u64>| async move {
            Ok::<InMemoryEventStream, _>(InMemoryEventStream::new())
        };
        let wrapper = ReconnectingEventStream::from_cursor_store(
            connect,
            BackoffPolicy::default(),
            &store,
            "never-set",
        )
        .await
        .unwrap();
        assert!(wrapper.last_seq().is_none());
    }

    #[test]
    fn backoff_delay_grows_up_to_cap() {
        let p = BackoffPolicy {
            initial: Duration::from_millis(100),
            max: Duration::from_millis(400),
            multiplier: 2.0,
            max_attempts: Some(10),
        };
        assert_eq!(p.delay_for(0), Duration::from_millis(100));
        assert_eq!(p.delay_for(1), Duration::from_millis(200));
        assert_eq!(p.delay_for(2), Duration::from_millis(400));
        // Capped at max (400ms) past attempt 2.
        assert_eq!(p.delay_for(5), Duration::from_millis(400));
    }
}
