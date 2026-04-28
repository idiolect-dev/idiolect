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

use std::fmt::Write as _;

use anyhow::Result;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Ident;

use crate::Example;
use crate::lexicon::{
    Def, LexiconDoc, ObjectDef, Prop, PropType, RecordDef, RefTarget, StringEnumDef, UnionDef,
    is_rust_keyword, module_name_for_nsid, module_path_for_nsid,
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
        family: &super::family::FamilyConfig,
    ) -> Result<Vec<EmittedFile>, EmitError> {
        let mut out = Vec::with_capacity(docs.len() + 2);

        for doc in docs {
            let contents = render_lexicon_file(doc)?;
            out.push(EmittedFile {
                path: format!("{}.rs", module_path_for_nsid(&doc.nsid).join("/")),
                contents: rustfmt(&contents)?,
            });
        }

        for (dir, contents) in render_directory_mod_files(docs) {
            out.push(EmittedFile {
                path: dir,
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
        // Generated family module: AnyRecord, decode_record, and
        // the RecordFamily impl. Adding a record to the family is
        // now a one-line lexicon change; the hand-written record.rs
        // shrinks to the Record trait + DecodeError.
        out.push(EmittedFile {
            path: "family.rs".to_owned(),
            contents: rustfmt(&super::family::render_family_rs(docs, family).map_err(|e| {
                EmitError::InvalidAst {
                    target: "rust",
                    source: e.into(),
                }
            })?)?,
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
        let field_ident = if is_rust_keyword(&snake) {
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

fn emit_open_string_enum(ty_name: &str, def: &StringEnumDef) -> TokenStream {
    let ty_ident = format_ident!("{}", ty_name);
    let doc_text = def.description.clone().unwrap_or_else(|| {
        format!(
            "{ty_name}. Open-enum slug; known values are kebab-cased; community-extended values pass through as `Other(String)`."
        )
    });
    let doc = doc_attr(&doc_text);

    let known_idents: Vec<_> = def
        .values
        .iter()
        .map(|v| format_ident!("{}", sanitize_variant_ident(v)))
        .collect();
    let known_kebab: Vec<&str> = def.values.iter().map(String::as_str).collect();
    // The fallback variant carries a community-extended slug. Try
    // common names in order; pick the first that does not collide
    // with any known variant. Final fallback uses a numeric suffix
    // to guarantee uniqueness even for pathological enums whose
    // knownValues already include `Other`, `Extended`, `Custom`,
    // and `Variant`.
    let candidate_fallbacks = ["Other", "Extended", "Custom", "Variant"];
    let fallback_name: String = if let Some(name) = candidate_fallbacks
        .iter()
        .find(|c| !known_idents.iter().any(|i| i == *c))
    {
        (*name).to_owned()
    } else {
        // Numeric-suffixed fallback. Bounded to a sane ceiling so
        // pathological inputs (every integer name taken) error
        // loudly rather than spinning forever.
        (0u32..1024)
            .map(|n| format!("Other{n}"))
            .find(|c| !known_idents.iter().any(|i| i == c))
            .expect("ceiling is high enough for any plausible enum")
    };
    let fallback_ident = format_ident!("{}", fallback_name);

    let variants = known_idents.iter().map(|ident| quote! { #ident, });

    let to_str_arms: Vec<TokenStream> = known_idents
        .iter()
        .zip(&known_kebab)
        .map(|(ident, kebab)| quote! { Self::#ident => #kebab, })
        .collect();
    let from_str_arms: Vec<TokenStream> = known_idents
        .iter()
        .zip(&known_kebab)
        .map(|(ident, kebab)| quote! { #kebab => Self::#ident, })
        .collect();
    let from_str_arms_dup = from_str_arms.clone();

    quote! {
        #doc
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum #ty_ident {
            #(#variants)*
            /// Community-extended slug not present in the lexicon's
            /// `knownValues`. Resolves through the sibling
            /// `*Vocab` field on the containing record.
            #fallback_ident(String),
        }

        impl #ty_ident {
            /// Wire-form slug for this value. Known variants render
            /// kebab-case; the fallback variant passes through verbatim.
            #[must_use]
            pub fn as_str(&self) -> &str {
                match self {
                    #(#to_str_arms)*
                    Self::#fallback_ident(s) => s.as_str(),
                }
            }
        }

        impl From<String> for #ty_ident {
            fn from(s: String) -> Self {
                match s.as_str() {
                    #(#from_str_arms)*
                    _ => Self::#fallback_ident(s),
                }
            }
        }

        impl From<&str> for #ty_ident {
            fn from(s: &str) -> Self {
                match s {
                    #(#from_str_arms_dup)*
                    _ => Self::#fallback_ident(s.to_owned()),
                }
            }
        }

        impl serde::Serialize for #ty_ident {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for #ty_ident {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let s = String::deserialize(deserializer)?;
                Ok(Self::from(s))
            }
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
    OpenStringEnum {
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
            Self::StringEnum { .. } | Self::OpenStringEnum { .. } => 1,
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
        InlineType::OpenStringEnum {
            name,
            description,
            values,
        } => emit_open_string_enum(
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
        PropType::String => (quote! { String }, None),
        PropType::CidLink => (quote! { idiolect_records::Cid }, None),
        PropType::StringLanguage => (quote! { idiolect_records::Language }, None),
        PropType::StringDatetime => (quote! { idiolect_records::Datetime }, None),
        PropType::StringAtUri => (quote! { idiolect_records::AtUri }, None),
        PropType::StringDid => (quote! { idiolect_records::Did }, None),
        PropType::StringNsid => (quote! { idiolect_records::Nsid }, None),
        PropType::StringUri => (quote! { idiolect_records::Uri }, None),
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
        PropType::InlineOpenStringEnum(values) => {
            let name = format!("{}{}", parent_ty, pascal_case(prop_name));
            let name_ident = format_ident!("{}", &name);
            let inline = InlineType::OpenStringEnum {
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
        return quote! { #ident };
    }
    // Cross-lexicon: address the type by its absolute path under
    // `crate::generated::<segment>::<...>::<Type>`. The lexicon-tree
    // file layout means we cannot safely use `super::` chains — a
    // ref from `dev.idiolect.encounter` to `dev.panproto.schema.lens`
    // would need a different number of `super`s than a ref staying
    // inside `dev.idiolect`. Absolute paths sidestep that entirely.
    let segments: Vec<Ident> = module_path_for_nsid(&target.nsid)
        .into_iter()
        .map(|s| rust_path_ident(&s))
        .collect();
    let ty = if target.def_name == "main" {
        format_ident!("{}", pascal_case(&module_name_for_nsid(&target.nsid)))
    } else {
        format_ident!("{}", pascal_case(&target.def_name))
    };
    quote! { crate::generated::#(#segments)::*::#ty }
}

/// Build a Rust `Ident` for a path segment, raw-prefixing it when the
/// segment is a Rust keyword. Keyword segments are legal in NSIDs (a
/// lexicon under `pub.layers.*` is the running example) and surface
/// in cross-lexicon ref paths whenever those NSIDs appear.
fn rust_path_ident(seg: &str) -> Ident {
    if is_rust_keyword(seg) {
        Ident::new_raw(seg, proc_macro2::Span::call_site())
    } else {
        format_ident!("{}", seg)
    }
}

// ---------- aggregating files ----------

fn render_mod_rs(docs: &[LexiconDoc]) -> String {
    use std::collections::BTreeSet;

    let mut out = String::new();
    out.push_str("// @generated by idiolect-codegen. do not edit.\n\n");
    out.push_str(
        "//! Rust types generated from the `dev.idiolect.*` lexicons plus the vendored\n\
         //! `dev.panproto.*` tree (see `lexicons/dev/panproto/VENDORED.md`).\n\
         //!\n\
         //! The on-disk layout mirrors the lexicon directory tree under\n\
         //! `lexicons/`: each lexicon `dev/<authority>/<...>/<name>.json`\n\
         //! emits a corresponding `dev/<authority>/<...>/<name>.rs`.\n\
         //! Record types are re-exported at the crate root so existing\n\
         //! callers keep using `idiolect_records::Encounter` etc.\n\n",
    );
    out.push_str("#![allow(missing_docs)]\n\n");

    // Top-level sub-modules are the unique first segments of every
    // emitted lexicon path, plus the hand-written `examples` module.
    let mut roots: BTreeSet<String> = BTreeSet::new();
    for doc in docs {
        if let Some(first) = module_path_for_nsid(&doc.nsid).into_iter().next() {
            roots.insert(first);
        }
    }
    for r in &roots {
        let ident = if is_rust_keyword(r) {
            format!("r#{r}")
        } else {
            r.clone()
        };
        out.push_str(&format!("pub mod {ident};\n"));
    }
    out.push_str("pub mod examples;\n");
    out.push_str("pub mod family;\n\n");

    // Re-export every lexicon's main record type at the crate root.
    // When two records under different parent namespaces share a leaf
    // TypeName (e.g. `pub.layers.changelog.entry::Entry` and
    // `pub.layers.resource.entry::Entry`), an unaliased re-export
    // would collide (E0252). Walk-up disambiguation produces unique
    // prefixed aliases only for the colliding groups.
    let mut records: Vec<&LexiconDoc> = docs
        .iter()
        .filter(|d| matches!(d.defs.get("main"), Some(Def::Record(_))))
        .collect();
    records.sort_by(|a, b| a.nsid.cmp(&b.nsid));
    let prepared: Vec<(Vec<String>, String)> = records
        .iter()
        .map(|d| {
            (
                rust_module_path(&d.nsid),
                pascal_case(&module_name_for_nsid(&d.nsid)),
            )
        })
        .collect();
    let aliases = walk_up_aliases(&prepared);
    for (i, (path, ty)) in prepared.iter().enumerate() {
        match &aliases[i] {
            Some(alias) => {
                let _ = writeln!(out, "pub use {}::{ty} as {alias};", path.join("::"));
            }
            None => {
                let _ = writeln!(out, "pub use {}::{ty};", path.join("::"));
            }
        }
    }
    out
}

/// `module_path_for_nsid` segments with raw-identifier escaping
/// applied to any segment that collides with a Rust keyword (`pub`,
/// `mod`, etc.). Used by every emitter that needs a fully-qualified
/// `crate::generated::<…>` path to a per-record module.
pub(super) fn rust_module_path(nsid: &str) -> Vec<String> {
    module_path_for_nsid(nsid)
        .into_iter()
        .map(|s| {
            if is_rust_keyword(&s) {
                format!("r#{s}")
            } else {
                s
            }
        })
        .collect()
}

/// Compute walk-up disambiguating aliases for `(path, type_name)` pairs.
///
/// `aliases[i]` is `Some(prefix + ty)` when `type_name` `i` collides
/// with another in the slice; `None` when its leaf is unique.
pub(super) fn walk_up_aliases(prepared: &[(Vec<String>, String)]) -> Vec<Option<String>> {
    use std::collections::BTreeMap;

    let mut by_ty: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, (_, ty)) in prepared.iter().enumerate() {
        by_ty.entry(ty.as_str()).or_default().push(i);
    }
    let mut aliases: Vec<Option<String>> = vec![None; prepared.len()];
    for indices in by_ty.values() {
        if indices.len() < 2 {
            continue;
        }
        let mut take = 1;
        loop {
            let suffixes: Vec<String> = indices
                .iter()
                .map(|&i| suffix_for(&prepared[i].0, take))
                .collect();
            let unique: std::collections::BTreeSet<&String> = suffixes.iter().collect();
            let exhausted = indices
                .iter()
                .all(|&i| take >= prepared[i].0.len().saturating_sub(1));
            if unique.len() == indices.len() || exhausted {
                for &i in indices {
                    aliases[i] = Some(prefixed_alias(&prepared[i], take));
                }
                break;
            }
            take += 1;
        }
    }
    aliases
}

fn suffix_for(path: &[String], take: usize) -> String {
    let leaf_idx = path.len().saturating_sub(1);
    let start = leaf_idx.saturating_sub(take);
    path[start..leaf_idx].join("/")
}

fn prefixed_alias((path, ty): &(Vec<String>, String), take: usize) -> String {
    let leaf_idx = path.len().saturating_sub(1);
    let start = leaf_idx.saturating_sub(take);
    let prefix: String = path[start..leaf_idx]
        .iter()
        .map(|s| pascal_case(s.trim_start_matches("r#")))
        .collect();
    format!("{prefix}{ty}")
}

/// Per-directory `mod.rs` files. For every internal directory in the
/// lexicon tree (e.g. `dev/`, `dev/idiolect/`, `dev/panproto/`,
/// `dev/panproto/schema/`), emit a `mod.rs` that declares its
/// immediate children. The leaf `.rs` files are emitted by the main
/// loop; this helper only produces the intermediate index files that
/// stitch the tree into a compilable module graph.
fn render_directory_mod_files(docs: &[LexiconDoc]) -> Vec<(String, String)> {
    use std::collections::BTreeMap;

    // dir-segments → set of immediate children (each child is the next
    // segment underneath that dir).
    let mut tree: BTreeMap<Vec<String>, std::collections::BTreeSet<String>> = BTreeMap::new();

    for doc in docs {
        let path = module_path_for_nsid(&doc.nsid);
        if path.is_empty() {
            continue;
        }
        // Every prefix `path[..i]` is an internal directory whose
        // immediate child is `path[i]`. The full path's leaf module
        // (`path[..len-1]` + `path[len-1]`) is also recorded as a
        // child of its parent dir.
        for i in 0..path.len() {
            let dir = path[..i].to_vec();
            let child = path[i].clone();
            tree.entry(dir).or_default().insert(child);
        }
    }

    // The empty-dir entry would correspond to the root `mod.rs`,
    // which is rendered separately; skip it here.
    tree.remove(&Vec::new());

    let mut files = Vec::with_capacity(tree.len());
    for (dir, children) in tree {
        let mut body = String::new();
        body.push_str("// @generated by idiolect-codegen. do not edit.\n\n");
        body.push_str("#![allow(missing_docs)]\n\n");
        let dir_leaf = dir.last().map(String::as_str);
        for child in children {
            // A child whose name equals its parent directory is the
            // lexicon-tree expression of an NSID like
            // `dev.panproto.schema.schema`: directory `schema/` plus
            // file `schema.rs`. The compiler accepts it; clippy's
            // module_inception lint flags it. Suppress on the line.
            if dir_leaf == Some(child.as_str()) {
                body.push_str("#[allow(clippy::module_inception)]\n");
            }
            let ident = if is_rust_keyword(&child) {
                format!("r#{child}")
            } else {
                child.clone()
            };
            body.push_str(&format!("pub mod {ident};\n"));
        }
        let path = format!("{}/mod.rs", dir.join("/"));
        files.push((path, body));
    }
    files
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

/// Pascal-case a slug for use as an open-enum variant identifier,
/// stripping characters that are not legal in a Rust identifier
/// (dots, slashes, colons, etc.). The original slug is preserved
/// at the wire-form match arm; this is purely the in-source name.
fn sanitize_variant_ident(slug: &str) -> String {
    let mut out = String::with_capacity(slug.len());
    let mut upper_next = true;
    for ch in slug.chars() {
        if ch.is_ascii_alphanumeric() {
            if upper_next {
                out.push(ch.to_ascii_uppercase());
                upper_next = false;
            } else {
                out.push(ch);
            }
        } else {
            // Any non-alphanumeric (`-`, `_`, ` `, `.`, `/`, `:`, …)
            // becomes a word boundary; the next alpha is uppercased.
            upper_next = true;
        }
    }
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        // Pure-numeric or empty slugs need a prefix to be valid idents.
        format!("V{out}")
    } else {
        out
    }
}

pub(crate) fn pascal_case(s: &str) -> String {
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

#[cfg(test)]
mod re_export_tests {
    use super::*;
    use crate::lexicon::{Def, LexiconDoc, ObjectDef, RecordDef};
    use std::collections::BTreeMap;

    fn record_doc(nsid: &str) -> LexiconDoc {
        let mut defs = BTreeMap::new();
        defs.insert(
            "main".to_owned(),
            Def::Record(RecordDef {
                description: None,
                key: None,
                body: ObjectDef {
                    description: None,
                    required: Vec::new(),
                    properties: Vec::new(),
                },
            }),
        );
        LexiconDoc {
            nsid: nsid.to_owned(),
            description: None,
            defs,
        }
    }

    #[test]
    fn unique_leaf_typenames_emit_unaliased_re_exports() {
        let docs = vec![
            record_doc("pub.layers.changelog.entry"),
            record_doc("pub.layers.persona.persona"),
        ];
        let out = render_mod_rs(&docs);
        assert!(
            out.contains("pub use r#pub::layers::changelog::entry::Entry;\n"),
            "expected unaliased Entry re-export, got:\n{out}"
        );
        assert!(out.contains("pub use r#pub::layers::persona::persona::Persona;\n"));
        assert!(
            !out.contains(" as "),
            "no aliasing expected when leaves unique"
        );
    }

    #[test]
    fn collision_disambiguates_with_walk_up_alias() {
        // Two records share leaf TypeName `Entry`; the parent segment
        // (`changelog`/`resource`) is enough to disambiguate.
        let docs = vec![
            record_doc("pub.layers.changelog.entry"),
            record_doc("pub.layers.resource.entry"),
        ];
        let out = render_mod_rs(&docs);
        assert!(
            out.contains("pub use r#pub::layers::changelog::entry::Entry as ChangelogEntry;\n"),
            "missing disambiguated changelog alias:\n{out}"
        );
        assert!(
            out.contains("pub use r#pub::layers::resource::entry::Entry as ResourceEntry;\n"),
            "missing disambiguated resource alias:\n{out}"
        );
    }
}

#[cfg(test)]
mod sanitize_tests {
    use super::sanitize_variant_ident;

    #[test]
    fn alphanumeric_pascal_cases() {
        assert_eq!(sanitize_variant_ident("foo-bar"), "FooBar");
        assert_eq!(sanitize_variant_ident("foo_bar"), "FooBar");
        assert_eq!(sanitize_variant_ident("FooBar"), "FooBar");
    }

    #[test]
    fn dotted_slug_strips_punctuation() {
        // The layers-pub fixture has knownValue "chive.pub"; the
        // dot must not leak into the Rust ident.
        assert_eq!(sanitize_variant_ident("chive.pub"), "ChivePub");
    }

    #[test]
    fn slash_and_colon_are_separators() {
        assert_eq!(sanitize_variant_ident("foo/bar"), "FooBar");
        assert_eq!(sanitize_variant_ident("foo:bar"), "FooBar");
    }

    #[test]
    fn leading_separator_pascal_cases_first_alpha() {
        assert_eq!(sanitize_variant_ident("-foo"), "Foo");
        assert_eq!(sanitize_variant_ident("..foo"), "Foo");
    }

    #[test]
    fn consecutive_separators_collapse() {
        assert_eq!(sanitize_variant_ident("foo--bar"), "FooBar");
        assert_eq!(sanitize_variant_ident("foo. .bar"), "FooBar");
    }

    #[test]
    fn digit_leading_slug_gets_v_prefix() {
        // Slugs starting with a digit produce invalid idents
        // without a prefix; the sanitizer adds `V`.
        assert_eq!(sanitize_variant_ident("123abc"), "V123abc");
        assert_eq!(sanitize_variant_ident("9"), "V9");
    }

    #[test]
    fn empty_after_strip_becomes_v() {
        assert_eq!(sanitize_variant_ident("..."), "V");
        assert_eq!(sanitize_variant_ident(""), "V");
    }

    #[test]
    fn unicode_is_treated_as_separator() {
        // Non-ASCII chars are stripped (treated as separators) so
        // the emitted ident is always a valid Rust identifier even
        // if the original slug carried Greek or CJK characters.
        assert_eq!(sanitize_variant_ident("αβγ"), "V");
        assert_eq!(sanitize_variant_ident("foo-αβ-bar"), "FooBar");
    }
}

#[cfg(test)]
mod open_enum_tests {
    use super::*;
    use crate::lexicon::StringEnumDef;

    fn render(values: &[&str]) -> String {
        let def = StringEnumDef {
            description: None,
            values: values.iter().map(|s| (*s).to_owned()).collect(),
        };
        let tokens = emit_open_string_enum("TestKind", &def);
        tokens.to_string()
    }

    #[test]
    fn fallback_is_other_when_no_collision() {
        let out = render(&["foo", "bar"]);
        assert!(out.contains("Other (String)"), "expected Other variant in:\n{out}");
        assert!(!out.contains("Extended (String)"));
    }

    #[test]
    fn fallback_falls_back_to_extended_on_other_collision() {
        let out = render(&["other", "foo"]);
        assert!(
            out.contains("Extended (String)"),
            "expected Extended fallback in:\n{out}"
        );
        // The known `Other` variant must remain.
        assert!(out.contains("Other ,"));
    }

    #[test]
    fn fallback_falls_back_to_custom_on_other_and_extended_collision() {
        let out = render(&["other", "extended"]);
        assert!(
            out.contains("Custom (String)"),
            "expected Custom fallback in:\n{out}"
        );
    }

    #[test]
    fn known_value_serializes_to_original_kebab() {
        // Ensures underscored / kebab slugs preserve their wire form
        // through as_str arms.
        let out = render(&["subsumed_by", "broader-than"]);
        assert!(
            out.contains("Self :: SubsumedBy => \"subsumed_by\""),
            "expected verbatim wire form for underscored slug:\n{out}"
        );
        assert!(
            out.contains("Self :: BroaderThan => \"broader-than\""),
            "expected verbatim wire form for kebab slug:\n{out}"
        );
    }

    #[test]
    fn dotted_slug_renders_distinct_variant() {
        let out = render(&["chive.pub", "wikidata"]);
        assert!(
            out.contains("ChivePub ,"),
            "expected ChivePub variant from dotted slug:\n{out}"
        );
        assert!(
            out.contains("\"chive.pub\""),
            "expected verbatim chive.pub wire form:\n{out}"
        );
    }
}
