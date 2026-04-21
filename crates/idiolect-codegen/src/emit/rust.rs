//! Rust target: lexicon → `syn::File` → `prettyplease`.
//!
//! The flow is a lens in spirit even though the intermediate isn't
//! a `panproto_schema::Schema` today:
//!
//! ```text
//!   LexiconDoc                                    // typed input view
//!     -> [build_file]  -> Vec<syn::Item>          // by-construction rust ast
//!     -> [prettyplease::unparse] -> String        // formatted rust source
//! ```
//!
//! `syn` is the canonical rust ast; `quote!` builds token streams that
//! syn parses into items with the full correctness invariant enforced
//! by the grammar. `prettyplease` gives us rustfmt-grade formatting
//! without a separate pass. When a `panproto_schema::Schema` emitter
//! for the tree-sitter-rust grammar lands (see the panproto-issue
//! doc), a second impl of [`TargetEmitter`] can replace this one.

// generator-local allows: this module is a string/token composer, so
// the pedantic lints against `format!` into `String`, `if let/else`
// vs `map_or_else`, and similar, are noise here.
#![allow(
    clippy::format_push_string,
    clippy::option_if_let_else,
    clippy::branches_sharing_code,
    clippy::if_same_then_else,
    clippy::missing_const_for_fn,
    clippy::needless_pass_by_value
)]

use anyhow::Result;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Ident;

use crate::Example;
use crate::lexicon::{
    Def, LexiconDoc, ObjectDef, Prop, PropType, RecordDef, RefTarget, StringEnumDef, UnionDef,
    module_name_for_nsid,
};
use crate::target::{EmitError, EmittedFile, TargetEmitter};

// ---------- public entry ----------

/// Rust target — emits one `.rs` per lexicon plus `mod.rs` and
/// `examples.rs` in the crate's `generated/` directory.
pub struct RustTarget;

impl TargetEmitter for RustTarget {
    fn language(&self) -> &'static str {
        "rust"
    }

    fn emit(
        &self,
        docs: &[LexiconDoc],
        examples: &[Example],
    ) -> Result<Vec<EmittedFile>, EmitError> {
        let mut out = Vec::with_capacity(docs.len() + 2);

        for doc in docs {
            let contents = render_lexicon_file(doc)?;
            out.push(EmittedFile {
                path: format!("{}.rs", module_name_for_nsid(&doc.nsid)),
                contents: rustfmt(&contents)?,
            });
        }

        out.push(EmittedFile {
            path: "mod.rs".to_owned(),
            contents: rustfmt(&render_mod_rs(docs))?,
        });
        out.push(EmittedFile {
            path: "examples.rs".to_owned(),
            contents: rustfmt(&render_examples_rs(examples))?,
        });

        Ok(out)
    }
}

/// Run the host's `rustfmt` as a subprocess to canonicalise the
/// emitter's output. Prettyplease is not rustfmt-idempotent for every
/// construct we emit (notably `.expect(...)` chains wider than
/// 100 cols), and the hand-written `render_examples_rs` template
/// picks a shape rustfmt may rewrite; routing everything through
/// rustfmt means the checked-in files are a fixed point of `cargo fmt`
/// and the drift check stays honest.
fn rustfmt(source: &str) -> Result<String, EmitError> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let mut child = Command::new("rustfmt")
        .args(["--edition", "2024", "--emit", "stdout"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| EmitError::InvalidAst {
            target: "rust",
            source: Box::new(e),
        })?;

    child
        .stdin
        .as_mut()
        .expect("piped stdin")
        .write_all(source.as_bytes())
        .map_err(|e| EmitError::InvalidAst {
            target: "rust",
            source: Box::new(e),
        })?;

    let out = child
        .wait_with_output()
        .map_err(|e| EmitError::InvalidAst {
            target: "rust",
            source: Box::new(e),
        })?;

    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr).into_owned();
        return Err(EmitError::InvalidAst {
            target: "rust",
            source: format!("rustfmt exited with {}: {msg}", out.status).into(),
        });
    }

    String::from_utf8(out.stdout).map_err(|e| EmitError::InvalidAst {
        target: "rust",
        source: Box::new(e),
    })
}

// ---------- per-lexicon file rendering ----------

/// Render each top-level token stream via prettyplease individually,
/// then join with a blank line between items. Prettyplease's own output
/// packs items with no blank separation, which is unreadable for a
/// generated module; per-item rendering + join recovers rustfmt-style
/// spacing.
fn render_items(items: Vec<TokenStream>) -> Result<String, EmitError> {
    let mut rendered = Vec::with_capacity(items.len());
    for item in items {
        let file =
            syn::parse2::<syn::File>(quote! { #item }).map_err(|e| EmitError::InvalidAst {
                target: "rust",
                source: Box::new(e),
            })?;
        rendered.push(prettyplease::unparse(&file));
    }
    Ok(rendered.join("\n"))
}

fn render_lexicon_file(doc: &LexiconDoc) -> Result<String, EmitError> {
    let mut items: Vec<TokenStream> = Vec::new();

    // 1. emit main first (if present), deferring its inlines to the end.
    let main_inlines: Vec<InlineType> = if let Some(Def::Record(record)) = doc.defs.get("main") {
        let ty_name = pascal_case(module_name_for_nsid(&doc.nsid).as_str());
        let (struct_item, impl_item, inlines) = emit_record(&ty_name, &doc.nsid, record);
        items.push(struct_item);
        items.push(impl_item);
        inlines
    } else {
        Vec::new()
    };

    // 2. emit non-main defs in btree order, with their inlines immediately after.
    let mut pending_non_main_inlines: Vec<InlineType> = Vec::new();
    for (def_name, def) in &doc.defs {
        if def_name == "main" {
            continue;
        }
        let ty_name = pascal_case(def_name);
        match def {
            Def::Record(_) => {
                // lexicon spec: only `main` may be a record. silently skip
                // others — the lexicon parser should have already rejected.
            }
            Def::Object(obj) => {
                let (item, inlines) =
                    emit_object(&ty_name, &doc.nsid, obj, obj.description.as_deref());
                items.push(item);
                pending_non_main_inlines.extend(inlines);
            }
            Def::StringEnum(enm) => {
                items.push(emit_string_enum(&ty_name, enm));
            }
            Def::Union(uni) => {
                items.push(emit_union(&ty_name, &doc.nsid, uni));
            }
        }
    }

    // 3. emit non-main inlines.
    for inline in pending_non_main_inlines {
        items.push(render_inline(&inline, &doc.nsid));
    }

    // 4. emit main's deferred inlines last.
    for inline in main_inlines {
        items.push(render_inline(&inline, &doc.nsid));
    }

    let body = render_items(items)?;

    // prettyplease strips leading attributes-on-file headers in subtle
    // ways; we prepend the @generated banner + inner doc + module
    // allow-list as hand-written lines so they are stable.
    let module_doc = doc.description.as_deref().map_or_else(
        || format!("//! Generated from `{}`.\n\n", doc.nsid),
        |d| format!("//! {d}\n\n"),
    );

    let mut out = String::with_capacity(body.len() + module_doc.len() + 256);
    out.push_str("// @generated by idiolect-codegen. do not edit.\n");
    out.push_str(&format!("// source: {}\n\n", doc.nsid));
    out.push_str(&module_doc);
    out.push_str(
        "#![allow(\n    \
         missing_docs,\n    \
         clippy::doc_markdown,\n    \
         clippy::struct_excessive_bools,\n    \
         clippy::derive_partial_eq_without_eq\n\
         )]\n",
    );
    out.push_str("use serde::{Deserialize, Serialize};\n\n");
    out.push_str(&body);
    Ok(out)
}

// ---------- def emitters ----------

fn emit_record(
    ty_name: &str,
    nsid: &str,
    def: &RecordDef,
) -> (TokenStream, TokenStream, Vec<InlineType>) {
    let (struct_item, inlines) = emit_struct(ty_name, nsid, &def.body, def.description.as_deref());
    let ty_ident = format_ident!("{}", ty_name);
    let nsid_lit = nsid;
    let impl_item = quote! {
        impl crate::Record for #ty_ident {
            const NSID: &'static str = #nsid_lit;
        }
    };
    (struct_item, impl_item, inlines)
}

fn emit_object(
    ty_name: &str,
    nsid: &str,
    def: &ObjectDef,
    description: Option<&str>,
) -> (TokenStream, Vec<InlineType>) {
    emit_struct(ty_name, nsid, def, description)
}

fn emit_struct(
    ty_name: &str,
    nsid: &str,
    def: &ObjectDef,
    description: Option<&str>,
) -> (TokenStream, Vec<InlineType>) {
    let ty_ident = format_ident!("{}", ty_name);
    let doc = description.map(doc_attr);

    let mut field_tokens: Vec<TokenStream> = Vec::with_capacity(def.properties.len());
    let mut inlines: Vec<InlineType> = Vec::new();

    // sort properties alphabetically by name for stable output.
    let mut sorted_props: Vec<&(String, Prop)> = def.properties.iter().collect();
    sorted_props.sort_by(|a, b| a.0.cmp(&b.0));

    for (prop_name, prop) in sorted_props {
        let required = def.required.iter().any(|r| r == prop_name);
        let (tok, inline) = resolve_prop_type(&prop.ty, ty_name, prop_name, nsid);
        if let Some(inline) = inline {
            inlines.push(inline);
        }
        let snake = snake_case(prop_name);
        let field_ident = if is_rust_reserved(&snake) {
            format_ident!("r#{}", snake)
        } else {
            format_ident!("{}", snake)
        };
        let field_doc = prop.description.as_deref().map(doc_attr);
        let field_ty = if required {
            tok
        } else {
            quote! { Option<#tok> }
        };
        let serde_attrs = if required {
            quote! {}
        } else {
            quote! { #[serde(default, skip_serializing_if = "Option::is_none")] }
        };
        field_tokens.push(quote! {
            #field_doc
            #serde_attrs
            pub #field_ident: #field_ty,
        });
    }

    // categorical re-sort of inlines: unions first, then enums, then objects.
    inlines.sort_by_key(InlineType::category_order);

    let item = quote! {
        #doc
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct #ty_ident {
            #(#field_tokens)*
        }
    };
    (item, inlines)
}

fn emit_string_enum(ty_name: &str, def: &StringEnumDef) -> TokenStream {
    let ty_ident = format_ident!("{}", ty_name);
    let doc = def
        .description
        .as_deref()
        .map_or_else(|| doc_attr(&format!("{ty_name}.")), doc_attr);
    let variants = def.values.iter().map(|v| {
        let ident = format_ident!("{}", pascal_case(v));
        quote! { #ident, }
    });
    quote! {
        #doc
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "kebab-case")]
        pub enum #ty_ident {
            #(#variants)*
        }
    }
}

fn emit_union(ty_name: &str, current_nsid: &str, def: &UnionDef) -> TokenStream {
    let ty_ident = format_ident!("{}", ty_name);
    let doc_text = def
        .description
        .clone()
        .unwrap_or_else(|| format!("{ty_name} tagged union."));
    let doc = doc_attr(&doc_text);

    let variants = def.variants.iter().map(|variant| {
        let variant_ty = pascal_case(&variant.def_name);
        let variant_ident = format_ident!("{}", &variant_ty);
        let variant_ty_ref = ref_target_tokens(variant, current_nsid);
        let rename = if variant.nsid == current_nsid {
            format!("{}#{}", variant.nsid, variant.def_name)
        } else {
            format!("{}#{}", variant.nsid, variant.def_name)
        };
        quote! {
            #[serde(rename = #rename)]
            #variant_ident(#variant_ty_ref),
        }
    });

    quote! {
        #doc
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(tag = "$type")]
        pub enum #ty_ident {
            #(#variants)*
        }
    }
}

// ---------- inline type synthesis ----------

#[derive(Debug, Clone)]
enum InlineType {
    StringEnum {
        name: String,
        description: Option<String>,
        values: Vec<String>,
    },
    Object {
        name: String,
        description: Option<String>,
        def: Box<ObjectDef>,
    },
    Union {
        name: String,
        description: Option<String>,
        variants: Vec<RefTarget>,
    },
}

impl InlineType {
    /// 0 = union, 1 = enum, 2 = object — the order inlines appear
    /// in the existing generated output for each parent def.
    fn category_order(&self) -> u8 {
        match self {
            Self::Union { .. } => 0,
            Self::StringEnum { .. } => 1,
            Self::Object { .. } => 2,
        }
    }
}

fn render_inline(inline: &InlineType, current_nsid: &str) -> TokenStream {
    match inline {
        InlineType::StringEnum {
            name,
            description,
            values,
        } => emit_string_enum(
            name,
            &StringEnumDef {
                description: description.clone(),
                values: values.clone(),
            },
        ),
        InlineType::Object {
            name,
            description,
            def,
        } => {
            let (item, nested) = emit_object(name, current_nsid, def, description.as_deref());
            // nested-in-inline: flatten into the same stream.
            let nested_items = nested.iter().map(|i| render_inline(i, current_nsid));
            quote! {
                #item
                #(#nested_items)*
            }
        }
        InlineType::Union {
            name,
            description,
            variants,
        } => emit_union(
            name,
            current_nsid,
            &UnionDef {
                description: description.clone(),
                variants: variants.clone(),
            },
        ),
    }
}

// ---------- prop type resolution ----------

fn resolve_prop_type(
    ty: &PropType,
    parent_ty: &str,
    prop_name: &str,
    current_nsid: &str,
) -> (TokenStream, Option<InlineType>) {
    match ty {
        // cid-link renders as an atproto cid string on the wire; we
        // keep it a bare `String` like other string formats rather
        // than carrying a distinct newtype yet.
        PropType::String | PropType::StringDatetime | PropType::CidLink => {
            (quote! { String }, None)
        }
        PropType::Integer => (quote! { i64 }, None),
        PropType::Boolean => (quote! { bool }, None),
        PropType::Number => (quote! { f64 }, None),
        PropType::Bytes | PropType::Blob | PropType::Unknown => {
            // `unknown` accepts any json value; atproto's `bytes` / `blob`
            // wire shapes are also arbitrary json objects that we don't
            // model as distinct types yet. routing them through
            // `serde_json::Value` keeps deserialization total.
            (quote! { serde_json::Value }, None)
        }
        PropType::Ref(target) => (ref_target_tokens(target, current_nsid), None),
        PropType::Array(inner) => {
            let (inner_tok, inline) = resolve_prop_type(inner, parent_ty, prop_name, current_nsid);
            (quote! { Vec<#inner_tok> }, inline)
        }
        PropType::InlineStringEnum(values) => {
            let name = format!("{}{}", parent_ty, pascal_case(prop_name));
            let name_ident = format_ident!("{}", &name);
            let inline = InlineType::StringEnum {
                name,
                description: None,
                values: values.clone(),
            };
            (quote! { #name_ident }, Some(inline))
        }
        PropType::InlineUnion(variants) => {
            let name = format!("{}{}", parent_ty, pascal_case(prop_name));
            let name_ident = format_ident!("{}", &name);
            let inline = InlineType::Union {
                name,
                description: None,
                variants: variants.clone(),
            };
            (quote! { #name_ident }, Some(inline))
        }
        PropType::InlineObject(obj) => {
            let name = format!("{}{}", parent_ty, pascal_case(prop_name));
            let name_ident = format_ident!("{}", &name);
            let inline = InlineType::Object {
                name,
                description: obj.description.clone(),
                def: obj.clone(),
            };
            (quote! { #name_ident }, Some(inline))
        }
    }
}

fn ref_target_tokens(target: &RefTarget, current_nsid: &str) -> TokenStream {
    if target.nsid == current_nsid {
        // same lexicon: bare type name.
        let ident: Ident = format_ident!("{}", pascal_case(&target.def_name));
        quote! { #ident }
    } else {
        // cross-lexicon: super::<module>::<Type>
        let module = format_ident!("{}", module_name_for_nsid(&target.nsid));
        let ty = if target.def_name == "main" {
            // record refs resolve to the lexicon's main type.
            format_ident!("{}", pascal_case(&module_name_for_nsid(&target.nsid)))
        } else {
            format_ident!("{}", pascal_case(&target.def_name))
        };
        quote! { super::#module::#ty }
    }
}

// ---------- aggregating files ----------

fn render_mod_rs(docs: &[LexiconDoc]) -> String {
    let mut out = String::new();
    out.push_str("// @generated by idiolect-codegen. do not edit.\n\n");
    out.push_str(
        "//! Rust types generated from the `dev.idiolect.*` lexicons plus the vendored\n\
         //! `dev.panproto.*` tree (see `lexicons/dev/panproto/VENDORED.md`).\n\n",
    );
    out.push_str("#![allow(missing_docs)]\n\n");

    let mut modules: Vec<String> = docs.iter().map(|d| module_name_for_nsid(&d.nsid)).collect();
    modules.push("examples".to_owned());
    modules.sort();
    for m in &modules {
        out.push_str(&format!("pub mod {m};\n"));
    }
    out.push('\n');

    // re-export record types at the crate root for ergonomic call sites.
    let mut records: Vec<&LexiconDoc> = docs
        .iter()
        .filter(|d| matches!(d.defs.get("main"), Some(Def::Record(_))))
        .collect();
    records.sort_by(|a, b| a.nsid.cmp(&b.nsid));
    for doc in records {
        let m = module_name_for_nsid(&doc.nsid);
        let ty = pascal_case(&m);
        out.push_str(&format!("pub use {m}::{ty};\n"));
    }
    out
}

fn render_examples_rs(examples: &[Example]) -> String {
    let mut out = String::new();
    out.push_str("// @generated by idiolect-codegen. do not edit.\n\n");
    out.push_str(
        "//! Minimally-valid fixture records, surfaced from `lexicons/dev/*/examples/`.\n",
    );
    out.push_str("//!\n");
    out.push_str(
        "//! Each `*_JSON` const is the raw json fixture; each companion\n\
         //! function returns the deserialised record. Panics on invalid\n\
         //! json are by design — the roundtrip test suite catches drift\n\
         //! before anything lands.\n\n",
    );
    out.push_str("#![allow(missing_docs)]\n");
    out.push_str("#![allow(clippy::missing_panics_doc)]\n\n");

    let mut sorted: Vec<&Example> = examples.iter().collect();
    sorted.sort_by(|a, b| a.nsid.cmp(&b.nsid));

    for ex in sorted {
        let module = module_name_for_nsid(&ex.nsid);
        let upper = module.to_ascii_uppercase();
        let ty = pascal_case(&module);
        // Inline the fixture as a raw string literal. Embedding avoids
        // `include_str!` with parent-directory paths, which breaks when
        // cargo packages the crate for crates.io (the tarball contains
        // only the crate directory, not the workspace's lexicons/ tree).
        let pounds = fence_for(&ex.json);
        let fence = "#".repeat(pounds);

        out.push_str(&format!(
            "/// Raw json for the bundled `{nsid}` fixture.\n\
             pub const {upper}_JSON: &str = r{fence}\"{json}\"{fence};\n\n",
            nsid = ex.nsid,
            upper = upper,
            fence = fence,
            json = ex.json,
        ));
        out.push_str(&format!(
            "/// Deserialised `{nsid}` fixture. Panics if the bundled json is invalid.\n\
             #[must_use]\n\
             pub fn {module}() -> crate::{ty} {{\n    \
             serde_json::from_str({upper}_JSON)\n        \
             .expect(\"bundled {nsid} fixture deserialises\")\n\
             }}\n\n",
            nsid = ex.nsid,
            module = module,
            ty = ty,
            upper = upper,
        ));
    }
    out
}

// ---------- helpers ----------

/// Pick a `#` count for a raw-string fence that won't collide with any
/// `"#…#` substring inside `s`. Raw strings terminate on `"` followed
/// by the same count of `#` that opened them, so any shorter run inside
/// `s` is safe.
fn fence_for(s: &str) -> usize {
    let mut max_run = 0usize;
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let mut run = 0;
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] == b'#' {
                run += 1;
                j += 1;
            }
            if run > max_run {
                max_run = run;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    max_run + 1
}

/// Produce a `#[doc = " <text>"]` attribute. The leading space matches
/// rustfmt's convention for `/// ` rendering; prettyplease preserves
/// the string contents verbatim.
fn doc_attr(text: &str) -> TokenStream {
    let padded = format!(" {text}");
    quote! { #[doc = #padded] }
}

/// Strict-keyword list used to decide when a field ident needs the
/// raw prefix. Non-exhaustive; covers everything we actually hit in
/// the `dev.idiolect.*` lexicons.
fn is_rust_reserved(s: &str) -> bool {
    matches!(
        s,
        "type"
            | "match"
            | "move"
            | "ref"
            | "self"
            | "crate"
            | "super"
            | "where"
            | "async"
            | "await"
            | "dyn"
            | "fn"
            | "impl"
            | "loop"
            | "for"
            | "if"
            | "else"
            | "return"
            | "mod"
            | "pub"
            | "use"
            | "let"
            | "mut"
            | "const"
            | "static"
            | "struct"
            | "enum"
            | "union"
            | "trait"
            | "in"
            | "as"
            | "box"
            | "break"
            | "continue"
            | "false"
            | "true"
            | "extern"
            | "unsafe"
            | "while"
    )
}

fn pascal_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper_next = true;
    for ch in s.chars() {
        if ch == '-' || ch == '_' || ch == ' ' {
            upper_next = true;
        } else if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == ' ' {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    out
}
