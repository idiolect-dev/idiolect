// idiolect-codegen is publish = false: its public types exist only
// to wire up the binary and the spec-driven emitters, and the spec
// files document the fields semantically. Relax the public-API doc
// lints that would otherwise demand per-field docstrings on decl
// structs that already round-trip a lexicon.
#![allow(missing_docs, clippy::missing_errors_doc)]

//! Lexicon-driven codegen library for idiolect.
//!
//! Idiolect dogfoods its own hyperdeclarative principle: lexicons in
//! `lexicons/dev/idiolect/*.json` are the canonical source of truth,
//! and the Rust record types (in `idiolect-records`) and TypeScript
//! interfaces (in `@idiolect/schema`) are derived views emitted from
//! them by this crate.
//!
//! The pipeline has two complementary passes:
//!
//! 1. A lightweight in-house parser ([`lexicon::parse`]) recovers a
//!    typed view of the lexicon json shaped for emitter ergonomics
//!    (nested object defs, record key strategy, property-order-
//!    preserving structs). The target emitters in [`emit`] consume
//!    this view.
//!
//! 2. The panproto atproto parser is invoked in parallel so that
//!    the same lexicons land in a `panproto_schema::Schema` graph
//!    which `panproto_check::diff` / `classify` consume for
//!    breaking-change detection. This keeps panproto on the hot
//!    path and anchors idiolect's version-control story.
//!
//! As of panproto v0.35.0, `parse_lexicon` preserves atproto string
//! refinements (`format`, `knownValues`) as constraints on the string
//! vertex — the codegen's in-house parser originally existed to
//! recover those refinements, but that recovery is no longer panproto's
//! responsibility to miss. The in-house parser is retained because it
//! is still more ergonomic for emitter shape (inline object defs, field
//! order), not because of a fidelity gap. See
//! `tests/panproto_fidelity.rs` for the pinned upstream contract.
//!
//! Downstream binaries combine the two: `idiolect-codegen` walks
//! the `lexicons/` tree, runs both passes, writes generated sources,
//! and (with `--check`) diffs the schema graphs against a baseline.
//!
//! # Target architecture
//!
//! The `emit` module is organised around a [`target::TargetEmitter`]
//! trait. Today we ship two impls:
//!
//! - [`emit::rust::RustTarget`] builds a `syn::File` by `quote!`ing
//!   each generated item, then renders via `prettyplease`.
//! - [`emit::typescript::TypeScriptTarget`] builds a small in-house
//!   ast and walks it with a hand-rolled printer.
//!
//! The trait is the swap-point for a future panproto-native
//! by-construction pretty-printer: a third impl would route through
//! a `panproto_schema::Schema` in the tree-sitter-rust /
//! tree-sitter-typescript theory and hand off to panproto's
//! `emit_pretty`. No call-site changes in this crate.
//!
//! # Examples module
//!
//! Example fixtures discovered under `lexicons/dev/idiolect/examples/`
//! are surfaced in an `examples` module on both the Rust and TS
//! sides, so appview authors can use minimally-valid records in
//! tests and integration fixtures without reinventing them.

pub mod emit;
pub mod lexicon;
pub mod spec_driven;
pub mod target;

/// One discovered record fixture.
///
/// The filesystem convention is
/// `lexicons/dev/idiolect/examples/<record>.json` where `<record>`
/// is the last segment of the record's nsid. Fixtures are ordinary
/// lexicon records with an extra `"$nsid"` field; both emitters
/// tolerate unknown fields on deserialize, so this marker is purely
/// an aid when reading the fixture file by hand.
#[derive(Debug, Clone)]
pub struct Example {
    /// Full nsid the fixture is an example of, e.g.
    /// `dev.idiolect.encounter`.
    pub nsid: String,
    /// Repo-relative path to the fixture file (used by the Rust
    /// emitter to write `include_str!(...)` pointing at the repo
    /// root).
    pub repo_relative_path: String,
    /// Raw json contents (used by the TS emitter to inline the
    /// example as a const).
    pub json: String,
}
