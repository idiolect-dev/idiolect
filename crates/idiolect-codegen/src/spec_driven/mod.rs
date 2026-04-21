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
use quote::quote;

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
        anyhow::anyhow!("lexicon {} has no primary entry vertex", lexicon_path.display())
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

/// Render a list of top-level token streams as a Rust source file,
/// formatted via `prettyplease` and prefaced with an
/// `@generated`/source banner plus inner doc lines.
///
/// Shared by the per-crate emitters so the generated-file preamble
/// is identical across the spec-driven outputs.
///
/// # Errors
///
/// Returns an error when any token stream fails to parse as a
/// `syn::File` — a programmer error in the emitter, not a spec
/// issue.
pub fn render_file_with_source(
    inner_doc: &str,
    source_rel_path: &str,
    items: Vec<TokenStream>,
) -> Result<String> {
    let mut rendered = Vec::with_capacity(items.len());
    for item in items {
        let file: syn::File = syn::parse2(quote! { #item }).with_context(|| {
            format!("parse generated token stream ({source_rel_path})")
        })?;
        rendered.push(prettyplease::unparse(&file));
    }
    let body = rendered.join("\n");

    let mut out = String::with_capacity(body.len() + inner_doc.len() + 256);
    use std::fmt::Write as _;
    writeln!(
        out,
        "// @generated by idiolect-codegen from {source_rel_path}. do not edit.\n"
    )
    .expect("write to String");
    for line in inner_doc.lines() {
        out.push_str("//! ");
        out.push_str(line);
        out.push('\n');
    }
    // Generated code inherits the spec file's documentation; per-
    // item docstrings are not emitted because every item's meaning
    // is already stated in the spec. Silence the workspace's
    // missing-docs warn for this module.
    out.push_str("\n#![allow(missing_docs, clippy::doc_markdown)]\n");
    out.push('\n');
    out.push_str(&body);
    Ok(out)
}
