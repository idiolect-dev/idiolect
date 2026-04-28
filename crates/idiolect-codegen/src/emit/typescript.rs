//! TypeScript target: lexicon → in-house ir → oxc ast → `oxc_codegen`.
//!
//! The lexicon is first lowered to the in-module IR ([`TsDecl`],
//! [`TsType`], [`Inline`]) that stages semantics (which fields are
//! required, which refs are cross-module, which inlines need their own
//! named type). Each IR declaration is then built as an `oxc_ast`
//! subtree and rendered via `oxc_codegen::Codegen`, so the typescript
//! bytes come from oxc's ast-to-source printer rather than hand
//! composition.
//!
//! `JSDoc` blocks ride oxc's comment system: each declaration assigns
//! a unique `attached_to` id, appends its jsdoc text into a per-decl
//! `source_text` buffer, and drops a matching `Comment` into the
//! program's comment vector. The codegen's `print_leading_comments`
//! hook then pulls those comments out by id and prints them above the
//! interface or before each property signature.
//!
//! File-level fixed bits — the `// @generated` header, the top-level
//! description, the blank lines between declarations — are still
//! stitched as text; the aggregate files (`index.ts`, `examples.ts`,
//! `records.ts`) keep string composition too because their payloads
//! (`typeof NSID[K]`, `as const`, tagged template literals) bloat the
//! ast builder pipeline without material robustness gains.

// generator-local allows: most pedantic lints flag style choices in
// this kind of string/ast composition code without flagging actual
// correctness issues.
#![allow(
    clippy::format_push_string,
    clippy::option_if_let_else,
    clippy::missing_const_for_fn,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    // source_text is our own buffer and never grows past u32::MAX;
    // oxc's comment model stores spans as u32 so a cast is required.
    clippy::cast_possible_truncation
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use oxc_allocator::{Allocator, Vec as OxcVec};
use oxc_ast::ast::{
    BindingIdentifier, CommentNewlines, Declaration, ImportDeclarationSpecifier,
    ImportOrExportKind, ModuleExportName, PropertyKey, Statement, TSLiteral, TSSignature, TSType,
};
use oxc_ast::{AstBuilder, Comment, CommentContent, CommentKind, CommentPosition, NONE};
use oxc_codegen::{Codegen, CodegenOptions, CommentOptions, IndentChar};
use oxc_span::{SPAN, SourceType, Span};

use crate::Example;
use crate::lexicon::{
    Def, LexiconDoc, ObjectDef, Prop, PropType, RefTarget, StringEnumDef, UnionDef,
    module_name_for_nsid, module_path_for_nsid,
};
use crate::target::{EmitError, EmittedFile, TargetEmitter};

/// TypeScript target — emits one `.ts` per lexicon plus
/// `index.ts`, `examples.ts`, and `records.ts`.
pub struct TypeScriptTarget;

impl TargetEmitter for TypeScriptTarget {
    fn language(&self) -> &'static str {
        "typescript"
    }

    fn emit(
        &self,
        docs: &[LexiconDoc],
        examples: &[Example],
        family: &super::family::FamilyConfig,
    ) -> Result<Vec<EmittedFile>, EmitError> {
        let mut out = Vec::with_capacity(docs.len() + 3);

        for doc in docs {
            out.push(EmittedFile {
                path: format!("{}.ts", module_path_for_nsid(&doc.nsid).join("/")),
                contents: render_lexicon_file(doc),
            });
        }

        for (path, contents) in render_directory_index_ts_files(docs) {
            out.push(EmittedFile { path, contents });
        }

        out.push(EmittedFile {
            path: "index.ts".to_owned(),
            contents: render_root_index_ts(docs),
        });
        out.push(EmittedFile {
            path: "examples.ts".to_owned(),
            contents: render_examples_ts(examples),
        });
        out.push(EmittedFile {
            path: "family.ts".to_owned(),
            contents: render_family_ts(docs, family),
        });

        Ok(out)
    }
}

/// Relative TS import specifier from one nsid's file to another's.
/// `dev.idiolect.encounter` → `dev.idiolect.defs` resolves to
/// `"./defs"`; `dev.idiolect.encounter` → `dev.panproto.schema.lens`
/// resolves to `"../panproto/schema/lens"`. Used for cross-module
/// `import type` lines emitted at the top of each lexicon file.
fn relative_ts_import(from_nsid: &str, to_nsid: &str) -> String {
    let from = module_path_for_nsid(from_nsid);
    let to = module_path_for_nsid(to_nsid);
    let from_dir = if from.is_empty() {
        &[][..]
    } else {
        &from[..from.len() - 1]
    };
    let to_dir = if to.is_empty() {
        &[][..]
    } else {
        &to[..to.len() - 1]
    };
    let to_leaf = to.last().cloned().unwrap_or_default();

    let mut common = 0usize;
    while common < from_dir.len() && common < to_dir.len() && from_dir[common] == to_dir[common] {
        common += 1;
    }
    let ups = from_dir.len() - common;
    let downs = &to_dir[common..];

    let mut spec = String::new();
    if ups == 0 {
        spec.push_str("./");
    } else {
        for _ in 0..ups {
            spec.push_str("../");
        }
    }
    for seg in downs {
        spec.push_str(seg);
        spec.push('/');
    }
    spec.push_str(&to_leaf);
    spec
}

// ---------- in-house ts ir ----------

#[derive(Debug, Clone)]
enum TsDecl {
    Interface {
        name: String,
        description: Option<String>,
        fields: Vec<TsField>,
    },
    StringLiteralUnion {
        name: String,
        description: Option<String>,
        values: Vec<String>,
    },
    TaggedUnion {
        name: String,
        description: Option<String>,
        /// `(tag_value, variant_ty_name, import_from)`. `import_from`
        /// is `Some("module")` when the variant type lives in another
        /// module (triggers a cross-module `import type` at the top of
        /// the file); `None` for same-module refs.
        variants: Vec<(String, String, Option<String>)>,
    },
}

#[derive(Debug, Clone)]
struct TsField {
    name: String,
    description: Option<String>,
    ty: TsType,
    optional: bool,
}

#[derive(Debug, Clone)]
enum TsType {
    /// `"string"`, `"number"`, `"boolean"`, `"unknown"`.
    Primitive(&'static str),
    /// Named type reference. `import_from` is `None` for same-module
    /// refs; `Some("module")` triggers a cross-module `import type` at
    /// the top of the file.
    Ref {
        name: String,
        import_from: Option<String>,
    },
    Array(Box<Self>),
}

// ---------- per-lexicon rendering ----------

fn render_lexicon_file(doc: &LexiconDoc) -> String {
    let decls = collect_decls_for_doc(doc);
    let imports = collect_imports(&decls);

    let allocator = Allocator::default();
    let ab = AstBuilder::new(&allocator);

    let mut out = String::new();
    out.push_str("// @generated by idiolect-codegen. do not edit.\n");
    let _ = writeln!(out, "// source: {}", doc.nsid);
    out.push('\n');

    if !imports.is_empty() {
        for (module, names) in &imports {
            let stmt = build_import_type_stmt(ab, module, names);
            out.push_str(&render_program(
                &allocator,
                ab,
                vec![stmt],
                String::new(),
                Vec::new(),
            ));
        }
        out.push('\n');
    }

    if let Some(desc) = &doc.description {
        let _ = writeln!(out, "// {desc}");
        out.push('\n');
    }

    for (i, decl) in decls.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&render_decl(&allocator, ab, decl));
    }

    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Build the ordered declaration list for a single lexicon doc.
///
/// The output order mirrors the legacy string emitter: main record
/// first, then sibling defs alphabetically, then non-main inlines,
/// then main-derived inlines. The prior emitter's ordering is load-
/// bearing for downstream snapshot stability.
fn collect_decls_for_doc(doc: &LexiconDoc) -> Vec<TsDecl> {
    let mut main_inlines: Vec<Inline> = Vec::new();
    let mut decls: Vec<TsDecl> = Vec::new();
    let mut non_main_inlines: Vec<Inline> = Vec::new();

    if let Some(Def::Record(record)) = doc.defs.get("main") {
        let ty_name = pascal_case(&module_name_for_nsid(&doc.nsid));
        let (decl, inlines) = build_interface(
            &ty_name,
            &doc.nsid,
            &record.body,
            record.description.as_deref(),
        );
        decls.push(decl);
        main_inlines = inlines;
    }

    for (def_name, def) in &doc.defs {
        if def_name == "main" {
            continue;
        }

        let ty_name = pascal_case(def_name);
        match def {
            Def::Record(_) => {}
            Def::Object(obj) => {
                let (decl, inlines) =
                    build_interface(&ty_name, &doc.nsid, obj, obj.description.as_deref());
                decls.push(decl);
                non_main_inlines.extend(inlines);
            }
            Def::StringEnum(enm) => {
                decls.push(build_string_union(&ty_name, enm));
            }
            Def::Union(uni) => {
                decls.push(build_tagged_union(&ty_name, &doc.nsid, uni));
            }
        }
    }

    for inline in non_main_inlines {
        decls.extend(render_inline(&inline, &doc.nsid));
    }
    for inline in main_inlines {
        decls.extend(render_inline(&inline, &doc.nsid));
    }

    decls
}

// ---------- oxc ast construction ----------

/// Render a single declaration as a one-statement program, attaching
/// its jsdoc (and any field-level jsdoc) through oxc's comment system.
fn render_decl(allocator: &Allocator, ab: AstBuilder<'_>, decl: &TsDecl) -> String {
    let mut source_text = String::new();
    let mut comments: Vec<Comment> = Vec::new();
    let mut next_id: u32 = 1;

    let stmt = match decl {
        TsDecl::Interface {
            name,
            description,
            fields,
        } => build_interface_stmt(
            ab,
            &mut source_text,
            &mut comments,
            &mut next_id,
            name,
            description.as_deref(),
            fields,
        ),
        TsDecl::StringLiteralUnion {
            name,
            description,
            values,
        } => build_string_literal_union_stmt(
            ab,
            &mut source_text,
            &mut comments,
            &mut next_id,
            name,
            description.as_deref(),
            values,
        ),
        TsDecl::TaggedUnion {
            name,
            description,
            variants,
        } => build_tagged_union_stmt(
            ab,
            &mut source_text,
            &mut comments,
            &mut next_id,
            name,
            description.as_deref(),
            variants,
        ),
    };

    render_program(allocator, ab, vec![stmt], source_text, comments)
}

/// Assemble a program from the given statements, `source_text`, and
/// comments; run it through `oxc_codegen` and return the printed
/// bytes.
fn render_program<'a>(
    allocator: &'a Allocator,
    ab: AstBuilder<'a>,
    body: Vec<Statement<'a>>,
    source_text: String,
    comments: Vec<Comment>,
) -> String {
    let src: &'a str = allocator.alloc_str(&source_text);
    let comments_vec: OxcVec<'a, Comment> = OxcVec::from_iter_in(comments, allocator);
    let directives: OxcVec<'a, oxc_ast::ast::Directive<'a>> = ab.vec();
    let body_vec: OxcVec<'a, Statement<'a>> = OxcVec::from_iter_in(body, allocator);

    let program = ab.program(
        SPAN,
        SourceType::ts(),
        src,
        comments_vec,
        None,
        directives,
        body_vec,
    );

    let opts = CodegenOptions {
        indent_char: IndentChar::Space,
        indent_width: 2,
        comments: CommentOptions::default(),
        ..Default::default()
    };

    Codegen::new().with_options(opts).build(&program).code
}

/// Push a `/** ... */` jsdoc block into `source_text` and a matching
/// `Comment` with the given attach id.
fn push_jsdoc(
    source_text: &mut String,
    comments: &mut Vec<Comment>,
    attach_id: u32,
    description: &str,
) {
    let mut body = String::from("/**\n");
    for line in description.lines() {
        body.push_str(" * ");
        body.push_str(line);
        body.push('\n');
    }
    body.push_str(" */");

    let start = source_text.len() as u32;
    source_text.push_str(&body);
    let end = source_text.len() as u32;
    // separator so the next jsdoc's span won't touch this one.
    source_text.push('\n');

    let mut comment = Comment::new(start, end, CommentKind::MultiLineBlock);
    comment.attached_to = attach_id;
    comment.position = CommentPosition::Leading;
    comment.newlines = CommentNewlines::Leading | CommentNewlines::Trailing;
    comment.content = CommentContent::Jsdoc;
    comments.push(comment);
}

fn fresh_id(next_id: &mut u32) -> u32 {
    let id = *next_id;
    *next_id = next_id.saturating_add(1);
    id
}

fn build_interface_stmt<'a>(
    ab: AstBuilder<'a>,
    source_text: &mut String,
    comments: &mut Vec<Comment>,
    next_id: &mut u32,
    name: &str,
    description: Option<&str>,
    fields: &[TsField],
) -> Statement<'a> {
    let iface_id = fresh_id(next_id);
    if let Some(desc) = description {
        push_jsdoc(source_text, comments, iface_id, desc);
    }

    let mut signatures: OxcVec<'a, TSSignature<'a>> = ab.vec_with_capacity(fields.len());
    for field in fields {
        let sig_id = fresh_id(next_id);
        if let Some(desc) = &field.description {
            push_jsdoc(source_text, comments, sig_id, desc);
        }

        let key: PropertyKey<'a> = ab.property_key_static_identifier(SPAN, ab.ident(&field.name));
        let ty = build_ts_type(ab, &field.ty);
        let anno = ab.alloc_ts_type_annotation(SPAN, ty);
        let sig = ab.ts_property_signature(
            Span::new(sig_id, sig_id),
            false,
            field.optional,
            false,
            key,
            Some(anno),
        );
        signatures.push(TSSignature::TSPropertySignature(ab.alloc(sig)));
    }

    let body = ab.alloc_ts_interface_body(SPAN, signatures);
    let id: BindingIdentifier<'a> = ab.binding_identifier(SPAN, ab.ident(name));
    let iface = ab.alloc_ts_interface_declaration(
        Span::new(iface_id, iface_id),
        id,
        NONE,
        ab.vec(),
        body,
        false,
    );

    let export = ab.alloc_export_named_declaration(
        Span::new(iface_id, iface_id),
        Some(Declaration::TSInterfaceDeclaration(iface)),
        ab.vec(),
        None,
        ImportOrExportKind::Value,
        NONE,
    );
    Statement::ExportNamedDeclaration(export)
}

fn build_string_literal_union_stmt<'a>(
    ab: AstBuilder<'a>,
    source_text: &mut String,
    comments: &mut Vec<Comment>,
    next_id: &mut u32,
    name: &str,
    description: Option<&str>,
    values: &[String],
) -> Statement<'a> {
    let alias_id = fresh_id(next_id);
    if let Some(desc) = description {
        push_jsdoc(source_text, comments, alias_id, desc);
    }

    let types = ab.vec_from_iter(
        values
            .iter()
            .map(|v| ab.ts_type_literal_type(SPAN, literal_string(ab, v))),
    );
    // single-element unions still print cleanly through the union
    // builder, so there's no need to special-case `types.len() == 1`.
    let union_ty = ab.ts_type_union_type(SPAN, types);

    let id: BindingIdentifier<'a> = ab.binding_identifier(SPAN, ab.ident(name));
    let alias = ab.alloc_ts_type_alias_declaration(
        Span::new(alias_id, alias_id),
        id,
        NONE,
        union_ty,
        false,
    );

    let export = ab.alloc_export_named_declaration(
        Span::new(alias_id, alias_id),
        Some(Declaration::TSTypeAliasDeclaration(alias)),
        ab.vec(),
        None,
        ImportOrExportKind::Value,
        NONE,
    );
    Statement::ExportNamedDeclaration(export)
}

fn build_tagged_union_stmt<'a>(
    ab: AstBuilder<'a>,
    source_text: &mut String,
    comments: &mut Vec<Comment>,
    next_id: &mut u32,
    name: &str,
    description: Option<&str>,
    variants: &[(String, String, Option<String>)],
) -> Statement<'a> {
    let alias_id = fresh_id(next_id);
    if let Some(desc) = description {
        push_jsdoc(source_text, comments, alias_id, desc);
    }

    // one variant: `{ $type: "tag" } & VariantTy`
    let types = ab.vec_from_iter(variants.iter().map(|(tag, variant_ty, _)| {
        // build `{ $type: "tag" }`
        let sig_key = ab.property_key_static_identifier(SPAN, ab.ident("$type"));
        let tag_ty = ab.ts_type_literal_type(SPAN, literal_string(ab, tag));
        let tag_anno = ab.alloc_ts_type_annotation(SPAN, tag_ty);
        let sig = ab.ts_property_signature(SPAN, false, false, false, sig_key, Some(tag_anno));
        let tag_obj = ab.ts_type_type_literal(
            SPAN,
            ab.vec1(TSSignature::TSPropertySignature(ab.alloc(sig))),
        );

        // ref to the variant type
        let variant_ref = ab.ts_type_type_reference(
            SPAN,
            ab.ts_type_name_identifier_reference(SPAN, ab.ident(variant_ty)),
            NONE,
        );

        ab.ts_type_intersection_type(SPAN, ab.vec_from_array([tag_obj, variant_ref]))
    }));

    let union_ty = ab.ts_type_union_type(SPAN, types);

    let id: BindingIdentifier<'a> = ab.binding_identifier(SPAN, ab.ident(name));
    let alias = ab.alloc_ts_type_alias_declaration(
        Span::new(alias_id, alias_id),
        id,
        NONE,
        union_ty,
        false,
    );

    let export = ab.alloc_export_named_declaration(
        Span::new(alias_id, alias_id),
        Some(Declaration::TSTypeAliasDeclaration(alias)),
        ab.vec(),
        None,
        ImportOrExportKind::Value,
        NONE,
    );
    Statement::ExportNamedDeclaration(export)
}

fn build_import_type_stmt<'a>(
    ab: AstBuilder<'a>,
    module: &str,
    names: &BTreeSet<String>,
) -> Statement<'a> {
    let specifiers = ab.vec_from_iter(names.iter().map(|n| {
        let imported: ModuleExportName<'a> =
            ab.module_export_name_identifier_name(SPAN, ab.ident(n));
        let local: BindingIdentifier<'a> = ab.binding_identifier(SPAN, ab.ident(n));
        ImportDeclarationSpecifier::ImportSpecifier(ab.alloc_import_specifier(
            SPAN,
            imported,
            local,
            ImportOrExportKind::Value,
        ))
    }));
    let source = ab.string_literal(SPAN, ab.str(&format!("./{module}")), None);
    let decl = ab.alloc_import_declaration(
        SPAN,
        Some(specifiers),
        source,
        None,
        NONE,
        ImportOrExportKind::Type,
    );
    Statement::ImportDeclaration(decl)
}

fn build_ts_type<'a>(ab: AstBuilder<'a>, ty: &TsType) -> TSType<'a> {
    match ty {
        TsType::Primitive("string") => ab.ts_type_string_keyword(SPAN),
        TsType::Primitive("number") => ab.ts_type_number_keyword(SPAN),
        TsType::Primitive("boolean") => ab.ts_type_boolean_keyword(SPAN),
        TsType::Primitive(_) => ab.ts_type_unknown_keyword(SPAN),
        TsType::Ref { name, .. } => ab.ts_type_type_reference(
            SPAN,
            ab.ts_type_name_identifier_reference(SPAN, ab.ident(name)),
            NONE,
        ),
        TsType::Array(inner) => {
            let inner_ty = build_ts_type(ab, inner);
            ab.ts_type_array_type(SPAN, inner_ty)
        }
    }
}

fn literal_string<'a>(ab: AstBuilder<'a>, value: &str) -> TSLiteral<'a> {
    TSLiteral::StringLiteral(ab.alloc_string_literal(SPAN, ab.str(value), None))
}

// ---------- builders (lexicon -> ir) ----------

fn build_interface(
    ty_name: &str,
    nsid: &str,
    def: &ObjectDef,
    description: Option<&str>,
) -> (TsDecl, Vec<Inline>) {
    let mut inlines: Vec<Inline> = Vec::new();
    let mut fields: Vec<TsField> = Vec::with_capacity(def.properties.len());

    let mut sorted: Vec<&(String, Prop)> = def.properties.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));

    for (prop_name, prop) in sorted {
        let required = def.required.iter().any(|r| r == prop_name);
        let (ty, inline) = resolve_prop_type(&prop.ty, ty_name, prop_name, nsid);
        if let Some(inline) = inline {
            inlines.push(inline);
        }

        fields.push(TsField {
            name: camel_case(prop_name),
            description: prop.description.clone(),
            ty,
            optional: !required,
        });
    }

    // categorical inline order: unions, enums, objects.
    inlines.sort_by_key(Inline::category_order);

    (
        TsDecl::Interface {
            name: ty_name.to_owned(),
            description: description.map(str::to_owned),
            fields,
        },
        inlines,
    )
}

fn build_string_union(ty_name: &str, def: &StringEnumDef) -> TsDecl {
    TsDecl::StringLiteralUnion {
        name: ty_name.to_owned(),
        description: def.description.clone(),
        values: def.values.clone(),
    }
}

fn build_tagged_union(ty_name: &str, current_nsid: &str, def: &UnionDef) -> TsDecl {
    let variants = def
        .variants
        .iter()
        .map(|v| {
            let tag = format!("{}#{}", v.nsid, v.def_name);
            let variant_ty = ts_ref_name(v, current_nsid);
            let import_from = if v.nsid == current_nsid {
                None
            } else {
                Some(relative_ts_import(current_nsid, &v.nsid))
            };
            (tag, variant_ty, import_from)
        })
        .collect();

    TsDecl::TaggedUnion {
        name: ty_name.to_owned(),
        description: def.description.clone(),
        variants,
    }
}

// ---------- inline type synthesis ----------

#[derive(Debug, Clone)]
enum Inline {
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

impl Inline {
    fn category_order(&self) -> u8 {
        match self {
            Self::Union { .. } => 0,
            Self::StringEnum { .. } => 1,
            Self::Object { .. } => 2,
        }
    }
}

fn render_inline(inline: &Inline, current_nsid: &str) -> Vec<TsDecl> {
    match inline {
        Inline::StringEnum {
            name,
            description,
            values,
        } => vec![build_string_union(
            name,
            &StringEnumDef {
                description: description.clone(),
                values: values.clone(),
            },
        )],
        Inline::Object {
            name,
            description,
            def,
        } => {
            let (decl, nested) = build_interface(name, current_nsid, def, description.as_deref());
            let mut out = vec![decl];
            for inline in nested {
                out.extend(render_inline(&inline, current_nsid));
            }
            out
        }
        Inline::Union {
            name,
            description,
            variants,
        } => vec![build_tagged_union(
            name,
            current_nsid,
            &UnionDef {
                description: description.clone(),
                variants: variants.clone(),
            },
        )],
    }
}

// ---------- prop type resolution ----------

fn resolve_prop_type(
    ty: &PropType,
    parent_ty: &str,
    prop_name: &str,
    current_nsid: &str,
) -> (TsType, Option<Inline>) {
    match ty {
        // TypeScript records keep every format-typed field as a
        // bare `string`. Branded-string types (`AtUri`, `Did`, …)
        // and the runtime validators that enforce them are tracked
        // for a follow-up; Rust's parse-time validation is the
        // load-bearing piece for this release.
        PropType::String
        | PropType::StringDatetime
        | PropType::StringAtUri
        | PropType::StringDid
        | PropType::StringNsid
        | PropType::StringUri
        | PropType::StringLanguage
        | PropType::CidLink => (TsType::Primitive("string"), None),
        PropType::Integer | PropType::Number => (TsType::Primitive("number"), None),
        PropType::Boolean => (TsType::Primitive("boolean"), None),
        PropType::Bytes | PropType::Blob | PropType::Unknown => {
            (TsType::Primitive("unknown"), None)
        }
        PropType::Ref(target) => (ts_ref_type(target, current_nsid), None),
        PropType::Array(inner) => {
            let (inner_ty, inline) = resolve_prop_type(inner, parent_ty, prop_name, current_nsid);
            (TsType::Array(Box::new(inner_ty)), inline)
        }
        PropType::InlineStringEnum(values) => {
            let name = format!("{parent_ty}{}", pascal_case(prop_name));
            let ty = TsType::Ref {
                name: name.clone(),
                import_from: None,
            };
            let inline = Inline::StringEnum {
                name,
                description: None,
                values: values.clone(),
            };
            (ty, Some(inline))
        }
        PropType::InlineUnion(variants) => {
            let name = format!("{parent_ty}{}", pascal_case(prop_name));
            let ty = TsType::Ref {
                name: name.clone(),
                import_from: None,
            };
            let inline = Inline::Union {
                name,
                description: None,
                variants: variants.clone(),
            };
            (ty, Some(inline))
        }
        PropType::InlineObject(obj) => {
            let name = format!("{parent_ty}{}", pascal_case(prop_name));
            let ty = TsType::Ref {
                name: name.clone(),
                import_from: None,
            };
            let inline = Inline::Object {
                name,
                description: obj.description.clone(),
                def: obj.clone(),
            };
            (ty, Some(inline))
        }
    }
}

fn ts_ref_type(target: &RefTarget, current_nsid: &str) -> TsType {
    if target.nsid == current_nsid {
        TsType::Ref {
            name: pascal_case(&target.def_name),
            import_from: None,
        }
    } else {
        TsType::Ref {
            name: pascal_case(&target.def_name),
            import_from: Some(relative_ts_import(current_nsid, &target.nsid)),
        }
    }
}

fn ts_ref_name(target: &RefTarget, _current_nsid: &str) -> String {
    // tagged-union variant names appear bare in the generated source;
    // the import at the top of the file resolves cross-module refs.
    pascal_case(&target.def_name)
}

// ---------- import collection ----------

fn collect_imports(decls: &[TsDecl]) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for decl in decls {
        match decl {
            TsDecl::Interface { fields, .. } => {
                for f in fields {
                    collect_import_from_ty(&f.ty, &mut out);
                }
            }
            TsDecl::TaggedUnion { variants, .. } => {
                // Variants of a cross-module tagged union need a top-
                // of-file `import type` pass, same as interface refs.
                for (_, variant_ty, import_from) in variants {
                    if let Some(module) = import_from {
                        out.entry(module.clone())
                            .or_default()
                            .insert(variant_ty.clone());
                    }
                }
            }
            TsDecl::StringLiteralUnion { .. } => {}
        }
    }
    out
}

fn collect_import_from_ty(ty: &TsType, out: &mut BTreeMap<String, BTreeSet<String>>) {
    match ty {
        TsType::Ref {
            name,
            import_from: Some(module),
        } => {
            out.entry(module.clone()).or_default().insert(name.clone());
        }
        TsType::Array(inner) => collect_import_from_ty(inner, out),
        _ => {}
    }
}

// ---------- aggregating files ----------

fn render_root_index_ts(docs: &[LexiconDoc]) -> String {
    use std::collections::BTreeSet;

    let mut out = String::new();
    out.push_str("// @generated by idiolect-codegen. do not edit.\n\n");
    out.push_str(
        "// TypeScript types generated from the `dev.idiolect.*` lexicons plus the vendored\n\
         // `dev.panproto.*` tree (see `lexicons/dev/panproto/VENDORED.md`).\n\
         //\n\
         // The on-disk layout mirrors the lexicon directory tree under\n\
         // `lexicons/`: a per-directory `index.ts` re-exports its\n\
         // immediate children. Top-level barrel below points at every\n\
         // first-segment directory plus the cross-cutting fixtures and\n\
         // record helpers.\n\n",
    );

    let mut roots: BTreeSet<String> = BTreeSet::new();
    for doc in docs {
        if let Some(first) = module_path_for_nsid(&doc.nsid).into_iter().next() {
            roots.insert(first);
        }
    }
    for r in &roots {
        let _ = writeln!(out, "export * from \"./{r}/index\";");
    }
    let _ = writeln!(out, "export * from \"./examples\";");
    let _ = writeln!(out, "export * from \"./family\";");
    out
}

/// Per-directory `index.ts` barrels. For every internal directory in
/// the lexicon tree, emit an `index.ts` that re-exports its
/// immediate children (sub-directories via their own `./<name>/index`
/// barrels, leaf modules via `./<name>`).
fn render_directory_index_ts_files(docs: &[LexiconDoc]) -> Vec<(String, String)> {
    use std::collections::{BTreeMap, BTreeSet};

    // dir → (sub-directories, leaf modules)
    let mut dirs: BTreeMap<Vec<String>, BTreeSet<String>> = BTreeMap::new();
    let mut leaves: BTreeMap<Vec<String>, BTreeSet<String>> = BTreeMap::new();

    for doc in docs {
        let path = module_path_for_nsid(&doc.nsid);
        if path.is_empty() {
            continue;
        }
        // Each prefix `path[..i]` is an internal dir whose immediate
        // child at index `i` is either another directory (i + 1 < len)
        // or the leaf module file (i + 1 == len).
        for i in 0..path.len() {
            let dir = path[..i].to_vec();
            let child = path[i].clone();
            if i + 1 == path.len() {
                leaves.entry(dir).or_default().insert(child);
            } else {
                dirs.entry(dir).or_default().insert(child);
            }
        }
    }

    // Skip the root: that's `index.ts` rendered separately.
    dirs.remove(&Vec::new());
    leaves.remove(&Vec::new());

    let mut all_dirs: BTreeSet<Vec<String>> = BTreeSet::new();
    all_dirs.extend(dirs.keys().cloned());
    all_dirs.extend(leaves.keys().cloned());

    let mut files = Vec::with_capacity(all_dirs.len());
    for dir in all_dirs {
        let mut body = String::new();
        body.push_str("// @generated by idiolect-codegen. do not edit.\n\n");
        if let Some(subs) = dirs.get(&dir) {
            for sub in subs {
                let _ = writeln!(body, "export * from \"./{sub}/index\";");
            }
        }
        if let Some(ls) = leaves.get(&dir) {
            for leaf in ls {
                let _ = writeln!(body, "export * from \"./{leaf}\";");
            }
        }
        files.push((format!("{}/index.ts", dir.join("/")), body));
    }
    files
}

// ---------- family module: IR + composable renderers ----------
//
// The family module's TS output is a sequence of small, named pieces
// (banner, imports, family identity, record-keyed NSID const, AnyRecord
// union, type guards, decode/encode helpers). Each piece is a focused
// renderer over a typed slice of the IR, so the data flow reads as
//
//   docs + family
//     -> [collect] -> FamilyTsModule
//     -> [render]  -> [banner, imports, identity, ...] joined as bytes
//
// — the same lens-then-render decomposition the per-lexicon path uses,
// just at the family-aggregate granularity. The IR is family-agnostic;
// no idiolect strings are baked in.

/// Family-level IR for the generated `family.ts` module.
///
/// Built from `(docs, family)` by [`collect_family_module`] and
/// rendered by [`render_family_module`]. Decouples the
/// disambiguation / collection logic from the TS-shape rendering
/// so each renderer is testable in isolation.
struct FamilyTsModule {
    /// Identity bits propagated from `FamilyConfig`.
    family_id: String,
    family_marker: String,
    family_prefix: String,
    /// One entry per family-member record, in canonical (sorted) order.
    members: Vec<FamilyTsMember>,
}

/// Per-record IR for the family module. The `kind` and `ty_name` are
/// already disambiguated when leaf type names collide across nested
/// directories (e.g. `pub.layers.changelog.entry` vs
/// `pub.layers.resource.entry`).
struct FamilyTsMember {
    /// Full NSID of the record (`dev.idiolect.encounter`).
    nsid: String,
    /// Camel-case key for the runtime `NSID` const map (`encounter`).
    /// When leaf names collide, a parent prefix is folded in:
    /// `changelogEntry` / `resourceEntry`.
    kind: String,
    /// `PascalCase` TS type name as it appears in this module
    /// (`Encounter`, `ChangelogEntry`).
    ty_name: String,
    /// `PascalCase` TS type name in its source module — equals
    /// `ty_name` when no aliasing was needed.
    raw_ty_name: String,
    /// Relative import path from `family.ts` to the per-record
    /// module, with no leading `./` or trailing `.ts`.
    import_path: String,
}

fn render_family_ts(docs: &[LexiconDoc], family: &super::family::FamilyConfig) -> String {
    let module = collect_family_module(docs, family);
    render_family_module(&module)
}

/// Collect family-member entries, performing leaf-name
/// disambiguation when two records under different directories
/// share a leaf `TypeName`.
fn collect_family_module(
    docs: &[LexiconDoc],
    family: &super::family::FamilyConfig,
) -> FamilyTsModule {
    let mut records: Vec<&LexiconDoc> = docs
        .iter()
        .filter(|d| matches!(d.defs.get("main"), Some(Def::Record(_))))
        .filter(|d| d.nsid.starts_with(family.nsid_prefix.as_ref()))
        .collect();
    records.sort_by(|a, b| a.nsid.cmp(&b.nsid));

    let prepared: Vec<(Vec<String>, String)> = records
        .iter()
        .map(|r| {
            let path = module_path_for_nsid(&r.nsid);
            let ty = pascal_case(&module_name_for_nsid(&r.nsid));
            (path, ty)
        })
        .collect();
    let aliases = compute_disambiguation_aliases(&prepared);

    let members = records
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let (path_segments, raw_ty) = &prepared[i];
            let ty_name = aliases[i].clone().unwrap_or_else(|| raw_ty.clone());
            let kind = if aliases[i].is_some() {
                camel_case(&ty_name)
            } else {
                module_name_for_nsid(&r.nsid)
            };
            FamilyTsMember {
                nsid: r.nsid.clone(),
                kind,
                raw_ty_name: raw_ty.clone(),
                ty_name,
                import_path: path_segments.join("/"),
            }
        })
        .collect();

    FamilyTsModule {
        family_id: family.id.as_ref().to_owned(),
        family_marker: family.marker_name.as_ref().to_owned(),
        family_prefix: family.nsid_prefix.as_ref().to_owned(),
        members,
    }
}

/// Walk-up disambiguation: when two records share a leaf `TypeName`,
/// fold parent directory segments into the alias until each member
/// of the colliding group is unique. Returns `None` for entries that
/// don't need an alias.
fn compute_disambiguation_aliases(prepared: &[(Vec<String>, String)]) -> Vec<Option<String>> {
    let mut by_ty: std::collections::BTreeMap<String, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (i, (_, ty)) in prepared.iter().enumerate() {
        by_ty.entry(ty.clone()).or_default().push(i);
    }
    let mut aliases: Vec<Option<String>> = vec![None; prepared.len()];
    for indices in by_ty.values() {
        if indices.len() == 1 {
            continue;
        }
        let mut take = 1;
        loop {
            let suffixes: Vec<String> = indices
                .iter()
                .map(|&i| {
                    let (path, _) = &prepared[i];
                    let leaf_idx = path.len().saturating_sub(1);
                    let start = leaf_idx.saturating_sub(take);
                    path[start..leaf_idx].join("/")
                })
                .collect();
            let unique: std::collections::BTreeSet<&String> = suffixes.iter().collect();
            if unique.len() == indices.len() {
                for &i in indices {
                    let (path, ty) = &prepared[i];
                    let leaf_idx = path.len().saturating_sub(1);
                    let start = leaf_idx.saturating_sub(take);
                    let prefix: String = path[start..leaf_idx]
                        .iter()
                        .map(|s| pascal_case(s))
                        .collect();
                    aliases[i] = Some(format!("{prefix}{ty}"));
                }
                break;
            }
            if indices.iter().all(|&i| {
                let (path, _) = &prepared[i];
                take >= path.len().saturating_sub(1)
            }) {
                for &i in indices {
                    let (path, ty) = &prepared[i];
                    let leaf_idx = path.len().saturating_sub(1);
                    let prefix: String = path[..leaf_idx].iter().map(|s| pascal_case(s)).collect();
                    aliases[i] = Some(format!("{prefix}{ty}"));
                }
                break;
            }
            take += 1;
        }
    }
    aliases
}

/// Render a [`FamilyTsModule`] by composing focused per-section
/// renderers. Order of pieces is load-bearing for snapshot stability:
/// imports first, then identity, then types, then runtime helpers.
fn render_family_module(m: &FamilyTsModule) -> String {
    let pieces: Vec<String> = vec![
        render_family_banner(m),
        render_family_imports(&m.members),
        render_family_identity(m),
        render_family_marker(m),
        render_family_nsid_const(&m.members),
        render_family_nsid_type(),
        render_family_record_types(&m.members),
        render_family_any_record(&m.members),
        render_family_is_kind(),
        render_family_per_type_guards(&m.members),
        render_family_tag_record(),
        render_family_record_nsids(&m.members),
        render_family_nsid_set(),
        render_family_contains(),
        render_family_decoded_record(),
        render_family_decode_record(),
        render_family_to_typed_json(),
    ];
    pieces.join("\n")
}

fn render_family_banner(m: &FamilyTsModule) -> String {
    format!(
        "// @generated by idiolect-codegen. do not edit.\n\
         \n\
         // Generated record family for `{id}`.\n\
         //\n\
         // Per-record types come from the sibling generated modules. This file\n\
         // emits the discriminated-union view, the dispatch helpers, and the\n\
         // family identity (`FAMILY_ID`, `FAMILY_NSID_PREFIX`, `FamilyMarker`)\n\
         // that mirror the Rust `family.rs` surface.\n",
        id = m.family_id,
    )
}

fn render_family_imports(members: &[FamilyTsMember]) -> String {
    let mut out = String::new();
    for m in members {
        if m.ty_name == m.raw_ty_name {
            let _ = writeln!(
                out,
                "import type {{ {raw} }} from \"./{path}\";",
                raw = m.raw_ty_name,
                path = m.import_path,
            );
        } else {
            let _ = writeln!(
                out,
                "import type {{ {raw} as {used} }} from \"./{path}\";",
                raw = m.raw_ty_name,
                used = m.ty_name,
                path = m.import_path,
            );
        }
    }
    out
}

fn render_family_identity(m: &FamilyTsModule) -> String {
    format!(
        "/** Family identifier, mirrored from the Rust `RecordFamily::ID` constant. */\n\
         export const FAMILY_ID = \"{id}\" as const;\n\
         \n\
         /** NSID prefix every member of this family shares. Informational. */\n\
         export const FAMILY_NSID_PREFIX = \"{prefix}\" as const;\n",
        id = m.family_id,
        prefix = m.family_prefix,
    )
}

fn render_family_marker(m: &FamilyTsModule) -> String {
    format!(
        "/** Nominal marker for the family, mirrored from the Rust `{marker}` struct. */\n\
         export type FamilyMarker = \"{marker}\";\n",
        marker = m.family_marker,
    )
}

fn render_family_nsid_const(members: &[FamilyTsMember]) -> String {
    let mut out = String::new();
    out.push_str("/**\n * Canonical NSIDs, keyed by record kind for ergonomic call sites.\n */\n");
    out.push_str("export const NSID = {\n");
    for m in members {
        let _ = writeln!(out, "  {kind}: \"{nsid}\",", kind = m.kind, nsid = m.nsid);
    }
    out.push_str("} as const;\n");
    out
}

fn render_family_nsid_type() -> String {
    "export type NSID = (typeof NSID)[keyof typeof NSID];\n".to_owned()
}

fn render_family_record_types(members: &[FamilyTsMember]) -> String {
    let mut out = String::new();
    out.push_str("/**\n * Mapping from record NSID to its TypeScript record type.\n */\n");
    out.push_str("export type RecordTypes = {\n");
    for m in members {
        let _ = writeln!(out, "  [NSID.{kind}]: {ty};", kind = m.kind, ty = m.ty_name);
    }
    out.push_str("};\n");
    out
}

fn render_family_any_record(members: &[FamilyTsMember]) -> String {
    let mut out = String::new();
    out.push_str("/**\n * Discriminated union tagged by `$nsid` for runtime dispatch.\n */\n");
    out.push_str("export type AnyRecord =\n");
    let last = members.len().saturating_sub(1);
    for (i, m) in members.iter().enumerate() {
        let suffix = if i == last { ";" } else { "" };
        let _ = writeln!(
            out,
            "  | {{ readonly $nsid: typeof NSID.{kind}; readonly value: {ty} }}{suffix}",
            kind = m.kind,
            ty = m.ty_name,
        );
    }
    out
}

fn render_family_is_kind() -> String {
    "/** True if `r` is an `AnyRecord` tagged with the given nsid. */\n\
     export function isKind<K extends NSID>(\n\
     \x20\x20r: AnyRecord,\n\
     \x20\x20nsid: K,\n\
     ): r is Extract<AnyRecord, { $nsid: K }> {\n\
     \x20\x20return r.$nsid === nsid;\n\
     }\n"
        .to_owned()
}

fn render_family_per_type_guards(members: &[FamilyTsMember]) -> String {
    let mut out = String::new();
    let last = members.len().saturating_sub(1);
    for (i, m) in members.iter().enumerate() {
        let _ = writeln!(
            out,
            "/** True if `r` wraps a `{ty}`. */\n\
             export function is{ty}(r: AnyRecord): r is {{ readonly $nsid: typeof NSID.{kind}; readonly value: {ty} }} {{\n  \
             return r.$nsid === NSID.{kind};\n}}",
            ty = m.ty_name,
            kind = m.kind,
        );
        if i != last {
            out.push('\n');
        }
    }
    out
}

fn render_family_tag_record() -> String {
    "/**\n\
     \x20* Wrap a strongly-typed record in its `AnyRecord` variant.\n\
     \x20*/\n\
     export function tagRecord<K extends NSID>(\n\
     \x20\x20nsid: K,\n\
     \x20\x20value: RecordTypes[K],\n\
     ): AnyRecord {\n\
     \x20\x20return { $nsid: nsid, value } as AnyRecord;\n\
     }\n"
        .to_owned()
}

fn render_family_record_nsids(members: &[FamilyTsMember]) -> String {
    let mut out = String::new();
    out.push_str("/** All record NSIDs in declaration order. */\n");
    out.push_str("export const RECORD_NSIDS = [\n");
    for m in members {
        let _ = writeln!(out, "  NSID.{kind},", kind = m.kind);
    }
    out.push_str("] as const satisfies readonly NSID[];\n");
    out
}

fn render_family_nsid_set() -> String {
    // Membership check uses a precomputed set against RECORD_NSIDS so
    // the type predicate (`nsid is NSID`) is honoured at runtime,
    // matching the Rust `RecordFamily::contains` semantics. The
    // FAMILY_NSID_PREFIX constant remains exposed for external
    // consumers doing prefix-based dispatch (e.g. `OrFamily`-style
    // composition), but it is informational, not the membership gate.
    "const FAMILY_NSID_SET: ReadonlySet<string> = new Set(RECORD_NSIDS);\n".to_owned()
}

fn render_family_contains() -> String {
    "/**\n\
     \x20* True if `nsid` is a member of this family — exact match against\n\
     \x20* the family's record set. Mirrors the Rust `RecordFamily::contains`\n\
     \x20* predicate; the type narrowing to `NSID` is sound because the\n\
     \x20* runtime check tests against the same literal set the type encodes.\n\
     \x20*/\n\
     export function familyContains(nsid: string): nsid is NSID {\n\
     \x20\x20return FAMILY_NSID_SET.has(nsid);\n\
     }\n"
        .to_owned()
}

fn render_family_decoded_record() -> String {
    "/**\n\
     \x20* Loose decoded view: family NSID and an unvalidated record body.\n\
     \x20* `decodeRecord` produces this; callers pair it with a per-record\n\
     \x20* validator (Zod, io-ts, hand-rolled) before treating the body as\n\
     \x20* any specific record type.\n\
     \x20*/\n\
     export interface DecodedRecord {\n\
     \x20\x20readonly $nsid: NSID;\n\
     \x20\x20readonly body: unknown;\n\
     }\n"
        .to_owned()
}

fn render_family_decode_record() -> String {
    // Use bracket access for `$type` so `noPropertyAccessFromIndexSignature`
    // (TS strict default) doesn't reject the dotted form on a
    // `Record<string, unknown>` source.
    "/**\n\
     \x20* Split an atproto wire-form record (an object whose `$type` field\n\
     \x20* carries the NSID of the contained record) into a (`$nsid`, body)\n\
     \x20* pair. Mirrors the Rust `AnyRecord::from_typed_json` constructor in\n\
     \x20* shape, but TypeScript has no runtime structural validator for the\n\
     \x20* generated record types, so the body comes back as `unknown` and\n\
     \x20* the caller is responsible for narrowing it.\n\
     \x20*\n\
     \x20* Returns `null` if `value` is not structurally a record object or\n\
     \x20* its `$type` is outside this family.\n\
     \x20*/\n\
     export function decodeRecord(value: unknown): DecodedRecord | null {\n\
     \x20\x20if (typeof value !== \"object\" || value === null || Array.isArray(value)) {\n\
     \x20\x20\x20\x20return null;\n\
     \x20\x20}\n\
     \x20\x20const obj = value as Record<string, unknown>;\n\
     \x20\x20const ty = obj[\"$type\"];\n\
     \x20\x20if (typeof ty !== \"string\" || !familyContains(ty)) return null;\n\
     \x20\x20const { $type: _stripped, ...body } = obj;\n\
     \x20\x20void _stripped;\n\
     \x20\x20return { $nsid: ty, body };\n\
     }\n"
        .to_owned()
}

fn render_family_to_typed_json() -> String {
    "/**\n\
     \x20* Encode an `AnyRecord` into atproto wire form: the inner `value`\n\
     \x20* spread with a `$type` discriminator. Mirrors the Rust\n\
     \x20* `AnyRecord::to_typed_json` method.\n\
     \x20*/\n\
     export function toTypedJson(r: AnyRecord): Record<string, unknown> {\n\
     \x20\x20return { ...r.value, $type: r.$nsid } as Record<string, unknown>;\n\
     }\n"
        .to_owned()
}

fn render_examples_ts(examples: &[Example]) -> String {
    let mut out = String::new();
    out.push_str("// @generated by idiolect-codegen. do not edit.\n\n");
    out.push_str(
        "// Minimally-valid fixture records, surfaced from `lexicons/dev/*/examples/`.\n\
         // Each `*Json` const is the raw json fixture string.\n\n",
    );

    let mut sorted: Vec<&Example> = examples.iter().collect();
    sorted.sort_by(|a, b| a.nsid.cmp(&b.nsid));

    for ex in &sorted {
        let kind = module_name_for_nsid(&ex.nsid);
        let const_name = format!("{}Json", pascal_case(&kind));
        let escaped = ex.json.replace('\\', "\\\\").replace('`', "\\`");
        let _ = writeln!(out, "/** Raw json for `{nsid}`. */", nsid = ex.nsid);
        let _ = writeln!(out, "export const {const_name}: string = `{escaped}`;");
        out.push('\n');
    }
    out
}

// ---------- helpers ----------

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

fn camel_case(s: &str) -> String {
    let mut it = s.chars();
    match it.next() {
        Some(first) => {
            let rest: String = it.collect();
            let mut out = String::with_capacity(s.len());
            out.extend(first.to_lowercase());
            out.push_str(&rest);
            out
        }
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_import_same_directory() {
        assert_eq!(
            relative_ts_import("dev.idiolect.encounter", "dev.idiolect.defs"),
            "./defs",
        );
    }

    #[test]
    fn relative_import_up_then_down() {
        assert_eq!(
            relative_ts_import("dev.idiolect.encounter", "dev.panproto.schema.lens"),
            "../panproto/schema/lens",
        );
    }

    #[test]
    fn relative_import_deeper_to_shallower() {
        assert_eq!(
            relative_ts_import("dev.panproto.schema.lens", "dev.idiolect.defs"),
            "../../idiolect/defs",
        );
    }

    #[test]
    fn relative_import_within_nested_dir() {
        assert_eq!(
            relative_ts_import("dev.panproto.schema.lens", "dev.panproto.schema.complement"),
            "./complement",
        );
    }

    #[test]
    fn relative_import_across_sibling_subdirs() {
        assert_eq!(
            relative_ts_import("dev.panproto.schema.lens", "dev.panproto.vcs.commit"),
            "../vcs/commit",
        );
    }

    // ---------- family module IR tests ----------
    //
    // Each renderer is pure over a slice of the IR. Test by
    // constructing a synthetic FamilyTsModule and asserting on the
    // emitted bytes. Tests cover the load-bearing pieces; the
    // layers-pub integration test (`tests/layers_family.rs`) covers
    // the full pipeline against real lexicons.

    fn member(nsid: &str, kind: &str, ty: &str, import_path: &str) -> FamilyTsMember {
        FamilyTsMember {
            nsid: nsid.to_owned(),
            kind: kind.to_owned(),
            ty_name: ty.to_owned(),
            raw_ty_name: ty.to_owned(),
            import_path: import_path.to_owned(),
        }
    }

    fn aliased_member(
        nsid: &str,
        kind: &str,
        used: &str,
        raw: &str,
        import_path: &str,
    ) -> FamilyTsMember {
        FamilyTsMember {
            nsid: nsid.to_owned(),
            kind: kind.to_owned(),
            ty_name: used.to_owned(),
            raw_ty_name: raw.to_owned(),
            import_path: import_path.to_owned(),
        }
    }

    #[test]
    fn family_imports_no_aliasing() {
        let m = member("dev.x.foo", "foo", "Foo", "dev/x/foo");
        let out = render_family_imports(&[m]);
        assert_eq!(out, "import type { Foo } from \"./dev/x/foo\";\n");
    }

    #[test]
    fn family_imports_with_aliasing() {
        let m = aliased_member(
            "dev.x.changelog.entry",
            "changelogEntry",
            "ChangelogEntry",
            "Entry",
            "dev/x/changelog/entry",
        );
        let out = render_family_imports(&[m]);
        assert_eq!(
            out,
            "import type { Entry as ChangelogEntry } from \"./dev/x/changelog/entry\";\n"
        );
    }

    #[test]
    fn family_identity_emits_id_and_prefix_constants() {
        let m = FamilyTsModule {
            family_id: "pub.layers".to_owned(),
            family_marker: "LayersFamily".to_owned(),
            family_prefix: "pub.layers.".to_owned(),
            members: Vec::new(),
        };
        let out = render_family_identity(&m);
        assert!(out.contains("FAMILY_ID = \"pub.layers\" as const"));
        assert!(out.contains("FAMILY_NSID_PREFIX = \"pub.layers.\" as const"));
    }

    #[test]
    fn family_marker_emits_string_literal_type() {
        let m = FamilyTsModule {
            family_id: "x".to_owned(),
            family_marker: "XFamily".to_owned(),
            family_prefix: "x.".to_owned(),
            members: Vec::new(),
        };
        assert!(render_family_marker(&m).contains("export type FamilyMarker = \"XFamily\";"));
    }

    #[test]
    fn family_nsid_const_lists_every_member_kind() {
        let members = vec![
            member("dev.x.foo", "foo", "Foo", "dev/x/foo"),
            member("dev.x.bar", "bar", "Bar", "dev/x/bar"),
        ];
        let out = render_family_nsid_const(&members);
        assert!(out.contains("foo: \"dev.x.foo\""));
        assert!(out.contains("bar: \"dev.x.bar\""));
        assert!(out.ends_with("} as const;\n"));
    }

    #[test]
    fn family_record_nsids_uses_kind_keys() {
        let members = vec![
            member("dev.x.foo", "foo", "Foo", "dev/x/foo"),
            aliased_member(
                "dev.x.a.entry",
                "aEntry",
                "AEntry",
                "Entry",
                "dev/x/a/entry",
            ),
        ];
        let out = render_family_record_nsids(&members);
        assert!(out.contains("NSID.foo,"));
        assert!(out.contains("NSID.aEntry,"));
        assert!(out.contains("] as const satisfies readonly NSID[];"));
    }

    #[test]
    fn family_any_record_terminates_last_variant_with_semicolon() {
        let members = vec![
            member("dev.x.foo", "foo", "Foo", "dev/x/foo"),
            member("dev.x.bar", "bar", "Bar", "dev/x/bar"),
        ];
        let out = render_family_any_record(&members);
        // Non-final variant: no trailing semicolon between variants.
        assert!(out.contains("readonly value: Foo }\n"));
        // Final variant: terminated with semicolon.
        assert!(out.contains("readonly value: Bar };\n"));
    }

    #[test]
    fn family_per_type_guards_one_per_member() {
        let members = vec![
            member("dev.x.foo", "foo", "Foo", "dev/x/foo"),
            member("dev.x.bar", "bar", "Bar", "dev/x/bar"),
        ];
        let out = render_family_per_type_guards(&members);
        assert!(out.contains("export function isFoo(r: AnyRecord)"));
        assert!(out.contains("export function isBar(r: AnyRecord)"));
        assert!(out.contains("return r.$nsid === NSID.foo;"));
        assert!(out.contains("return r.$nsid === NSID.bar;"));
    }

    #[test]
    fn family_decode_record_uses_bracket_dollar_type() {
        let out = render_family_decode_record();
        // Must use bracket access for `$type` so TS strict
        // `noPropertyAccessFromIndexSignature` doesn't reject it.
        assert!(out.contains("obj[\"$type\"]"));
        assert!(!out.contains("obj.$type"));
    }

    #[test]
    fn family_contains_uses_set_lookup_not_prefix() {
        let out = render_family_contains();
        // Soundness gate: the predicate must test exact membership
        // against the family's record set, not a prefix match.
        assert!(out.contains("FAMILY_NSID_SET.has(nsid)"));
        assert!(!out.contains("startsWith"));
    }

    // ---------- compute_disambiguation_aliases ----------

    fn path(segments: &[&str]) -> Vec<String> {
        segments.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn aliases_no_collision_returns_all_none() {
        let prepared = vec![
            (path(&["dev", "x", "foo"]), "Foo".to_owned()),
            (path(&["dev", "x", "bar"]), "Bar".to_owned()),
        ];
        let aliases = compute_disambiguation_aliases(&prepared);
        assert_eq!(aliases, vec![None, None]);
    }

    #[test]
    fn aliases_two_way_collision_uses_one_segment() {
        let prepared = vec![
            (
                path(&["dev", "x", "changelog", "entry"]),
                "Entry".to_owned(),
            ),
            (
                path(&["dev", "x", "resource", "entry"]),
                "Entry".to_owned(),
            ),
        ];
        let aliases = compute_disambiguation_aliases(&prepared);
        assert_eq!(
            aliases,
            vec![
                Some("ChangelogEntry".to_owned()),
                Some("ResourceEntry".to_owned()),
            ]
        );
    }

    #[test]
    fn aliases_three_way_collision_resolves_minimally() {
        // Three records named `Entry` under three different parents;
        // one parent segment should suffice.
        let prepared = vec![
            (path(&["a", "alpha", "entry"]), "Entry".to_owned()),
            (path(&["a", "beta", "entry"]), "Entry".to_owned()),
            (path(&["a", "gamma", "entry"]), "Entry".to_owned()),
        ];
        let aliases = compute_disambiguation_aliases(&prepared);
        assert_eq!(
            aliases,
            vec![
                Some("AlphaEntry".to_owned()),
                Some("BetaEntry".to_owned()),
                Some("GammaEntry".to_owned()),
            ]
        );
    }

    #[test]
    fn aliases_collision_fallback_walks_to_root() {
        // Two records share the same parent chain except for the
        // root; expect the alias to walk all the way up.
        let prepared = vec![
            (path(&["alpha", "shared", "entry"]), "Entry".to_owned()),
            (path(&["beta", "shared", "entry"]), "Entry".to_owned()),
        ];
        let aliases = compute_disambiguation_aliases(&prepared);
        // One segment up is "shared" for both — not unique. Two
        // segments up gives "alpha/shared" vs "beta/shared" — unique.
        assert_eq!(
            aliases,
            vec![
                Some("AlphaSharedEntry".to_owned()),
                Some("BetaSharedEntry".to_owned()),
            ]
        );
    }

    // ---------- collect_family_module ----------

    fn synthetic_record(nsid: &str) -> LexiconDoc {
        let mut defs = std::collections::BTreeMap::new();
        defs.insert(
            "main".to_owned(),
            Def::Record(crate::lexicon::RecordDef {
                description: None,
                key: Some("tid".to_owned()),
                body: crate::lexicon::ObjectDef {
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
    fn collect_filters_to_family_prefix_and_records_only() {
        let docs = vec![
            synthetic_record("dev.x.in_family"),
            synthetic_record("dev.x.also_in"),
            synthetic_record("dev.other.out_of_family"),
        ];
        let family = super::super::family::FamilyConfig::new("XFamily", "dev.x", "dev.x.");
        let module = collect_family_module(&docs, &family);
        let names: Vec<&str> = module.members.iter().map(|m| m.nsid.as_str()).collect();
        assert_eq!(names, vec!["dev.x.also_in", "dev.x.in_family"]);
    }
}
