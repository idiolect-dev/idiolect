//! Rust record types mirroring the `dev.idiolect.*` Lexicon family.
//!
//! This crate's types are **generated** from the canonical lexicon
//! json in `lexicons/dev/idiolect/*.json` by `idiolect-codegen`.
//! Per the hyperdeclarative principle (P2: records-not-registries),
//! the lexicons are the single source of truth; Rust, TypeScript,
//! and SQL views are derived from them.
//!
//! Do not edit the contents of [`generated`] by hand. Re-run
//! `cargo run -p idiolect-codegen` after changing any lexicon.
//!
//! # Appview quickstart
//!
//! Every record type implements [`Record`], so indexers, xrpc
//! handlers, and tests can be generic over `R: Record`:
//!
//! ```
//! use idiolect_records::{Encounter, Record};
//! fn describe<R: Record>() -> String {
//!     format!("{} ({})", R::kind(), R::NSID)
//! }
//! assert_eq!(describe::<Encounter>(), "encounter (dev.idiolect.encounter)");
//! ```
//!
//! When the nsid is only known at runtime (e.g. firehose traffic),
//! parse it into [`Nsid`] and dispatch via [`decode_record`]:
//!
//! ```
//! use idiolect_records::{AnyRecord, decode_record, Encounter, Nsid, Record};
//! let json: serde_json::Value = serde_json::from_str(r#"{
//!   "lens":         { "uri": "at://did:plc:x/dev.idiolect.lens/1" },
//!   "sourceSchema": { "uri": "at://did:plc:x/dev.idiolect.schema/a" },
//!   "use":          { "action": "translate_source_to_target" },
//!   "kind":         "invocation-log",
//!   "visibility":   "public-detailed",
//!   "occurredAt":   "2026-04-19T00:00:00.000Z"
//! }"#).unwrap();
//! let nsid = Encounter::nsid();
//! let rec = decode_record(&nsid, json).unwrap();
//! match rec {
//!     AnyRecord::Encounter(_) => {}
//!     _ => panic!("expected encounter"),
//! }
//! ```
//!
//! Fixtures for every record kind are exported from [`examples`] for
//! use in downstream tests without reinventing minimally-valid json.

pub mod at_uri;
pub mod did;
pub mod generated;
pub mod nsid;
pub mod record;

pub use at_uri::{AtUri, AtUriError};
pub use did::{Did, DidError, DidMethod};
pub use nsid::{Nsid, NsidError};

// Forward every type and sub-module exposed by `generated::` to the
// crate root. The list of record-type aliases lives in the generated
// `mod.rs` (one `pub use dev::...::T` per lexicon main record) so it
// stays in sync with the lexicon set without a hand-edit step.
pub use generated::*;
pub use record::{AnyRecord, DecodeError, Record, decode_record};
