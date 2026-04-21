//! Reference observer daemon for the `dev.idiolect.*` family.
//!
//! An *observer* is a party that (a) consumes encounter-family
//! records off the atproto firehose, (b) runs a pluggable aggregation
//! method over them, and (c) periodically publishes a structured
//! `dev.idiolect.observation` record summarizing what it has seen.
//!
//! The observer decouples *ranking* from the orchestrator (P6 in the
//! idiolect architecture): any number of observers may publish
//! competing aggregates over the same firehose, and users choose whom
//! to trust by referencing specific observer dids.
//!
//! # Pipeline
//!
//! ```text
//!      firehose (tapped)
//!          │
//!          ▼
//!   idiolect_indexer::drive_indexer
//!          │            ┌──────────────────────┐
//!          ▼            │                      ▼
//!    ObserverHandler  ──┤   ObservationMethod  │   (fold into aggregate)
//!          │            └──────────────────────┘
//!   flush timer
//!          │
//!          ▼
//!      Observation (dev.idiolect.observation)
//!          │
//!          ▼
//!    ObservationPublisher
//! ```
//!
//! # Shape
//!
//! Three narrow traits do the pluggable work:
//!
//! - [`ObservationMethod`] — the stateful aggregator. Folds decoded
//!   events into internal state and can be asked to snapshot that
//!   state as a json blob.
//! - [`ObservationPublisher`] — backend that stores or transmits
//!   finished observations. [`InMemoryPublisher`] is shipped for
//!   tests and offline use; [`PdsPublisher`] writes every observation
//!   into the observer's own PDS via an
//!   [`idiolect_lens::PdsWriter`] (the `pds-atrium` feature on
//!   `idiolect-lens` supplies an atrium-backed client).
//! - [`RecordHandler`](idiolect_indexer::RecordHandler) — reused from
//!   [`idiolect_indexer`]. [`ObserverHandler`] implements it by
//!   adapting the indexer's per-event surface onto an
//!   [`ObservationMethod`].
//!
//! The [`drive_observer`] orchestrator runs the pipeline end to end.
//! The companion binary (feature `daemon`) wires a tapped-backed
//! firehose, an in-memory publisher for now, and a sigterm-driven
//! shutdown.
//!
//! # Quickstart (in-memory, test-style)
//!
//! ```
//! use idiolect_indexer::{
//!     InMemoryCursorStore, InMemoryEventStream, IndexerAction, IndexerConfig,
//!     RawEvent,
//! };
//! use idiolect_observer::{
//!     CorrectionRateMethod, FlushSchedule, InMemoryPublisher, ObserverConfig,
//!     ObserverHandler, drive_observer,
//! };
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! let method = CorrectionRateMethod::new();
//! let publisher = InMemoryPublisher::new();
//! let cfg = ObserverConfig {
//!     observer_did: "did:plc:observer".to_owned(),
//!     ..ObserverConfig::default()
//! };
//!
//! let handler = ObserverHandler::new(method, publisher, cfg)
//!     .with_clock(|| "2026-04-20T00:00:00Z".to_owned());
//!
//! let mut stream = InMemoryEventStream::new();
//! let cursors = InMemoryCursorStore::new();
//! let ix = IndexerConfig::default();
//!
//! // no events, empty run — the loop cleanly closes.
//! drive_observer(&mut stream, &handler, &cursors, &ix, FlushSchedule::Manual)
//!     .await
//!     .unwrap();
//!
//! // no events observed, nothing to publish.
//! assert!(handler.publisher().is_empty());
//! # }
//! ```

pub mod driver;
pub mod error;
/// Generated from `observer-spec/methods.json` by `idiolect-codegen`.
/// Do not edit by hand; regenerate via
/// `cargo run -p idiolect-codegen -- generate`.
pub mod generated;
pub mod handler;
pub mod instance_method;
pub mod method;
pub mod methods;
pub mod publisher;

pub use driver::{FlushSchedule, drive_observer};
pub use error::{ObserverError, ObserverResult};
pub use handler::{ObserverConfig, ObserverHandler};
pub use method::{ObservationMethod, build_observation};
pub use methods::{
    CorrectionRateMethod, EncounterThroughputMethod, LensAdoptionMethod,
    VerificationCoverageMethod,
};
pub use publisher::{
    InMemoryPublisher, LogPublisher, OBSERVATION_NSID, ObservationPublisher, PdsPublisher,
};
