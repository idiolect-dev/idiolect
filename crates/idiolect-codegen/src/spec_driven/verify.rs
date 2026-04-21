//! Verify runner spec → Rust codegen.
//!
//! Reads `verify-spec/runners.json` and emits
//! `crates/idiolect-verify/src/generated.rs` containing:
//!
//! - A `RUNNERS` slice of `RunnerDescriptor { kind, module, description }`.
//! - A `runner_kinds()` helper returning the `VerificationKind` values
//!   the crate has shipped runners for.
//!
//! Unlike the observer's `default_methods()`, no factory helper is
//! emitted here — each runner's constructor takes different
//! configuration (a corpus, a budget, a generator closure) that
//! cannot be supplied declaratively.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct RunnerSpec {
    #[serde(default)]
    pub description: Option<String>,
    pub runners: Vec<RunnerDecl>,
}

#[derive(Debug, Deserialize)]
pub struct RunnerDecl {
    pub kind: String,
    pub module: String,
    #[serde(rename = "struct")]
    pub struct_name: String,
    #[serde(default)]
    pub description: Option<String>,
}

pub fn load_spec(lexicon_path: &Path, spec_path: &Path) -> Result<RunnerSpec> {
    let (_schema, spec_json) =
        super::validate_spec_through_panproto(lexicon_path, spec_path)?;
    let spec: RunnerSpec = serde_json::from_value(spec_json)
        .with_context(|| format!("deserialize {}", spec_path.display()))?;
    validate(&spec)?;
    Ok(spec)
}

fn validate(spec: &RunnerSpec) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for r in &spec.runners {
        if !seen.insert(&r.kind) {
            bail!("duplicate runner kind: {}", r.kind);
        }
    }
    Ok(())
}

pub fn emit(spec: &RunnerSpec, verify_src: &Path) -> Result<Vec<PathBuf>> {
    let out = verify_src.join("generated.rs");
    std::fs::write(&out, emit_source(spec)?)
        .with_context(|| format!("write {}", out.display()))?;
    Ok(vec![out])
}

fn emit_source(spec: &RunnerSpec) -> Result<String> {
    let descriptor_rows: Vec<TokenStream> = spec
        .runners
        .iter()
        .map(|r| {
            let kind = &r.kind;
            let module = &r.module;
            let desc = r.description.as_deref().unwrap_or("");
            quote! {
                RunnerDescriptor {
                    kind: #kind,
                    module: #module,
                    description: #desc,
                }
            }
        })
        .collect();

    let kind_variants: Vec<TokenStream> = spec
        .runners
        .iter()
        .map(|r| {
            let variant = format_ident!("{}", kebab_to_pascal(&r.kind));
            quote! { VerificationKind::#variant }
        })
        .collect();

    let items = vec![quote! {
        use idiolect_records::generated::verification::VerificationKind;

        /// Static descriptor for a shipped verification runner.
        #[derive(Debug, Clone, Copy)]
        pub struct RunnerDescriptor {
            /// Kebab-case kind matching the lexicon taxonomy.
            pub kind: &'static str,
            /// Module name containing the runner impl.
            pub module: &'static str,
            /// Spec-sourced description.
            pub description: &'static str,
        }

        /// Every shipped verification runner's descriptor, in spec order.
        pub const RUNNERS: &[RunnerDescriptor] = &[
            #(#descriptor_rows,)*
        ];

        /// Return the `VerificationKind` values the crate has shipped runners for.
        #[must_use]
        pub fn runner_kinds() -> Vec<VerificationKind> {
            vec![ #(#kind_variants),* ]
        }
    }];

    let inner_doc = spec
        .description
        .as_deref()
        .unwrap_or("Verification runner registry.");
    super::render_file_with_source(inner_doc, "verify-spec/runners.json", items)
}

fn kebab_to_pascal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper_next = true;
    for ch in s.chars() {
        if ch == '-' {
            upper_next = true;
        } else if upper_next {
            out.push(ch.to_ascii_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}
