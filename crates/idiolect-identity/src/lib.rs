//! DID resolution for idiolect.
//!
//! Most idiolect subsystems need to go from a DID to one of:
//!
//! - the repo's PDS URL (for record fetches and writes),
//! - the identity's current handle (for display),
//! - the DID document verbatim (for signature verification).
//!
//! This crate supplies that mapping over one narrow trait —
//! [`IdentityResolver`] — with two shipped implementations:
//!
//! - [`InMemoryIdentityResolver`] for tests and fixtures.
//! - [`ReqwestIdentityResolver`] (behind `resolver-reqwest`) for live
//!   traffic. Speaks plc.directory for `did:plc:*` and
//!   `.well-known/did.json` for `did:web:*`. The plc directory URL is
//!   configurable so callers can point at a self-hosted mirror.
//!
//! Callers who need caching wrap a resolver themselves; the trait is
//! narrow enough that a caching layer is a thin wrapper.
//!
//! # Quickstart (in-memory)
//!
//! ```
//! use idiolect_identity::{Did, DidDocument, IdentityResolver, InMemoryIdentityResolver};
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! let did = Did::parse("did:plc:alice").unwrap();
//! let mut resolver = InMemoryIdentityResolver::new();
//! resolver.insert(&did, DidDocument {
//!     id: did.as_str().to_owned(),
//!     also_known_as: vec!["at://alice.test".to_owned()],
//!     service: vec![idiolect_identity::Service {
//!         id: "#atproto_pds".to_owned(),
//!         service_type: "AtprotoPersonalDataServer".to_owned(),
//!         service_endpoint: "https://pds.example".to_owned(),
//!     }],
//!     verification_method: vec![],
//!     extras: Default::default(),
//! });
//!
//! let pds = resolver.resolve_pds_url(&did).await.unwrap();
//! assert_eq!(pds, "https://pds.example");
//! # }
//! ```

pub mod caching;
pub mod did;
pub mod document;
pub mod error;
#[cfg(feature = "resolver-reqwest")]
pub mod reqwest_resolver;
pub mod resolver;

pub use caching::CachingIdentityResolver;
pub use did::{Did, DidError, DidMethod};
pub use document::{DidDocument, Service};
pub use error::IdentityError;
pub use resolver::{IdentityResolver, InMemoryIdentityResolver};

#[cfg(feature = "resolver-reqwest")]
pub use reqwest_resolver::ReqwestIdentityResolver;
