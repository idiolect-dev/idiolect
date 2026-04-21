//! Panproto-native schema and adapter traits for atproto OAuth
//! session state.
//!
//! # Why this crate exists
//!
//! Idiolect's core bet is that every value the appview touches is a
//! panproto w-instance of a panproto [`Schema`](panproto_schema::Schema).
//! For records published to a PDS that bet is trivially true — the
//! lexicons in `lexicons/dev/idiolect/` parse directly into
//! panproto schemas. But plenty of appview state never reaches a
//! PDS: firehose cursors, identity-resolution caches, `DPoP` nonces,
//! and — the subject of this crate — OAuth session state.
//!
//! Tokens are sensitive and PDS-scoped; publishing them would be a
//! security bug. Treating them as one-off "ephemeral state" would
//! be an ergonomic bug, because now the appview has two fundamentally
//! different value kinds: panproto-shaped records and non-panproto
//! session state. This crate closes the gap: it defines a **panproto
//! side-channel schema** for session state, so the token lifecycle
//! (issue, refresh, revoke) becomes ordinary lens application over
//! w-instances — the same vocabulary everything else in the appview
//! already speaks.
//!
//! # Schema
//!
//! The session schema is authored as an atproto Lexicon at
//! [`SESSION_LEXICON_JSON`] (bundled via `include_str!`). The nsid
//! is `dev.idiolect.internal.oauthSession`. At runtime the lexicon
//! parses through [`panproto_protocols::web_document::atproto::parse_lexicon`]
//! into a [`panproto_schema::Schema`], which you can hand to an
//! `InMemorySchemaLoader` or any other [`idiolect_lens::SchemaLoader`].
//!
//! # Struct mirror
//!
//! [`OAuthSession`] is the serde-friendly Rust mirror. Its json wire
//! form matches the schema's body vertex exactly, so the struct can
//! travel both directions:
//!
//! - forward: serialize the struct, route through a lens, land a
//!   target w-instance that can be re-deserialized.
//! - backward: a target w-instance (possibly modified by the lens)
//!   deserializes into a fresh `OAuthSession` directly.
//!
//! # Storage
//!
//! The [`OAuthTokenStore`] trait is the persistence boundary.
//! Production backends (browser `localStorage`, sqlite, Redis, …)
//! implement the three-method `save` / `load` / `delete` surface;
//! tests use [`InMemoryOAuthTokenStore`].
//!
//! # Quickstart
//!
//! ```
//! use idiolect_oauth::{
//!     InMemoryOAuthTokenStore, OAuthSession, OAuthTokenStore,
//!     SESSION_NSID, session_schema,
//! };
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! // the schema is a real panproto::Schema graph.
//! let schema = session_schema().expect("bundled schema parses");
//! assert!(!schema.vertices.is_empty());
//!
//! // a session round-trips through the storage boundary.
//! let session = OAuthSession::new(
//!     "did:plc:alice",
//!     "https://pds.example",
//!     "access-jwt",
//!     "refresh-jwt",
//!     "dpop-jwk",
//!     "2026-04-19T00:00:00.000Z",
//!     "2026-04-19T01:00:00.000Z",
//! );
//!
//! let store = InMemoryOAuthTokenStore::new();
//! store.save(&session).await.unwrap();
//! let loaded = store.load("did:plc:alice").await.unwrap();
//! assert_eq!(loaded.unwrap().pds_url, "https://pds.example");
//! assert_eq!(SESSION_NSID, "dev.idiolect.internal.oauthSession");
//! # }
//! ```

pub mod schema;
pub mod session;
pub mod store;

#[cfg(feature = "store-filesystem")]
pub mod store_fs;
#[cfg(feature = "store-sqlite")]
pub mod store_sqlite;

pub use schema::{
    SESSION_LEXICON_JSON, SESSION_NSID, SchemaError, session_body_vertex, session_schema,
};
pub use session::{OAuthSession, SessionError};
pub use store::{InMemoryOAuthTokenStore, OAuthTokenStore, StoreError};

#[cfg(feature = "store-filesystem")]
pub use store_fs::FilesystemOAuthTokenStore;
#[cfg(feature = "store-sqlite")]
pub use store_sqlite::SqliteOAuthTokenStore;
