//! Reference orchestrator for the `dev.idiolect.*` record family.
//!
//! The orchestrator is the read-side counterpart to the observer: it
//! ingests the *declarative* records — adapters, bounties, communities,
//! dialects, recommendations, verifications — and answers
//! principle-compatible queries (P4 community self-constitution, P5
//! non-mediating federation) over them.
//!
//! The orchestrator never *enforces*: it only answers. A community can
//! publish a recommendation; the orchestrator will tell you it exists
//! and whether its required verifications are in place. The decision
//! to *follow* the recommendation is the caller's.
//!
//! # Pipeline
//!
//! ```text
//!      firehose (any EventStream)
//!             │
//!             ▼
//!   idiolect_indexer::drive_indexer
//!             │
//!             ▼
//!     CatalogHandler ──► Arc<Mutex<Catalog>>
//!                               ▲
//!                               │  (query::* functions)
//!                               │
//!                       caller's queries
//! ```
//!
//! # Scope
//!
//! - Ingests: `Adapter`, `Bounty`, `Community`, `Dialect`,
//!   `Recommendation`, `Verification`.
//! - Ignores (observer territory): `Encounter`, `Correction`,
//!   `Retrospection`, `Observation`.
//! - Ignores (vendored): `dev.panproto.*`.
//!
//! Records not in the catalog's kind set are dropped silently because
//! the firehose is shared — it is normal for other handlers to want
//! those kinds.
//!
//! # Quickstart
//!
//! ```
//! use idiolect_indexer::{
//!     InMemoryCursorStore, InMemoryEventStream, IndexerConfig, drive_indexer,
//! };
//! use idiolect_orchestrator::{CatalogHandler, query};
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! let handler = CatalogHandler::new();
//! let catalog_handle = handler.catalog();
//!
//! let mut stream = InMemoryEventStream::new();
//! let cursors = InMemoryCursorStore::new();
//!
//! drive_indexer(&mut stream, &handler, &cursors, &IndexerConfig::default())
//!     .await
//!     .unwrap();
//!
//! // read out cataloged records after firehose drains.
//! let catalog = catalog_handle.lock().unwrap();
//! assert!(query::open_bounties(&catalog).is_empty());
//! # }
//! ```

pub mod catalog;
#[cfg(feature = "catalog-sqlite")]
pub mod catalog_sqlite;
pub mod error;
pub mod expr_eval;
/// Generated from `orchestrator-spec/queries.json` by
/// `idiolect-codegen`. Do not edit by hand; regenerate via
/// `cargo run -p idiolect-codegen -- generate`.
pub mod generated;
pub mod handler;
#[cfg(feature = "query-http")]
pub mod http;
pub mod predicates;
pub mod query;

pub use catalog::{Catalog, CatalogRef, Entry};
#[cfg(feature = "catalog-sqlite")]
pub use catalog_sqlite::SqliteCatalogStore;
pub use error::{OrchestratorError, OrchestratorResult};
pub use handler::CatalogHandler;
#[cfg(feature = "query-http")]
pub use http::{AppState, router as http_router};
