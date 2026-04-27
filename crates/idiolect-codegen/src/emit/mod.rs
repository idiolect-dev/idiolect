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
/// # Errors
///
/// Propagates any error from the rust target's emit pass — in
/// practice, only `syn` ast-construction failures which indicate a
/// bug in the lens rather than a user-lexicon issue.
pub fn emit_rust(docs: &[LexiconDoc], examples: &[Example]) -> Result<Vec<EmittedFile>> {
    Ok(rust::RustTarget.emit(docs, examples)?)
}

/// Emit the full typescript file set for the supplied lexicons + fixtures.
///
/// # Errors
///
/// Propagates any error from the typescript target's emit pass. None
/// of the current builders can fail, but the signature is kept result-
/// shaped so the orchestration can absorb a future panproto-backed
/// typescript target that has fallible render steps.
pub fn emit_typescript(docs: &[LexiconDoc], examples: &[Example]) -> Result<Vec<EmittedFile>> {
    Ok(typescript::TypeScriptTarget.emit(docs, examples)?)
}
