//! Firehose consumer for `dev.idiolect.*` records.
//!
//! `idiolect-indexer` sits between a tapped-backed firehose
//! transport and an appview's per-record handlers. The crate is
//! organized around three narrow traits that mirror the adapter
//! pattern already used in `idiolect-lens`:
//!
//! - [`EventStream`]: source of raw firehose commits.
//! - [`RecordHandler`]: appview-side consumer of decoded records.
//! - [`CursorStore`]: durable seat for the firehose cursor so the
//!   indexer resumes cleanly after a restart.
//!
//! The orchestrator [`drive_indexer`] pulls from the stream, filters
//! to the `dev.idiolect.*` nsid prefix, decodes bodies into
//! [`idiolect_records::AnyRecord`], dispatches to the handler, and
//! commits the cursor on live events.
//!
//! The default feature set is transport-agnostic: every trait ships
//! an `InMemory*` impl that appview tests can wire up directly.
//! Enable `firehose-tapped` to plug the tapped-based adapter in.
//!
//! # Quickstart (in-memory)
//!
//! ```
//! use idiolect_indexer::{
//!     InMemoryCursorStore, InMemoryEventStream, IndexerAction, IndexerConfig,
//!     NoopRecordHandler, RawEvent, drive_indexer,
//! };
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! let mut stream = InMemoryEventStream::new();
//! stream.push(RawEvent {
//!     seq: 1,
//!     live: true,
//!     did: "did:plc:alice".to_owned(),
//!     rev: "3l5".to_owned(),
//!     collection: idiolect_records::Nsid::parse("dev.idiolect.encounter").unwrap(),
//!     rkey: "3l5".to_owned(),
//!     action: IndexerAction::Delete,
//!     cid: None,
//!     body: None,
//! });
//!
//! let handler = NoopRecordHandler::new();
//! let cursors = InMemoryCursorStore::new();
//! let cfg = IndexerConfig::default();
//!
//! drive_indexer(&mut stream, &handler, &cursors, &cfg)
//!     .await
//!     .unwrap();
//!
//! assert_eq!(handler.observed(), 1);
//! assert_eq!(handler.deletes(), 1);
//! # }
//! ```

pub mod cursor;
pub mod error;
pub mod event;
pub mod handler;
pub mod indexer;
#[cfg(feature = "reconnecting")]
pub mod reconnect;
#[cfg(feature = "resilience")]
pub mod resilience;
pub mod stream;

#[cfg(feature = "firehose-tapped")]
pub mod tapped;

#[cfg(feature = "firehose-jetstream")]
pub mod jetstream;

#[cfg(feature = "cursor-filesystem")]
pub mod cursor_fs;

#[cfg(feature = "cursor-sqlite")]
pub mod cursor_sqlite;

pub use cursor::{CursorStore, InMemoryCursorStore};
pub use error::IndexerError;
pub use event::{IndexerAction, IndexerEvent};
pub use handler::{NoopRecordHandler, RecordHandler};
pub use indexer::{IndexerConfig, drive_indexer};
pub use stream::{EventStream, InMemoryEventStream, RawEvent};

#[cfg(feature = "firehose-tapped")]
pub use tapped::TappedEventStream;

#[cfg(feature = "firehose-jetstream")]
pub use jetstream::{JetstreamEventStream, parse_frame as parse_jetstream_frame};

#[cfg(feature = "cursor-filesystem")]
pub use cursor_fs::FilesystemCursorStore;

#[cfg(feature = "cursor-sqlite")]
pub use cursor_sqlite::SqliteCursorStore;

#[cfg(feature = "reconnecting")]
pub use reconnect::{BackoffPolicy, ReconnectingEventStream};

#[cfg(feature = "resilience")]
pub use resilience::{CircuitBreakerHandler, CircuitPolicy, RetryPolicy, RetryingHandler};
