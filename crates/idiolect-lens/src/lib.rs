//! Tier-1 lens runtime for idiolect.
//!
//! This crate resolves `dev.panproto.schema.lens` records — the
//! vendored panproto lens type re-exported by [`idiolect_records`] —
//! from three backing stores:
//!
//! - [`InMemoryResolver`] for fixtures and unit tests,
//! - [`PdsResolver`] for PDS fetches (via a pluggable [`PdsClient`]),
//! - [`PanprotoVcsResolver`] for panproto vcs stores (via a pluggable
//!   [`PanprotoVcsClient`], with [`InMemoryVcsClient`] shipped for
//!   tests and offline fixtures).
//!
//! A production-ready `PdsClient` backed by atrium-api + reqwest is
//! available behind the `pds-atrium` cargo feature as
//! [`AtriumPdsClient`]; with the feature off the core crate stays
//! free of any http or atproto transport dependency.
//!
//! On top of those resolvers, [`apply_lens`] fetches the source
//! record, loads the source and target schemas, applies the protolens
//! expression (single step or multi-step chain), and returns the
//! translated value together with its complement (the data `get`
//! discarded, which [`apply_lens_put`] needs to reconstruct the
//! source).
//!
//! The runtime covers every flavor of panproto lens:
//!
//! - **State-based**: [`apply_lens`] / [`apply_lens_put`] for
//!   whole-record translation. This is also the entry point for
//!   dependent optics: panproto's `WTypeFibration` models the
//!   schema-theory projection as a Grothendieck fibration whose
//!   cartesian lift is `put` and whose opcartesian lift is `get`, so
//!   the same two functions cover that structure.
//! - **Edit-based**: [`apply_lens_get_edit`] / [`apply_lens_put_edit`]
//!   for incremental translation of [`panproto_inst::TreeEdit`]
//!   sequences through a stateful [`panproto_lens::EditLens`].
//! - **Symmetric**: [`apply_lens_symmetric`] builds a
//!   [`panproto_lens::SymmetricLens`] from two lens records that
//!   share a source (middle) schema and runs either leg.
//!
//! # Quickstart
//!
//! ```
//! use idiolect_lens::{InMemoryResolver, Resolver, parse_at_uri};
//! use idiolect_records::PanprotoLens;
//!
//! let uri = parse_at_uri("at://did:plc:x/dev.panproto.schema.lens/l1").unwrap();
//! let lens = PanprotoLens {
//!     blob: None,
//!     created_at: "2026-04-19T00:00:00.000Z".to_owned(),
//!     laws_verified: None,
//!     object_hash: "sha256:deadbeef".to_owned(),
//!     round_trip_class: None,
//!     source_schema: "sha256:aaa".to_owned(),
//!     target_schema: "sha256:bbb".to_owned(),
//! };
//!
//! let mut r = InMemoryResolver::new();
//! r.insert(&uri, lens);
//!
//! let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
//! let got = rt.block_on(r.resolve(&uri)).unwrap();
//! assert_eq!(got.object_hash, "sha256:deadbeef");
//! ```

pub mod at_uri;
pub mod caching_resolver;
pub mod error;
pub mod fetcher;
#[cfg(feature = "pds-atrium")]
pub mod pds_atrium;
pub mod publisher;
#[cfg(feature = "pds-resolve")]
pub mod resolve;
#[cfg(feature = "pds-reqwest")]
pub mod signing_writer;
#[cfg(feature = "dpop-p256")]
pub mod dpop_p256;
pub mod verifying_resolver;
#[cfg(feature = "pds-reqwest")]
pub mod pds_reqwest;
pub mod resolver;
pub mod runtime;
pub mod schema_loader;

pub use at_uri::{AtUri, parse_at_uri};
pub use caching_resolver::CachingResolver;
pub use error::LensError;
pub use fetcher::{ListedEntry, ListedPage, RecordFetcher};
pub use publisher::RecordPublisher;
#[cfg(feature = "pds-atrium")]
pub use pds_atrium::AtriumPdsClient;
#[cfg(feature = "pds-reqwest")]
pub use pds_reqwest::ReqwestPdsClient;
#[cfg(feature = "pds-reqwest")]
pub use signing_writer::{
    AuthScheme, DpopProver, NoOpDpopProver, SigningPdsWriter, StaticDpopProver,
};
#[cfg(feature = "dpop-p256")]
pub use dpop_p256::P256DpopProver;
#[cfg(feature = "pds-resolve")]
pub use resolve::{fetcher_for_did, publisher_for_did, publisher_for_did_with_client};
pub use resolver::{
    CreateRecordRequest, DeleteRecordRequest, InMemoryResolver, InMemoryVcsClient,
    ListRecordsResponse, ListedRecord, PanprotoVcsClient, PanprotoVcsResolver, PdsClient,
    PdsResolver, PdsWriter, PutRecordRequest, Resolver, WriteRecordResponse,
};
pub use runtime::{
    ApplyLensEditInput, ApplyLensEditOutput, ApplyLensInput, ApplyLensOutput, ApplyLensPutInput,
    ApplyLensPutOutput, ApplyLensSymmetricInput, ApplyLensSymmetricOutput, LensBody,
    SymmetricDirection, apply_lens, apply_lens_get_edit, apply_lens_put, apply_lens_put_edit,
    apply_lens_symmetric,
};
pub use schema_loader::{FilesystemSchemaLoader, InMemorySchemaLoader, SchemaLoader};
pub use verifying_resolver::{Hasher, Sha256Hasher, VerifyingResolver};
