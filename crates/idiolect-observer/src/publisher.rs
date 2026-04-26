//! The [`ObservationPublisher`] trait — where finished observations go.
//!
//! The observer runtime is deliberately agnostic about the backend
//! that stores or transmits observations. Three shapes ship here:
//!
//! - [`InMemoryPublisher`] — a `Vec<Observation>` behind a mutex, for
//!   tests and offline fixtures.
//! - [`PdsPublisher`] — a PDS-backed publisher that serializes each
//!   observation to json, appends a `$type` field, and hands it to an
//!   [`idiolect_lens::PdsWriter`] via `com.atproto.repo.createRecord`.
//!   Generic over the writer, so an atrium-backed client slots in
//!   behind the `pds-atrium` feature on `idiolect-lens` while tests
//!   can substitute a fake writer.
//! - [`LogPublisher`] — a tracing-backed publisher that writes each
//!   observation as a structured `tracing::info!` event. Useful for
//!   smoke tests against a live firehose before a real PDS is wired
//!   up, and for "journal of observations" usage where the operator
//!   just wants a log line per snapshot.
//!
//! Publishers own their own retry and durability policy. The runtime
//! calls [`publish`](ObservationPublisher::publish) once per flush
//! cycle and treats any error as fatal; implementations that need
//! at-least-once delivery should buffer internally.

use std::sync::Mutex;

use idiolect_lens::{CreateRecordRequest, PdsWriter};
use idiolect_records::generated::dev::idiolect::observation::Observation;

use crate::error::{ObserverError, ObserverResult};

/// NSID of the observation record collection.
///
/// Lives here so both the PDS publisher (which needs it as the
/// `collection` parameter) and any future log-only publisher (which
/// would stamp it in the tracing output) can reach for the same
/// constant.
pub const OBSERVATION_NSID: &str = "dev.idiolect.observation";

/// Backend-agnostic observation sink.
///
/// Implementations MUST be [`Send`] + [`Sync`] so the runtime can
/// share a single publisher across the indexer task and the flush
/// task. `publish` takes `&self` because that is what every realistic
/// backend needs — a mutex-protected buffer, an owned http client, a
/// channel sender — so method authors do not have to prove exclusive
/// access to commit a record.
#[allow(async_fn_in_trait)]
pub trait ObservationPublisher: Send + Sync {
    /// Commit one observation.
    ///
    /// Returns [`ObserverError::Publisher`] for any backend failure.
    /// The runtime does not retry; callers that want retries should
    /// wrap their publisher.
    ///
    /// # Errors
    ///
    /// See [`ObserverError::Publisher`].
    async fn publish(&self, observation: &Observation) -> ObserverResult<()>;
}

/// A publisher that stashes observations in memory.
///
/// Used in tests and fixtures. Thread-safe via an internal `Mutex`;
/// [`observations`](Self::observations) snapshots the buffer by
/// cloning.
#[derive(Debug, Default)]
pub struct InMemoryPublisher {
    /// Committed observations, in publish order. Kept behind a mutex
    /// because the publisher is shared by `&self`.
    buffer: Mutex<Vec<Observation>>,
}

impl InMemoryPublisher {
    /// Construct an empty publisher.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clone all observations published so far, in publish order.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex has been poisoned by a previous
    /// panic while a guard was held. That only happens if an earlier
    /// `publish` call panicked mid-insert, which is a programmer
    /// error rather than a runtime one.
    #[must_use]
    pub fn observations(&self) -> Vec<Observation> {
        // unwrap: a poisoned mutex is a programmer error (another
        // thread panicked while holding the guard); callers of an
        // InMemoryPublisher in tests can afford to panic through.
        self.buffer
            .lock()
            .expect("in-memory publisher mutex was poisoned")
            .clone()
    }

    /// Number of observations published.
    ///
    /// # Panics
    ///
    /// See [`observations`](Self::observations).
    #[must_use]
    pub fn len(&self) -> usize {
        self.buffer
            .lock()
            .expect("in-memory publisher mutex was poisoned")
            .len()
    }

    /// Whether no observations have been published.
    ///
    /// # Panics
    ///
    /// See [`observations`](Self::observations).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Forward [`ObservationPublisher`] through a shared `Arc<T>`. Lets
/// one publisher instance be shared by multiple observers, or injected
/// as `Arc<dyn ObservationPublisher>` into a handler without a manual
/// delegate.
impl<T: ObservationPublisher + ?Sized> ObservationPublisher for std::sync::Arc<T> {
    async fn publish(&self, observation: &Observation) -> ObserverResult<()> {
        (**self).publish(observation).await
    }
}

impl ObservationPublisher for InMemoryPublisher {
    async fn publish(&self, observation: &Observation) -> ObserverResult<()> {
        self.buffer
            .lock()
            .map_err(|err| {
                ObserverError::Publisher(format!("in-memory publisher poisoned: {err}"))
            })?
            .push(observation.clone());
        Ok(())
    }
}

/// A publisher that writes observations into the observer's PDS.
///
/// Serializes every [`Observation`] into a json object, adds a
/// `$type: "dev.idiolect.observation"` field (so the record parses as
/// a lexicon-typed record on read), and calls
/// [`PdsWriter::create_record`] against the observer's own repo.
///
/// The publisher is generic over the writer so:
///
/// - A production deployment plugs in
///   [`idiolect_lens::AtriumPdsClient`] (behind the
///   `idiolect-lens/pds-atrium` feature) pre-configured with a
///   session-authenticated reqwest transport.
/// - Tests inject a hand-rolled `PdsWriter` fake that captures the
///   outgoing [`CreateRecordRequest`] without hitting the network.
///
/// `Send + Sync` bounds on [`PdsWriter`] already ensure the publisher
/// is safe to share across the indexer task and the flush task.
#[derive(Debug, Clone)]
pub struct PdsPublisher<W> {
    /// The underlying write-capable PDS client.
    writer: W,
    /// DID of the repo to write into. Every observation lands under
    /// this repo's `dev.idiolect.observation` collection.
    repo: String,
}

impl<W> PdsPublisher<W> {
    /// Construct a PDS publisher that writes every observation into
    /// `repo`.
    ///
    /// `repo` should be the observer's own DID — it is the account
    /// the PDS will attribute the record to, and it must match the
    /// `observer` field on the observations handed in.
    pub const fn new(writer: W, repo: String) -> Self {
        Self { writer, repo }
    }

    /// Borrow the underlying writer, e.g. to share it with another
    /// subsystem that needs PDS writes.
    pub const fn writer(&self) -> &W {
        &self.writer
    }

    /// Borrow the configured repo DID.
    pub fn repo(&self) -> &str {
        &self.repo
    }
}

/// A publisher that writes each observation as a structured
/// [`tracing::info!`] event on the `idiolect_observer::publisher` target.
///
/// No durability: events are as durable as the process's tracing
/// subscriber chooses to make them. Intended for:
///
/// - Smoke tests against a live firehose where a PDS is not yet
///   wired up.
/// - "Journal" deployments that just want a time-ordered log of
///   snapshots rather than a record on a PDS.
/// - Development / dry-run mode for downstream observers.
///
/// The event is structured: `method`, `version`, `observer`, and
/// `occurred_at` are emitted as fields, and the full `output` payload
/// goes through as a json string so a log scraper can re-hydrate it.
#[derive(Debug, Clone, Copy, Default)]
pub struct LogPublisher {
    /// Whether to also include the full serialized observation body
    /// in the tracing event. If `false`, the event only records the
    /// identifying fields (method, version, observer, `occurred_at`).
    /// Default `true`.
    include_body: bool,
}

impl LogPublisher {
    /// Construct a log publisher that emits the full observation body.
    #[must_use]
    pub const fn new() -> Self {
        Self { include_body: true }
    }

    /// Construct a log publisher that omits the observation body and
    /// only emits identifying fields.
    #[must_use]
    pub const fn headers_only() -> Self {
        Self {
            include_body: false,
        }
    }
}

impl ObservationPublisher for LogPublisher {
    async fn publish(&self, observation: &Observation) -> ObserverResult<()> {
        if self.include_body {
            let body = serde_json::to_string(observation)
                .map_err(|e| ObserverError::Publisher(format!("serialize observation: {e}")))?;
            tracing::info!(
                target: "idiolect_observer::publisher",
                method = %observation.method.name,
                version = %observation.version,
                observer = %observation.observer,
                occurred_at = %observation.occurred_at,
                body = %body,
                "observation published",
            );
        } else {
            tracing::info!(
                target: "idiolect_observer::publisher",
                method = %observation.method.name,
                version = %observation.version,
                observer = %observation.observer,
                occurred_at = %observation.occurred_at,
                "observation published",
            );
        }
        Ok(())
    }
}

impl<W: PdsWriter> ObservationPublisher for PdsPublisher<W> {
    async fn publish(&self, observation: &Observation) -> ObserverResult<()> {
        // serialize the typed record. then splice the `$type` field
        // in so the PDS recognizes it as a lexicon-typed observation.
        // the generated `Observation` struct does not carry `$type`
        // because it is the record body, not the envelope; adding it
        // here keeps every publish call a single serialization round.
        let mut record = serde_json::to_value(observation)
            .map_err(|e| ObserverError::Publisher(format!("serialize observation: {e}")))?;
        if let Some(obj) = record.as_object_mut() {
            obj.insert(
                "$type".to_owned(),
                serde_json::Value::String(OBSERVATION_NSID.to_owned()),
            );
        } else {
            return Err(ObserverError::Publisher(
                "serialized observation is not a json object".to_owned(),
            ));
        }

        self.writer
            .create_record(CreateRecordRequest {
                repo: self.repo.clone(),
                collection: OBSERVATION_NSID.to_owned(),
                // let the PDS allocate a TID. observations are append-
                // only; a predictable rkey would only be useful for
                // idempotent retry, which the current contract leaves
                // to a wrapping publisher.
                rkey: None,
                record,
                validate: None,
            })
            .await
            .map_err(|e| ObserverError::Publisher(format!("pds create_record: {e}")))?;

        Ok(())
    }
}
