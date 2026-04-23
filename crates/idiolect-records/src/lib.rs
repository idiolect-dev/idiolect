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
//! use [`decode_record`] to decode into the [`AnyRecord`] variant:
//!
//! ```
//! use idiolect_records::{AnyRecord, decode_record, Record, Encounter};
//! let json: serde_json::Value = serde_json::from_str(r#"{
//!   "lens":         { "uri": "at://did:plc:x/dev.idiolect.lens/1" },
//!   "sourceSchema": { "uri": "at://did:plc:x/dev.idiolect.schema/a" },
//!   "purpose":      { "action": "translate_source_to_target" },
//!   "kind":         "invocation-log",
//!   "visibility":   "public-detailed",
//!   "occurredAt":   "2026-04-19T00:00:00.000Z"
//! }"#).unwrap();
//! let rec = decode_record(Encounter::NSID, json).unwrap();
//! match rec {
//!     AnyRecord::Encounter(_) => {}
//!     _ => panic!("expected encounter"),
//! }
//! ```
//!
//! Fixtures for every record kind are exported from [`examples`] for
//! use in downstream tests without reinventing minimally-valid json.

pub mod generated;
pub mod nsid;
pub mod record;

// re-export every generated module at the crate root, plus each
// lexicon's main record type, so callers can write
// `idiolect_records::Encounter` instead of
// `idiolect_records::generated::encounter::Encounter`.
pub use generated::{
    Adapter, Bounty, Community, Correction, Dialect, Encounter, Observation, PanprotoCommit,
    PanprotoComplement, PanprotoLens, PanprotoLensAttestation, PanprotoProtolens,
    PanprotoProtolensChain, PanprotoRefUpdate, PanprotoRepo, PanprotoSchema, Recommendation,
    Retrospection, Verification, adapter, bounty, community, correction, defs, dialect, encounter,
    examples, observation, panproto_commit, panproto_complement, panproto_lens,
    panproto_lens_attestation, panproto_protolens, panproto_protolens_chain, panproto_ref_update,
    panproto_repo, panproto_schema, recommendation, retrospection, verification,
};
pub use record::{AnyRecord, DecodeError, Record, decode_record};
