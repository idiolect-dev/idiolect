//! Observer method spec → Rust codegen.
//!
//! Reads `observer-spec/methods.json` and emits
//! `crates/idiolect-observer/src/generated.rs` containing:
//!
//! - `pub mod` re-exports for every method module.
//! - A `METHODS` slice of `MethodDescriptor { name, version, description }`.
//! - A `default_methods()` convenience that returns boxed-trait instances
//!   of every bundled method, ready to register on a driver.
//!
//! Hand-written semantics live in
//! `crates/idiolect-observer/src/methods/<module>.rs`; this codegen
//! never touches them.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct MethodSpec {
    #[serde(default)]
    pub description: Option<String>,
    pub methods: Vec<MethodDecl>,
}

#[derive(Debug, Deserialize)]
pub struct MethodDecl {
    pub name: String,
    pub module: String,
    #[serde(rename = "struct")]
    pub struct_name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Which trait the module's struct implements. `record`
    /// (default) = `ObservationMethod`, instantiated by
    /// `default_methods()`. `instance` = `InstanceMethod`, not
    /// auto-instantiated (the adapter needs a schema resolver which
    /// is a deployment concern); descriptor appears in `METHODS`.
    #[serde(default = "default_form")]
    pub form: MethodForm,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MethodForm {
    Record,
    Instance,
}

const fn default_form() -> MethodForm {
    MethodForm::Record
}

pub fn load_spec(lexicon_path: &Path, spec_path: &Path) -> Result<MethodSpec> {
    let (_schema, spec_json) =
        super::validate_spec_through_panproto(lexicon_path, spec_path)?;
    let spec: MethodSpec = serde_json::from_value(spec_json)
        .with_context(|| format!("deserialize {}", spec_path.display()))?;
    validate(&spec)?;
    Ok(spec)
}

fn validate(spec: &MethodSpec) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for m in &spec.methods {
        if !seen.insert(&m.name) {
            bail!("duplicate method name: {}", m.name);
        }
    }
    Ok(())
}

/// Emit the observer's generated.rs into `observer_src`.
pub fn emit(spec: &MethodSpec, observer_src: &Path) -> Result<Vec<PathBuf>> {
    let out = observer_src.join("generated.rs");
    std::fs::write(&out, emit_source(spec)?)
        .with_context(|| format!("write {}", out.display()))?;
    Ok(vec![out])
}

fn emit_source(spec: &MethodSpec) -> Result<String> {
    // Build descriptor rows.
    let descriptor_rows: Vec<TokenStream> = spec
        .methods
        .iter()
        .map(|m| {
            let module = format_ident!("{}", m.module);
            let desc_lit = m.description.as_deref().unwrap_or("");
            let form_variant = match m.form {
                MethodForm::Record => format_ident!("Record"),
                MethodForm::Instance => format_ident!("Instance"),
            };
            quote! {
                MethodDescriptor {
                    name: crate::methods::#module::METHOD_NAME,
                    version: crate::methods::#module::METHOD_VERSION,
                    description: #desc_lit,
                    form: MethodForm::#form_variant,
                }
            }
        })
        .collect();

    // Record-form method constructors for default_methods().
    let default_exprs: Vec<TokenStream> = spec
        .methods
        .iter()
        .filter(|m| matches!(m.form, MethodForm::Record))
        .map(|m| {
            let module = format_ident!("{}", m.module);
            let ty = format_ident!("{}", m.struct_name);
            quote! { Box::new(crate::methods::#module::#ty::new()) }
        })
        .collect();

    let items: Vec<TokenStream> = vec![
        quote! {
            #![allow(unused_imports)]

            /// Whether a method's `observe` receives the raw event (record-form)
            /// or a panproto WInstance (instance-form).
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub enum MethodForm {
                /// `observe(&IndexerEvent)` — implements `crate::method::ObservationMethod`.
                Record,
                /// `observe(&WInstance, nsid)` — implements `crate::instance_method::InstanceMethod`.
                Instance,
            }

            /// Static descriptor for a bundled observation method.
            #[derive(Debug, Clone, Copy)]
            pub struct MethodDescriptor {
                /// Canonical method name.
                pub name: &'static str,
                /// Version string read from `<module>::METHOD_VERSION`.
                pub version: &'static str,
                /// Spec-sourced description.
                pub description: &'static str,
                /// Which trait the method implements.
                pub form: MethodForm,
            }

            /// Every bundled observation method's descriptor, in spec order.
            pub const METHODS: &[MethodDescriptor] = &[
                #(#descriptor_rows,)*
            ];

            /// Fresh instances of every bundled record-form method, in spec order.
            ///
            /// Instance-form methods are not included — they need a caller-supplied
            /// schema resolver passed to
            /// [`crate::instance_method::InstanceMethodAdapter`]. See [`METHODS`]
            /// for their descriptors.
            #[must_use]
            pub fn default_methods() -> Vec<Box<dyn crate::method::ObservationMethod>> {
                vec![ #(#default_exprs),* ]
            }
        },
    ];

    let inner_doc = spec
        .description
        .as_deref()
        .unwrap_or("Observer method registry.");
    super::render_file_with_source(inner_doc, "observer-spec/methods.json", items)
}
