//! Spec-driven codegen for crates that contain a taxonomy of
//! similarly-shaped items.
//!
//! Each taxonomy lives in a `<crate>-spec/` directory with two files:
//!
//! - `lexicon.json` — an atproto lexicon defining the spec's shape.
//!   Lives in `dev.idiolect.internal.spec.*` so it never surfaces as
//!   a PDS record.
//! - A spec instance (`queries.json`, `methods.json`, `runners.json`)
//!   — the declarative taxonomy itself.
//!
//! Codegen loads each lexicon through
//! `panproto_protocols::web_document::atproto::parse_lexicon`,
//! validates the spec instance against the parsed schema via
//! `panproto_inst::parse::parse_json`, and only then deserializes
//! into the Rust decl types and emits. The panproto round-trip is
//! the structural check: a spec shape that drifts from its lexicon
//! surfaces as a parse error before any Rust is emitted.
//!
//! Business logic lives in hand-written sibling modules
//! (`predicates.rs`, `semantics/`); the generated module is pure
//! wire-up — signatures, registrations, route tables.
//!
//! The three-layer stack every taxonomy-containing crate follows:
//!
//! 1. `<crate>-spec/lexicon.json` + `<crate>-spec/*.json` — the
//!    declarative taxonomy, panproto-validated.
//! 2. `<crate>/src/generated/` — emitted wire-up.
//! 3. `<crate>/src/predicates.rs` or `<crate>/src/semantics/` —
//!    hand-written business logic the generated layer invokes.
//!
//! Crates that participate today:
//!
//! - **`idiolect-orchestrator`** — catalog queries (filter semantics
//!   over one entity kind each). See [`orchestrator`].
//! - **`idiolect-observer`** — observation methods. See [`observer`].
//! - **`idiolect-verify`** — verification runners. See [`verify`].
//! - **`idiolect-cli`** — subcommand dispatcher, derived from the
//!   orchestrator's query spec. See [`cli`].

pub mod cli;
pub mod observer;
pub mod orchestrator;
pub mod verify;

use std::path::Path;

use anyhow::{Context, Result};
use panproto_inst::parse::parse_json;
use panproto_protocols::web_document::atproto::parse_lexicon;
use panproto_schema::Schema;
use proc_macro2::TokenStream;
use quote::{TokenStreamExt, quote};
use syn::parse_str;

/// Validate a spec instance against its lexicon.
///
/// Loads `lexicon_path` through `parse_lexicon`, loads `spec_path`
/// as JSON, and runs `parse_json` against the parsed schema. A
/// successful parse confirms the spec's shape; the returned
/// `serde_json::Value` is the spec body, ready to deserialize into
/// the codegen's Rust decl types.
///
/// # Errors
///
/// Any IO, JSON, lexicon, or schema-parse failure surfaces with the
/// failing path stamped in the message.
pub fn validate_spec_through_panproto(
    lexicon_path: &Path,
    spec_path: &Path,
) -> Result<(Schema, serde_json::Value)> {
    let lex_raw = std::fs::read_to_string(lexicon_path)
        .with_context(|| format!("read {}", lexicon_path.display()))?;
    let lex_json: serde_json::Value = serde_json::from_str(&lex_raw)
        .with_context(|| format!("parse {}", lexicon_path.display()))?;
    let schema = parse_lexicon(&lex_json)
        .map_err(|e| anyhow::anyhow!("parse_lexicon({}): {e}", lexicon_path.display()))?;

    let spec_raw = std::fs::read_to_string(spec_path)
        .with_context(|| format!("read {}", spec_path.display()))?;
    let spec_json: serde_json::Value = serde_json::from_str(&spec_raw)
        .with_context(|| format!("parse {}", spec_path.display()))?;

    // Strip the `$schema` documentation hint before structural
    // validation — the lexicon does not declare it as a property.
    let mut spec_for_validation = spec_json.clone();
    if let Some(obj) = spec_for_validation.as_object_mut() {
        obj.remove("$schema");
    }

    let root = panproto_schema::primary_entry(&schema).ok_or_else(|| {
        anyhow::anyhow!(
            "lexicon {} has no primary entry vertex",
            lexicon_path.display()
        )
    })?;
    parse_json(&schema, root, &spec_for_validation).with_context(|| {
        format!(
            "spec {} failed structural validation against {}",
            spec_path.display(),
            lexicon_path.display()
        )
    })?;

    Ok((schema, spec_json))
}

/// Structured description of a generated Rust source file. Callers
/// build this up with typed helpers (doc lines, allow-lints, items,
/// extra inner attrs) instead of concatenating strings, then hand the
/// result to [`render_generated_file`].
///
/// The only piece that remains a raw string is the single
/// `// @generated ...` banner line — plain `//` line comments don't
/// round-trip through `syn` / `prettyplease` and that's precisely
/// what we want at the top of every generated file (it's tooling
/// metadata, not Rust prose).
pub struct GeneratedFile<'a> {
    /// Path (relative to the repo root) of the spec that generated
    /// this file. Stamped into the `@generated` banner and into parse
    /// error context.
    pub source_rel_path: &'a str,
    /// Module-level documentation, rendered as `//!` lines. May be
    /// multi-line; each line becomes its own `//!` in the output.
    pub inner_doc: &'a str,
    /// Lint names to silence at file scope, emitted as one
    /// `#![allow(a, b, c)]`. Pass full paths for clippy lints
    /// (`clippy::doc_markdown`) — the strings are parsed as syn paths,
    /// not interpolated into source.
    pub allow_lints: &'a [&'a str],
    /// Any additional inner attributes the caller wants on the file
    /// (e.g. `#![feature(...)]` — not used today but reserved so
    /// extending the header doesn't require editing this type).
    pub extra_inner_attrs: Vec<TokenStream>,
    /// Top-level items in file order.
    pub items: Vec<TokenStream>,
}

impl<'a> GeneratedFile<'a> {
    /// Constructor with just the required fields. Fills the optional
    /// lists with sensible empty defaults.
    #[must_use]
    pub const fn new(source_rel_path: &'a str, inner_doc: &'a str) -> Self {
        Self {
            source_rel_path,
            inner_doc,
            allow_lints: &[],
            extra_inner_attrs: Vec::new(),
            items: Vec::new(),
        }
    }

    /// Replace the `allow_lints` slice (builder style).
    #[must_use]
    pub const fn with_allow_lints(mut self, lints: &'a [&'a str]) -> Self {
        self.allow_lints = lints;
        self
    }

    /// Replace the items list (builder style).
    #[must_use]
    pub fn with_items(mut self, items: Vec<TokenStream>) -> Self {
        self.items = items;
        self
    }
}

/// Common set of lint allows shared across every spec-driven file
/// today. Offered as a reasonable default — callers that need
/// different lint surfaces pass their own slice to
/// [`GeneratedFile::with_allow_lints`] instead of mutating a global.
pub const DEFAULT_GENERATED_ALLOW_LINTS: &[&str] = &[
    "missing_docs",
    "clippy::doc_markdown",
    "clippy::too_many_lines",
];

/// Build the `#![allow(...)]` inner attribute `TokenStream` from a
/// list of lint paths. Each string is parsed as a `syn::Path`, so invalid
/// paths fail loudly at codegen time rather than producing malformed
/// Rust that the downstream rustc run has to diagnose.
///
/// # Errors
///
/// Returns an error if any lint name fails to parse as a
/// `syn::Path` — programmer error in the caller.
pub fn allow_lints_attr(lints: &[&str]) -> Result<TokenStream> {
    if lints.is_empty() {
        return Ok(TokenStream::new());
    }
    let mut paths: Vec<TokenStream> = Vec::with_capacity(lints.len());
    for lint in lints {
        let path: syn::Path =
            parse_str(lint).with_context(|| format!("parse lint path `{lint}` as syn::Path"))?;
        paths.push(quote! { #path });
    }
    Ok(quote! { #![allow(#(#paths),*)] })
}

/// Render a [`GeneratedFile`] to a formatted Rust source string.
///
/// Every non-banner component is built as `TokenStream` and parsed
/// through `syn::File` / `prettyplease`, so there is no string
/// concatenation of Rust source. The `//!` module doc lines are
/// emitted as `#![doc = "…"]` inner attributes — `prettyplease`
/// prints them back as real `//!` comments in the output.
///
/// # Errors
///
/// Returns an error when the assembled token stream fails to parse
/// as a `syn::File`, or when an allow-lint name isn't a valid syn
/// path.
pub fn render_generated_file(file: GeneratedFile<'_>) -> Result<String> {
    let GeneratedFile {
        source_rel_path,
        inner_doc,
        allow_lints,
        extra_inner_attrs,
        items,
    } = file;

    let mut inner_attrs: Vec<TokenStream> = Vec::new();
    for line in inner_doc.lines() {
        // Prepend a space so prettyplease emits `//! text` with the
        // conventional separator instead of `//!text`. An already
        // leading-space'd line is left alone.
        let padded = if line.starts_with(' ') || line.is_empty() {
            line.to_owned()
        } else {
            format!(" {line}")
        };
        inner_attrs.push(quote! { #![doc = #padded] });
    }
    let allow_attr = allow_lints_attr(allow_lints)?;
    if !allow_attr.is_empty() {
        inner_attrs.push(allow_attr);
    }
    inner_attrs.extend(extra_inner_attrs);

    let mut file_tokens = TokenStream::new();
    file_tokens.append_all(inner_attrs);
    file_tokens.append_all(items);

    let syn_file: syn::File = syn::parse2(file_tokens)
        .with_context(|| format!("parse generated tokens ({source_rel_path})"))?;
    let body = prettyplease::unparse(&syn_file);

    let banner =
        format!("// @generated by idiolect-codegen from {source_rel_path}. do not edit.\n\n");
    Ok(crate::rustfmt_source(&format!("{banner}{body}")))
}

/// Backward-compatible wrapper used by emitters that haven't yet
/// moved to [`render_generated_file`]. Applies
/// [`DEFAULT_GENERATED_ALLOW_LINTS`] to the file.
///
/// # Errors
///
/// Same conditions as [`render_generated_file`].
pub fn render_file_with_source(
    inner_doc: &str,
    source_rel_path: &str,
    items: Vec<TokenStream>,
) -> Result<String> {
    render_generated_file(
        GeneratedFile::new(source_rel_path, inner_doc)
            .with_allow_lints(DEFAULT_GENERATED_ALLOW_LINTS)
            .with_items(items),
    )
}
