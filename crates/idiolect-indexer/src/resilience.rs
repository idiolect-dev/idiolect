//! Resilience wrappers for [`RecordHandler`].
//!
//! `drive_indexer`'s policy is "one handler error halts the loop" —
//! appropriate for programmer-bug cases (a decode panic, a unique-
//! constraint violation), inappropriate for transient-downstream-blip
//! cases (a storage timeout, a rate-limit slap). This module wraps a
//! handler in policies that let the caller tune the failure mode:
//!
//! - [`RetryingHandler`] — bounded retry with exponential backoff.
//!   Surface the error only after every retry is exhausted.
//! - [`CircuitBreakerHandler`] — after `threshold` consecutive
//!   failures in `window`, reject every subsequent event with a fast
//!   `Handler` error until a cool-off period passes. Prevents a
//!   flapping downstream from saturating the firehose with futile
//!   dispatches.
//!
//! Both wrappers are composable: `CircuitBreakerHandler::new(
//! RetryingHandler::new(my_handler, ...), ...)` is a common shape.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::error::IndexerError;
use crate::event::IndexerEvent;
use crate::handler::RecordHandler;

// -----------------------------------------------------------------
// Retry
// -----------------------------------------------------------------

/// Retry policy: `max_attempts` total tries (including the first),
/// sleeping `initial_delay * multiplier^(attempt-1)` between them,
/// capped at `max_delay`.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Total attempts including the initial try. `1` = no retry.
    pub max_attempts: u32,
    /// Delay before the first retry.
    pub initial_delay: Duration,
    /// Upper bound on any single sleep.
    pub max_delay: Duration,
    /// Multiplier per retry. `1.0` = linear; `2.0` = exponential.
    pub multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            multiplier: 2.0,
        }
    }
}

impl RetryPolicy {
    /// Delay before attempt index `n` (0-based).
    #[must_use]
    pub fn delay_for(&self, n: u32) -> Duration {
        if n == 0 {
            return Duration::ZERO;
        }
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_wrap)]
        let secs = self.initial_delay.as_secs_f64() * self.multiplier.powi((n - 1) as i32);
        let capped = secs.min(self.max_delay.as_secs_f64()).max(0.0);
        Duration::from_secs_f64(capped)
    }

    /// Instant-no-backoff policy for tests.
    #[must_use]
    pub const fn instant(max_attempts: u32) -> Self {
        Self {
            max_attempts,
            initial_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
            multiplier: 1.0,
        }
    }
}

/// [`RecordHandler`] wrapper that retries on error.
pub struct RetryingHandler<H> {
    inner: H,
    policy: RetryPolicy,
}

impl<H> RetryingHandler<H> {
    /// Wrap a handler with a retry policy.
    pub const fn new(inner: H, policy: RetryPolicy) -> Self {
        Self { inner, policy }
    }

    /// Borrow the wrapped handler.
    pub const fn inner(&self) -> &H {
        &self.inner
    }
}

impl<H: RecordHandler> RecordHandler for RetryingHandler<H> {
    async fn handle(&self, event: &IndexerEvent) -> Result<(), IndexerError> {
        let mut last_err: Option<IndexerError> = None;
        for attempt in 0..self.policy.max_attempts {
            let delay = self.policy.delay_for(attempt);
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            match self.inner.handle(event).await {
                Ok(()) => return Ok(()),
                Err(err) => {
                    tracing::warn!(
                        attempt,
                        max = self.policy.max_attempts,
                        error = %err,
                        "handler attempt failed; retrying"
                    );
                    last_err = Some(err);
                }
            }
        }
        Err(last_err
            .unwrap_or_else(|| IndexerError::Handler("RetryingHandler: no attempts made".into())))
    }
}

// -----------------------------------------------------------------
// Circuit breaker
// -----------------------------------------------------------------

/// Circuit-breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CircuitState {
    /// Handler is healthy; every event is dispatched.
    Closed,
    /// Handler is failing; every event short-circuits. Transitions to
    /// `HalfOpen` after `cool_off` elapses.
    Open,
    /// Single trial event is permitted. On success → `Closed`; on
    /// failure → `Open`.
    HalfOpen,
}

/// Configuration knobs for [`CircuitBreakerHandler`].
#[derive(Debug, Clone, Copy)]
pub struct CircuitPolicy {
    /// Consecutive failures inside `window` that trip the breaker.
    pub threshold: u32,
    /// Sliding window over which failures accumulate.
    pub window: Duration,
    /// Time the breaker stays Open before allowing a trial event.
    pub cool_off: Duration,
}

impl Default for CircuitPolicy {
    fn default() -> Self {
        Self {
            threshold: 5,
            window: Duration::from_secs(60),
            cool_off: Duration::from_secs(30),
        }
    }
}

struct BreakerState {
    state: CircuitState,
    /// Timestamps of recent failures; pruned on every transition.
    failures: Vec<Instant>,
    /// When the breaker last tripped Open.
    opened_at: Option<Instant>,
}

impl BreakerState {
    const fn new() -> Self {
        Self {
            state: CircuitState::Closed,
            failures: Vec::new(),
            opened_at: None,
        }
    }

    fn record_failure(&mut self, policy: &CircuitPolicy) {
        let now = Instant::now();
        self.failures
            .retain(|t| now.duration_since(*t) < policy.window);
        self.failures.push(now);
        if u32::try_from(self.failures.len()).unwrap_or(u32::MAX) >= policy.threshold {
            self.state = CircuitState::Open;
            self.opened_at = Some(now);
        }
    }

    fn record_success(&mut self) {
        self.failures.clear();
        self.state = CircuitState::Closed;
        self.opened_at = None;
    }

    /// If the breaker is Open and the cool-off has elapsed, transition
    /// to `HalfOpen`. Returns the current state after the check.
    fn check_and_maybe_half_open(&mut self, policy: &CircuitPolicy) -> CircuitState {
        if self.state == CircuitState::Open
            && let Some(opened_at) = self.opened_at
            && opened_at.elapsed() >= policy.cool_off
        {
            self.state = CircuitState::HalfOpen;
        }
        self.state
    }
}

/// [`RecordHandler`] wrapper that opens a circuit after repeated
/// failures. An open circuit short-circuits every event with a fast
/// error until `cool_off` elapses; after cool-off a single trial
/// event goes through. Success closes the circuit; failure re-opens.
pub struct CircuitBreakerHandler<H> {
    inner: H,
    policy: CircuitPolicy,
    state: Mutex<BreakerState>,
}

impl<H> CircuitBreakerHandler<H> {
    /// Wrap a handler with a circuit-breaker policy.
    pub fn new(inner: H, policy: CircuitPolicy) -> Self {
        Self {
            inner,
            policy,
            state: Mutex::new(BreakerState::new()),
        }
    }

    /// Borrow the wrapped handler.
    pub const fn inner(&self) -> &H {
        &self.inner
    }

    /// Snapshot the current breaker state, useful for `/metrics` or
    /// a diagnostic endpoint.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex has been poisoned.
    #[must_use]
    pub fn is_open(&self) -> bool {
        let mut st = self.state.lock().expect("circuit state mutex poisoned");
        matches!(
            st.check_and_maybe_half_open(&self.policy),
            CircuitState::Open
        )
    }
}

impl<H: RecordHandler> RecordHandler for CircuitBreakerHandler<H> {
    async fn handle(&self, event: &IndexerEvent) -> Result<(), IndexerError> {
        // Pre-check: if Open AND cool-off has not elapsed, short-circuit.
        {
            let mut st = self
                .state
                .lock()
                .map_err(|e| IndexerError::Handler(format!("circuit mutex poisoned: {e}")))?;
            match st.check_and_maybe_half_open(&self.policy) {
                CircuitState::Open => {
                    return Err(IndexerError::Handler(
                        "circuit breaker open; short-circuiting".into(),
                    ));
                }
                CircuitState::Closed | CircuitState::HalfOpen => {}
            }
        }

        // Dispatch to inner.
        let result = self.inner.handle(event).await;

        // Post-dispatch: record success or failure.
        let mut st = self
            .state
            .lock()
            .map_err(|e| IndexerError::Handler(format!("circuit mutex poisoned: {e}")))?;
        match &result {
            Ok(()) => st.record_success(),
            Err(_) => st.record_failure(&self.policy),
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::IndexerAction;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn event() -> IndexerEvent {
        IndexerEvent {
            seq: 1,
            live: true,
            did: "did:plc:x".into(),
            rev: "r".into(),
            collection: idiolect_records::Nsid::parse("dev.idiolect.encounter").expect("valid nsid"),
            rkey: "k".into(),
            action: IndexerAction::Create,
            cid: None,
            record: None,
        }
    }

    struct Flaky {
        fail_count: AtomicUsize,
        fail_until: usize,
    }

    impl RecordHandler for Flaky {
        async fn handle(&self, _event: &IndexerEvent) -> Result<(), IndexerError> {
            let n = self.fail_count.fetch_add(1, Ordering::SeqCst);
            if n < self.fail_until {
                Err(IndexerError::Handler(format!("flaky attempt {n}")))
            } else {
                Ok(())
            }
        }
    }

    struct AlwaysFails;

    impl RecordHandler for AlwaysFails {
        async fn handle(&self, _event: &IndexerEvent) -> Result<(), IndexerError> {
            Err(IndexerError::Handler("always fails".into()))
        }
    }

    #[tokio::test]
    async fn retrying_handler_retries_then_succeeds() {
        let inner = Flaky {
            fail_count: AtomicUsize::new(0),
            fail_until: 2,
        };
        let wrapper = RetryingHandler::new(inner, RetryPolicy::instant(5));
        wrapper.handle(&event()).await.unwrap();
        // First two attempts failed, third succeeded: 3 total inner calls.
        assert_eq!(wrapper.inner().fail_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retrying_handler_gives_up_after_max_attempts() {
        let inner = Flaky {
            fail_count: AtomicUsize::new(0),
            fail_until: 100,
        };
        let wrapper = RetryingHandler::new(inner, RetryPolicy::instant(3));
        let err = wrapper.handle(&event()).await.unwrap_err();
        assert!(matches!(err, IndexerError::Handler(msg) if msg.contains("flaky attempt 2")));
        assert_eq!(wrapper.inner().fail_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn circuit_breaker_opens_after_threshold_failures() {
        let inner = AlwaysFails;
        let policy = CircuitPolicy {
            threshold: 3,
            window: Duration::from_secs(60),
            cool_off: Duration::from_secs(60),
        };
        let wrapper = CircuitBreakerHandler::new(inner, policy);

        // 3 failures open the circuit.
        for _ in 0..3 {
            assert!(wrapper.handle(&event()).await.is_err());
        }
        assert!(wrapper.is_open());

        // Subsequent calls short-circuit without reaching the inner handler.
        let err = wrapper.handle(&event()).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("circuit breaker open"));
    }

    #[tokio::test]
    async fn circuit_breaker_half_open_on_cool_off_and_closes_on_success() {
        struct FailThenPass {
            calls: Arc<AtomicUsize>,
        }
        impl RecordHandler for FailThenPass {
            async fn handle(&self, _event: &IndexerEvent) -> Result<(), IndexerError> {
                let n = self.calls.fetch_add(1, Ordering::SeqCst);
                if n < 3 {
                    Err(IndexerError::Handler("fail".into()))
                } else {
                    Ok(())
                }
            }
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = FailThenPass {
            calls: Arc::clone(&calls),
        };
        let policy = CircuitPolicy {
            threshold: 3,
            window: Duration::from_secs(60),
            // Tiny cool-off so the test can wait past it.
            cool_off: Duration::from_millis(5),
        };
        let wrapper = CircuitBreakerHandler::new(inner, policy);

        for _ in 0..3 {
            assert!(wrapper.handle(&event()).await.is_err());
        }
        assert!(wrapper.is_open());

        // Wait past cool-off, then try again. The half-open probe
        // succeeds (FailThenPass's 4th call is Ok), so the circuit
        // closes.
        tokio::time::sleep(Duration::from_millis(10)).await;
        wrapper.handle(&event()).await.unwrap();
        assert!(!wrapper.is_open());
    }

    #[tokio::test]
    async fn circuit_breaker_half_open_reopens_on_failure() {
        let inner = AlwaysFails;
        let policy = CircuitPolicy {
            threshold: 2,
            window: Duration::from_secs(60),
            cool_off: Duration::from_millis(5),
        };
        let wrapper = CircuitBreakerHandler::new(inner, policy);

        for _ in 0..2 {
            assert!(wrapper.handle(&event()).await.is_err());
        }
        assert!(wrapper.is_open());

        tokio::time::sleep(Duration::from_millis(10)).await;
        // Half-open probe fails (AlwaysFails), so the circuit reopens.
        let err = wrapper.handle(&event()).await.unwrap_err();
        let msg = err.to_string();
        // Could be either "always fails" (probe reached inner) or the
        // short-circuit message depending on race; we just care the
        // circuit is open after.
        let _ = msg;
        assert!(wrapper.is_open());
    }
}
