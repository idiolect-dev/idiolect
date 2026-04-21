//! Target-language emitter trait.
//!
//! A [`TargetEmitter`] renders the full set of files for one target
//! language given the parsed lexicons and discovered fixtures. Each
//! target owns its own intermediate representation: the rust target
//! builds a `syn::File` via `quote!` and renders with `prettyplease`;
//! the typescript target builds a small in-house ast and renders with
//! a hand-rolled printer.
//!
//! # Why a trait?
//!
//! This crate's codegen pipeline is conceptually
//!
//! ```text
//!   lexicon_json
//!     -> [parse]   -> LexiconDoc                          // fidelity-preserving view
//!     -> [lens]    -> target-language ast (syn, in-house) // by-construction
//!     -> [render]  -> source bytes                        // via prettyplease / printer
//! ```
//!
//! Today the lens + render halves live inside each target impl.
//! When `panproto-parse` grows a by-construction pretty-printer for
//! its tree-sitter grammars, a second impl of this trait can route
//! through a `panproto_schema::Schema` in the tree-sitter-rust /
//! tree-sitter-typescript theory and hand off the render step to
//! panproto. The trait boundary is the swap point.

use crate::Example;
use crate::lexicon::LexiconDoc;

/// One emitted source file.
#[derive(Debug, Clone)]
pub struct EmittedFile {
    /// Path relative to the target's output directory
    /// (e.g. `"encounter.rs"` or `"records.ts"`).
    pub path: String,
    /// Full file contents.
    pub contents: String,
}

/// A backend that emits every source file for one target language.
///
/// Implementations must be pure with respect to `(docs, examples)`:
/// given the same inputs, they must produce the same output bytes,
/// so drift detection against a checked-in baseline is meaningful.
pub trait TargetEmitter: Send + Sync {
    /// Human-readable target name, e.g. `"rust"` or `"typescript"`.
    fn language(&self) -> &'static str;

    /// Render every file this target owns.
    ///
    /// # Errors
    ///
    /// Returns [`EmitError::Unsupported`] if the inputs contain a
    /// construct this target does not yet handle, and
    /// [`EmitError::InvalidAst`] if the intermediate ast produced by
    /// the lens phase does not parse as valid source in the target
    /// language (a bug in the emitter, not in the user's lexicon).
    fn emit(
        &self,
        docs: &[LexiconDoc],
        examples: &[Example],
    ) -> Result<Vec<EmittedFile>, EmitError>;
}

/// Errors emitted during target rendering.
#[derive(Debug, thiserror::Error)]
pub enum EmitError {
    /// The lens produced a structure the renderer doesn't know how to print.
    #[error("unsupported construct in {target}: {detail}")]
    Unsupported {
        /// Target language name.
        target: &'static str,
        /// Free-form detail.
        detail: String,
    },

    /// An intermediate construction produced syntactically invalid rust
    /// or typescript. This indicates a bug in the lens / builder, not
    /// in the user's lexicon.
    #[error("{target} ast construction produced invalid source: {source}")]
    InvalidAst {
        /// Target language name.
        target: &'static str,
        /// Underlying syn / parse error.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}
