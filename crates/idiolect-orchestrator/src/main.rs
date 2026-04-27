//! Reference orchestrator daemon.
//!
//! Wires together the orchestrator's three parts into a standalone
//! process:
//!
//! - `idiolect_indexer::drive_indexer` against a tapped-backed
//!   firehose,
//! - `CatalogHandler` with a `SqliteCatalogStore` persistence mirror,
//! - the HTTP query API, bound to a caller-configured address.
//!
//! Configuration is read from environment variables, matching the
//! observer daemon's shape:
//!
//! | var                            | purpose                                           |
//! |--------------------------------|---------------------------------------------------|
//! | `IDIOLECT_TAP_URL`             | tap admin URL (required)                          |
//! | `IDIOLECT_TAP_ADMIN_PASSWORD`  | tap admin password (required)                     |
//! | `IDIOLECT_ORCHESTRATOR_DB`     | sqlite path (default `./orchestrator.db`)         |
//! | `IDIOLECT_ORCHESTRATOR_CURSORS`| sqlite cursors path (default `./cursors.db`)      |
//! | `IDIOLECT_HTTP_ADDR`           | bind address (default `127.0.0.1:8787`)           |
//! | `IDIOLECT_SUBSCRIPTION_ID`     | cursor slot (default `idiolect-orchestrator`)     |
//!
//! Graceful shutdown on SIGINT / SIGTERM: closes the firehose first,
//! then the HTTP listener.

#![cfg(feature = "daemon")]

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use idiolect_indexer::{IndexerConfig, SqliteCursorStore, TappedEventStream, drive_indexer};
use idiolect_orchestrator::{
    AppState, CatalogHandler, SqliteCatalogStore, handler::CatalogPersist, http_router,
};
use tapped::TapClient;
use tracing_subscriber::EnvFilter;

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_owned())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let tap_url = env_or("IDIOLECT_TAP_URL", "http://localhost:2480");
    let tap_pw = std::env::var("IDIOLECT_TAP_ADMIN_PASSWORD").ok();
    let db_path = env_or("IDIOLECT_ORCHESTRATOR_DB", "./orchestrator.db");
    let cursor_path = env_or("IDIOLECT_ORCHESTRATOR_CURSORS", "./cursors.db");
    let http_addr: SocketAddr = env_or("IDIOLECT_HTTP_ADDR", "127.0.0.1:8787")
        .parse()
        .context("IDIOLECT_HTTP_ADDR is not a socket addr")?;
    let subscription_id = env_or("IDIOLECT_SUBSCRIPTION_ID", "idiolect-orchestrator");

    tracing::info!(%db_path, %cursor_path, %http_addr, "starting orchestrator");

    // Open sqlite catalog and warm the in-memory view from it.
    let store = Arc::new(SqliteCatalogStore::open(&db_path)?);
    let catalog = Arc::new(std::sync::Mutex::new(store.load_catalog()?));
    tracing::info!(
        records = catalog.lock().unwrap().len(),
        "loaded catalog from sqlite"
    );

    let handler = CatalogHandler::with_catalog(Arc::clone(&catalog))
        .with_persist(store.clone() as Arc<dyn CatalogPersist>);

    // Tapped firehose.
    let tap = tap_pw
        .as_ref()
        .map_or_else(
            || TapClient::new(&tap_url),
            |pw| TapClient::with_auth(&tap_url, pw),
        )
        .context("construct tap client")?;
    let receiver = tap.channel().await.context("open tap event channel")?;
    let mut stream = TappedEventStream::new(receiver);

    // Sqlite cursor store.
    let cursors = SqliteCursorStore::open(&cursor_path).context("open cursors")?;
    let ix = IndexerConfig { subscription_id };

    // HTTP task. The readiness flag is flipped immediately because
    // we just successfully warmed the catalog; a deployment with a
    // longer warmup (e.g. backfill from multiple sources) can split
    // the warmup into a separate task and flip the flag only when it
    // is genuinely ready.
    let ready = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let state = AppState::with_readiness(Arc::clone(&catalog), Arc::clone(&ready));
    let app = http_router(state);
    let listener = tokio::net::TcpListener::bind(http_addr)
        .await
        .context("bind http listener")?;
    let http_task = tokio::spawn(async move {
        tracing::info!(%http_addr, "http api listening");
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!(error = %e, "http api terminated");
        }
    });

    // Firehose task.
    let indexer_task = tokio::spawn(async move {
        if let Err(e) = drive_indexer(&mut stream, &handler, &cursors, &ix).await {
            tracing::error!(error = %e, "indexer terminated");
        }
    });

    // Wait for a shutdown signal.
    tokio::select! {
        _ = tokio::signal::ctrl_c() => tracing::info!("SIGINT received, shutting down"),
        _ = indexer_task => tracing::info!("indexer exited"),
        _ = http_task => tracing::info!("http exited"),
    }

    Ok(())
}
