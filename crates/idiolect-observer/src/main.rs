//! Reference observer daemon binary.
//!
//! Wires a live tapped-backed firehose into the observer pipeline
//! shipped in this crate. By default observations accumulate in an
//! in-memory publisher; setting `IDIOLECT_PDS_URL` switches to a
//! [`PdsPublisher`](idiolect_observer::PdsPublisher) that writes
//! `dev.idiolect.observation` records into the observer's repo via
//! [`idiolect_lens::AtriumPdsClient`].
//!
//! # Configuration (env vars)
//!
//! - `IDIOLECT_OBSERVER_DID` (required) — observer's own DID. This is
//!   the did that will appear in every published observation's
//!   `observer` field and the repo the PDS publisher writes into.
//! - `IDIOLECT_TAP_URL` — HTTP URL of the tap instance (default:
//!   `http://localhost:2480`).
//! - `IDIOLECT_TAP_ADMIN_PASSWORD` — tap admin password, if the tap
//!   instance requires basic auth. Optional.
//! - `IDIOLECT_PDS_URL` — service URL of the PDS to publish into
//!   (e.g. `https://bsky.social`). When unset, the daemon uses an
//!   in-memory publisher and only logs counts on shutdown. The
//!   reference binary does not yet plumb authentication; set up the
//!   atrium client in a wrapper binary if your PDS requires auth.
//! - `IDIOLECT_FLUSH_EVENTS` — flush after every N events
//!   (default: `100`). Set `0` to disable event-count flushing
//!   (still flushes once at clean shutdown).
//! - `IDIOLECT_OBSERVER_CURSORS` — path to a sqlite file for cursor
//!   persistence. When set, the daemon uses
//!   [`SqliteCursorStore`](idiolect_indexer::SqliteCursorStore) so a
//!   restart resumes from the last committed firehose position.
//!   When unset, falls back to
//!   [`InMemoryCursorStore`](idiolect_indexer::InMemoryCursorStore)
//!   and the observer replays from tap's retention floor on each
//!   restart.
//! - `RUST_LOG` — tracing filter; defaults to `info`.
//!
//! Run with the `daemon` feature:
//!
//! ```shell
//! cargo run -p idiolect-observer --features daemon
//! ```

#![cfg(feature = "daemon")]
#![allow(clippy::too_many_lines)]

use std::env;

use anyhow::{Context, Result};
use idiolect_indexer::{
    CursorStore, InMemoryCursorStore, IndexerConfig, SqliteCursorStore, TappedEventStream,
};
use idiolect_lens::AtriumPdsClient;
use idiolect_observer::{
    CorrectionRateMethod, FlushSchedule, InMemoryPublisher, ObservationPublisher, ObserverConfig,
    ObserverHandler, PdsPublisher, drive_observer,
};
use tapped::TapClient;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let observer_did = env::var("IDIOLECT_OBSERVER_DID")
        .context("IDIOLECT_OBSERVER_DID must be set to the observer's DID")?;
    let tap_url =
        env::var("IDIOLECT_TAP_URL").unwrap_or_else(|_| "http://localhost:2480".to_owned());
    let admin_password = env::var("IDIOLECT_TAP_ADMIN_PASSWORD").ok();
    let pds_url = env::var("IDIOLECT_PDS_URL").ok();
    let flush_events: u32 = env::var("IDIOLECT_FLUSH_EVENTS")
        .ok()
        .as_deref()
        .map(str::parse)
        .transpose()
        .context("IDIOLECT_FLUSH_EVENTS must be a non-negative integer")?
        .unwrap_or(100);

    info!(
        observer_did = %observer_did,
        tap_url = %tap_url,
        pds_url = ?pds_url,
        flush_events,
        "starting idiolect-observer"
    );

    // connect to tap and open an event channel. admin_password is
    // optional because some dev setups leave auth off.
    let tap = admin_password
        .as_ref()
        .map_or_else(
            || TapClient::new(&tap_url),
            |pw| TapClient::with_auth(&tap_url, pw),
        )
        .context("failed to construct tap client")?;

    let receiver = tap
        .channel()
        .await
        .context("failed to open tap event channel")?;

    let mut stream = TappedEventStream::new(receiver);

    // Cursor store: sqlite if IDIOLECT_OBSERVER_CURSORS is set,
    // in-memory otherwise. The branch is deliberate (rather than a
    // single `Box<dyn CursorStore>`) because the trait is not
    // dyn-compatible under native async-fn-in-trait.
    let cursors_path = env::var("IDIOLECT_OBSERVER_CURSORS").ok();
    let ix = IndexerConfig::default();
    let schedule = if flush_events == 0 {
        FlushSchedule::Manual
    } else {
        FlushSchedule::EveryEvents(flush_events)
    };

    // Branch on the cursor store backend first (sqlite vs in-memory),
    // then on the publisher backend (PDS vs in-memory). Both branches
    // are taken at runtime, not compile-time feature flags, because
    // the binary already gates sqlite behind the `daemon` feature.
    match cursors_path {
        Some(path) => {
            info!(cursors_path = %path, "using SqliteCursorStore");
            let cursors = SqliteCursorStore::open(&path)
                .with_context(|| format!("open sqlite cursor store at {path}"))?;
            dispatch_publisher(
                &mut stream,
                observer_did,
                pds_url,
                &cursors,
                &ix,
                schedule,
            )
            .await
        }
        None => {
            info!(
                "IDIOLECT_OBSERVER_CURSORS unset; using InMemoryCursorStore \
                 (cursor resets on every restart)"
            );
            let cursors = InMemoryCursorStore::new();
            dispatch_publisher(
                &mut stream,
                observer_did,
                pds_url,
                &cursors,
                &ix,
                schedule,
            )
            .await
        }
    }
}

/// Dispatch on the publisher backend (PDS vs in-memory) and drive the
/// observer.
async fn dispatch_publisher<S, C>(
    stream: &mut S,
    observer_did: String,
    pds_url: Option<String>,
    cursors: &C,
    ix: &IndexerConfig,
    schedule: FlushSchedule,
) -> Result<()>
where
    S: idiolect_indexer::EventStream,
    C: CursorStore,
{
    if let Some(url) = pds_url {
        info!(pds_url = %url, "publishing observations via PdsPublisher");
        let writer = AtriumPdsClient::with_service_url(&url);
        let publisher = PdsPublisher::new(writer, observer_did.clone());
        run_observer(stream, publisher, observer_did, cursors, ix, schedule).await
    } else {
        info!("no IDIOLECT_PDS_URL set; using InMemoryPublisher (no records will be persisted)");
        let publisher = InMemoryPublisher::new();
        run_observer(stream, publisher, observer_did, cursors, ix, schedule).await
    }
}

/// Wire a method + publisher into an [`ObserverHandler`] and run
/// [`drive_observer`] to completion. Factored into a helper so the
/// in-memory and PDS branches share the pipeline body.
async fn run_observer<S, P, C>(
    stream: &mut S,
    publisher: P,
    observer_did: String,
    cursors: &C,
    ix: &IndexerConfig,
    schedule: FlushSchedule,
) -> Result<()>
where
    S: idiolect_indexer::EventStream,
    P: ObservationPublisher,
    C: CursorStore,
{
    let method = CorrectionRateMethod::new();
    let cfg = ObserverConfig {
        observer_did,
        ..ObserverConfig::default()
    };
    let handler = ObserverHandler::new(method, publisher, cfg);

    let run = drive_observer(stream, &handler, cursors, ix, schedule);

    tokio::select! {
        result = run => {
            if let Err(err) = result {
                warn!(error = %err, "observer loop exited with error");
                return Err(err.into());
            }
            info!("observer loop exited cleanly");
        }
        _ = tokio::signal::ctrl_c() => {
            info!("received ctrl-c, flushing and exiting");
            if let Err(err) = handler.flush().await {
                warn!(error = %err, "final flush failed");
            }
        }
    }

    info!("observer stopped");
    Ok(())
}

/// Initialize tracing with an env-filter and a compact console
/// formatter. Kept in a helper so tests of the library don't have to
/// stand up a subscriber.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}
