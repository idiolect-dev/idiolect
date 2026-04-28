//! Lexicon → target-language source orchestration.
//!
//! This module ties each [`TargetEmitter`] to the typed lexicon view
//! and fixture list loaded by `main.rs`. Today we ship two targets:
//!
//! - [`rust::RustTarget`] (syn + prettyplease)
//! - [`typescript::TypeScriptTarget`] (in-house ast + hand-rolled printer)
//!
//! Additional targets (or a by-construction panproto pretty-printer
//! swap-in) plug in by providing another `impl TargetEmitter`. The
//! orchestration here doesn't need to change.

use anyhow::Result;

use crate::Example;
use crate::lexicon::LexiconDoc;
use crate::target::{EmittedFile, TargetEmitter};

pub mod family;
pub mod rust;
pub mod typescript;

/// Emit the full rust file set for the supplied lexicons + fixtures.
///
/// `family` controls which records (by NSID prefix) populate the
/// generated family module. Idiolect's binary passes
/// [`family::idiolect_family`]; downstream consumers construct their
/// own via [`family::FamilyConfig::new`].
///
/// # Errors
///
/// Propagates any error from the rust target's emit pass — in
/// practice, only `syn` ast-construction failures which indicate a
/// bug in the lens rather than a user-lexicon issue.
pub fn emit_rust(
    docs: &[LexiconDoc],
    examples: &[Example],
    family: &family::FamilyConfig,
) -> Result<Vec<EmittedFile>> {
    Ok(rust::RustTarget.emit(docs, examples, family)?)
}

/// Emit the full typescript file set for the supplied lexicons + fixtures.
///
/// `family` is reserved for the upcoming TS family module that
/// mirrors `family.rs`; the current implementation accepts the
/// argument for trait uniformity but the emitted output is still
/// derived purely from `docs` and `examples`.
///
/// # Errors
///
/// Propagates any error from the typescript target's emit pass. None
/// of the current builders can fail, but the signature is kept result-
/// shaped so the orchestration can absorb a future panproto-backed
/// typescript target that has fallible render steps.
pub fn emit_typescript(
    docs: &[LexiconDoc],
    examples: &[Example],
    family: &family::FamilyConfig,
) -> Result<Vec<EmittedFile>> {
    Ok(typescript::TypeScriptTarget.emit(docs, examples, family)?)
}
