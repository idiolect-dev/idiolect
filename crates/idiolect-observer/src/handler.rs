//! [`ObserverHandler`] — the [`RecordHandler`] used by the observer
//! daemon.
//!
//! The handler is a thin adapter over an [`ObservationMethod`] and an
//! [`ObservationPublisher`]:
//!
//! 1. Every firehose event is folded into the method via
//!    [`ObservationMethod::observe`].
//! 2. When the caller asks for a flush (via
//!    [`ObserverHandler::flush`]), the handler snapshots the method,
//!    wraps the output in a [`dev.idiolect.observation`] record, and
//!    hands the record to the publisher.
//!
//! Flush cadence is a *deployment* concern, not a handler one. The
//! reference binary drives it off a tokio timer; tests drive it
//! explicitly.

use std::sync::Mutex;

use idiolect_indexer::{IndexerError, IndexerEvent, RecordHandler};
use idiolect_records::generated::defs::Visibility;

use crate::error::{ObserverError, ObserverResult};
use crate::method::{ObservationMethod, build_observation};
use crate::publisher::ObservationPublisher;

/// Configuration for an [`ObserverHandler`].
///
/// The handler uses these values to tag every emitted observation.
#[derive(Debug, Clone)]
pub struct ObserverConfig {
    /// DID that will show up in every published observation's
    /// `observer` field. Must match the PDS the publisher writes to.
    pub observer_did: String,
    /// Visibility to stamp on every emitted observation. Default
    /// deployments publish [`Visibility::PublicDetailed`]; observers
    /// that publish community-scoped aggregates override this.
    pub visibility: Visibility,
}

impl Default for ObserverConfig {
    fn default() -> Self {
        Self {
            observer_did: "did:plc:unconfigured-observer".to_owned(),
            visibility: Visibility::PublicDetailed,
        }
    }
}

/// A [`RecordHandler`] that folds in-scope events into an
/// [`ObservationMethod`] and publishes periodic
/// [`dev.idiolect.observation`] snapshots via an
/// [`ObservationPublisher`].
///
/// The method is owned behind a `Mutex` because
/// [`RecordHandler::handle`] takes `&self` (so the trait is object-
/// safe and shareable across tasks), but
/// [`ObservationMethod::observe`] takes `&mut self`. A single handler
/// serializes all writes to its method through that mutex.
pub struct ObserverHandler<M, P> {
    /// The pluggable aggregator. Wrapped in `Mutex` so the handler
    /// can expose `&self` to the indexer while still calling
    /// `method.observe(&mut self, ...)` under the hood.
    method: Mutex<M>,
    /// Where finished observations go.
    publisher: P,
    /// Static tagging applied to every emitted observation.
    config: ObserverConfig,
    /// Optional clock override. When `None`, [`flush`](Self::flush)
    /// stamps the emitted observation with the system clock in
    /// RFC-3339 format via [`time::OffsetDateTime::now_utc`]. Tests
    /// set this to freeze time.
    clock: Mutex<Option<Box<dyn Fn() -> String + Send + Sync>>>,
}

impl<M, P> ObserverHandler<M, P>
where
    M: ObservationMethod,
    P: ObservationPublisher,
{
    /// Construct a handler from a method + publisher + config.
    pub fn new(method: M, publisher: P, config: ObserverConfig) -> Self {
        Self {
            method: Mutex::new(method),
            publisher,
            config,
            clock: Mutex::new(None),
        }
    }

    /// Install a clock override.
    ///
    /// The closure is called once per [`flush`](Self::flush) and its
    /// return value becomes the `occurredAt` stamp on the emitted
    /// observation.
    ///
    /// # Panics
    ///
    /// Panics if the internal clock mutex has been poisoned by a
    /// previous panic while a guard was held — only reachable if an
    /// earlier call panicked while mutating the clock slot.
    #[must_use]
    pub fn with_clock<F>(self, clock: F) -> Self
    where
        F: Fn() -> String + Send + Sync + 'static,
    {
        // prime the clock slot. we cannot move self through the mutex
        // in-place, so replace the empty slot with a populated one.
        *self.clock.lock().expect("clock mutex poisoned") = Some(Box::new(clock));
        self
    }

    /// Borrow the publisher, e.g. to inspect emitted observations in a
    /// test.
    pub const fn publisher(&self) -> &P {
        &self.publisher
    }

    /// Take a snapshot of the current aggregator state, build a
    /// [`dev.idiolect.observation`] record, and hand it to the
    /// publisher.
    ///
    /// Returns `Ok(None)` when the method's
    /// [`snapshot`](ObservationMethod::snapshot) call returns `None`
    /// (not enough data to publish yet).
    ///
    /// # Errors
    ///
    /// Any [`ObserverError`] variant.
    pub async fn flush(&self) -> ObserverResult<Option<String>> {
        // build the observation under a short-lived lock so the
        // indexer's `observe` calls are not blocked while the
        // publisher's (potentially slow) write runs.
        let occurred_at = self.now();
        let observation = {
            let guard = self
                .method
                .lock()
                .map_err(|e| ObserverError::Method(format!("method mutex poisoned: {e}")))?;
            build_observation(
                &*guard,
                &self.config.observer_did,
                &occurred_at,
                self.config.visibility,
            )?
        };

        let Some(observation) = observation else {
            return Ok(None);
        };

        self.publisher.publish(&observation).await?;

        Ok(Some(occurred_at))
    }

    /// Current timestamp in rfc-3339 format.
    fn now(&self) -> String {
        let guard = self.clock.lock().expect("clock mutex poisoned");
        guard.as_ref().map_or_else(
            || {
                // time 0.3 produces iso-8601 by default; rfc-3339 is
                // a subset so the same format string suffices.
                time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
            },
            |clock| clock(),
        )
    }
}

impl<M, P> RecordHandler for ObserverHandler<M, P>
where
    M: ObservationMethod,
    P: ObservationPublisher,
{
    async fn handle(&self, event: &IndexerEvent) -> Result<(), IndexerError> {
        // lock the method, fold the event in, drop the guard before we
        // return so the lock is held no longer than observe() runs.
        // errors are surfaced as indexer-level handler errors so the
        // orchestrator's termination policy stays consistent across
        // all handler failures.
        let result = {
            let mut guard = self.method.lock().map_err(|e| {
                IndexerError::Handler(format!("observer method mutex poisoned: {e}"))
            })?;
            guard
                .observe(event)
                .map_err(|e| IndexerError::Handler(e.to_string()))
        };
        result?;
        Ok(())
    }
}
