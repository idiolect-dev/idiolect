//! Orchestrator query spec → Rust codegen.
//!
//! Reads `orchestrator-spec/queries.json`, validates its shape, and
//! emits three files under `crates/idiolect-orchestrator/src/generated/`:
//!
//! - `queries.rs` — one `pub fn` per spec entry, each calling the
//!   matching predicate in `crate::predicates`.
//! - `http.rs` — one axum handler per spec entry, plus a
//!   `register_routes` helper that wires them into a router.
//! - `mod.rs` — `pub mod queries; pub mod http;`.
//!
//! Every emitted file carries a header comment that directs changes
//! to the spec rather than to the generated source.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use serde::Deserialize;
use syn::{Type, parse_str};

/// Parsed shape of `orchestrator-spec/queries.json`.
#[derive(Debug, Deserialize)]
pub struct QuerySpec {
    /// Optional doc string describing the spec itself — surfaced as a
    /// header comment in the generated files.
    #[serde(default)]
    pub description: Option<String>,
    /// The queries to generate.
    pub queries: Vec<QueryDecl>,
}

#[derive(Debug, Deserialize)]
pub struct QueryDecl {
    pub name: String,
    pub entity: EntityKind,
    /// Rust predicate fn name in `crate::predicates`. Exactly one of
    /// `predicate` or `expression` is required.
    #[serde(default)]
    pub predicate: Option<String>,
    /// Panproto-expr source evaluated against the record. Exactly
    /// one of `predicate` or `expression` is required. The record is
    /// bound to `r` in the evaluation env.
    #[serde(default)]
    pub expression: Option<String>,
    #[serde(default)]
    pub params: Vec<ParamDecl>,
    pub http: HttpDecl,
    /// Prose description — stamped onto both the fn doc comment and
    /// the HTTP handler doc.
    #[serde(default)]
    pub description: Option<String>,
    /// CLI surface. Not read by this codegen module (consumed by the
    /// cli codegen); present here so one spec file drives multiple
    /// generators.
    #[serde(default)]
    #[allow(dead_code)]
    pub cli: Option<CliDecl>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Adapter,
    Belief,
    Bounty,
    Community,
    Dialect,
    Recommendation,
    Verification,
    Vocab,
}

impl EntityKind {
    /// Rust path to the generated record type.
    const fn rust_record_type(self) -> &'static str {
        match self {
            Self::Adapter => "idiolect_records::Adapter",
            Self::Belief => "idiolect_records::Belief",
            Self::Bounty => "idiolect_records::Bounty",
            Self::Community => "idiolect_records::Community",
            Self::Dialect => "idiolect_records::Dialect",
            Self::Recommendation => "idiolect_records::Recommendation",
            Self::Verification => "idiolect_records::Verification",
            Self::Vocab => "idiolect_records::Vocab",
        }
    }

    /// Method on `Catalog` that returns an iterator over the entity's
    /// entries.
    const fn catalog_accessor(self) -> &'static str {
        match self {
            Self::Adapter => "adapters",
            Self::Belief => "beliefs",
            Self::Bounty => "bounties",
            Self::Community => "communities",
            Self::Dialect => "dialects",
            Self::Recommendation => "recommendations",
            Self::Verification => "verifications",
            Self::Vocab => "vocabularies",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ParamDecl {
    pub name: String,
    pub rust_kind: RustKind,
    pub http_query: String,
    pub parser: ParserKind,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum RustKind {
    String,
    SchemaRef,
    LensRef,
    VerificationKind,
    AdapterInvocationProtocolKind,
}

impl RustKind {
    /// Owned Rust type at the HTTP-parameter boundary.
    const fn owned_type(self) -> &'static str {
        match self {
            Self::String => "String",
            Self::SchemaRef => "idiolect_records::generated::defs::SchemaRef",
            Self::LensRef => "idiolect_records::generated::defs::LensRef",
            Self::VerificationKind => "idiolect_records::generated::verification::VerificationKind",
            Self::AdapterInvocationProtocolKind => {
                "idiolect_records::generated::adapter::AdapterInvocationProtocolKind"
            }
        }
    }

    /// Rust type the query fn accepts for this param (always a
    /// reference, so the generated fn can borrow instead of owning).
    fn borrowed_type(self) -> String {
        match self {
            Self::String => "&str".to_owned(),
            other => format!("&{}", other.owned_type()),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParserKind {
    /// Passthrough — serde deserializes a String directly.
    String,
    /// Wrap a URI string into a `SchemaRef { uri, cid: None, language: None }`.
    SchemaRefFromUri,
    /// Wrap a URI string into a `LensRef { uri, cid: None, direction: None }`.
    LensRefFromUri,
    /// Parse a string into `VerificationKind` via
    /// `predicates::parse_verification_kind`.
    VerificationKind,
    /// Parse a string into `AdapterInvocationProtocolKind` via
    /// `predicates::parse_adapter_invocation_protocol_kind`.
    AdapterInvocationProtocolKind,
    /// Validate a string against the declared `VocabWorld` tokens
    /// (`closed-with-default`, `open`, `hierarchy-closed`) via
    /// `predicates::parse_vocab_world`. Produces a `String` so the
    /// predicate can compare against the rendered form.
    VocabWorld,
}

#[derive(Debug, Deserialize)]
pub struct HttpDecl {
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CliDecl {
    pub subcommand: Vec<String>,
    #[serde(default)]
    pub flag: Option<String>,
    #[serde(default)]
    pub default: bool,
}

// -----------------------------------------------------------------
// Load + validate
// -----------------------------------------------------------------

/// Load and validate the spec at `spec_path` against the lexicon at
/// `lexicon_path`.
///
/// # Errors
///
/// - IO/JSON parse error on either file.
/// - Structural validation error if the spec is not a valid instance
///   of the lexicon.
/// - [`anyhow::Error`] if any declaration fails crate-specific
///   invariants (duplicate name, parser kind inconsistent with rust
///   kind, predicate and expression both set, neither set).
pub fn load_spec(lexicon_path: &Path, spec_path: &Path) -> Result<QuerySpec> {
    let (_schema, spec_json) = super::validate_spec_through_panproto(lexicon_path, spec_path)?;
    let spec: QuerySpec = serde_json::from_value(spec_json)
        .with_context(|| format!("deserialize {}", spec_path.display()))?;
    validate_spec(&spec)?;
    Ok(spec)
}

fn validate_spec(spec: &QuerySpec) -> Result<()> {
    // Duplicate names would collide at the Rust fn level.
    let mut seen = std::collections::HashSet::new();
    for q in &spec.queries {
        if !seen.insert(&q.name) {
            bail!("duplicate query name: {}", q.name);
        }
        // Exactly one of `predicate` or `expression` must be set.
        match (&q.predicate, &q.expression) {
            (Some(_), None) | (None, Some(_)) => {}
            (Some(_), Some(_)) => bail!(
                "query {} has both `predicate` and `expression`; choose one",
                q.name
            ),
            (None, None) => bail!("query {} has neither `predicate` nor `expression`", q.name),
        }
        // Expression form does not take params — the expression
        // references the record directly. A future extension could
        // bind params as additional env entries, but this keeps the
        // two forms' surfaces decoupled.
        if q.expression.is_some() && !q.params.is_empty() {
            bail!(
                "query {} uses `expression` but also declares params; expression-form queries take no params in this version",
                q.name
            );
        }
        // Parser must match rust kind.
        for p in &q.params {
            let consistent = matches!(
                (p.rust_kind, p.parser),
                (
                    RustKind::String,
                    ParserKind::String | ParserKind::VocabWorld
                ) | (RustKind::SchemaRef, ParserKind::SchemaRefFromUri)
                    | (RustKind::LensRef, ParserKind::LensRefFromUri)
                    | (RustKind::VerificationKind, ParserKind::VerificationKind)
                    | (
                        RustKind::AdapterInvocationProtocolKind,
                        ParserKind::AdapterInvocationProtocolKind,
                    )
            );
            if !consistent {
                bail!(
                    "query {}: param {} parser {:?} does not match rust_kind {:?}",
                    q.name,
                    p.name,
                    p.parser,
                    p.rust_kind,
                );
            }
        }
    }
    Ok(())
}

// -----------------------------------------------------------------
// Emit
// -----------------------------------------------------------------

/// Emit all generated files into `orchestrator_src/generated/`.
///
/// `orchestrator_src` is the `crates/idiolect-orchestrator/src`
/// directory — passed in so tests can target a temp dir.
pub fn emit_all(spec: &QuerySpec, orchestrator_src: &Path) -> Result<Vec<PathBuf>> {
    let out_dir = orchestrator_src.join("generated");
    std::fs::create_dir_all(&out_dir).with_context(|| format!("mkdir {}", out_dir.display()))?;

    let mut written = Vec::new();

    let queries_path = out_dir.join("queries.rs");
    std::fs::write(&queries_path, emit_queries_rs(spec)?)?;
    written.push(queries_path);

    let http_path = out_dir.join("http.rs");
    std::fs::write(&http_path, emit_http_rs(spec)?)?;
    written.push(http_path);

    let mod_path = out_dir.join("mod.rs");
    std::fs::write(&mod_path, emit_mod_rs()?)?;
    written.push(mod_path);

    Ok(written)
}

const SOURCE_PATH: &str = "orchestrator-spec/queries.json";

fn render_file(inner_doc: &str, items: Vec<TokenStream>) -> Result<String> {
    super::render_file_with_source(inner_doc, SOURCE_PATH, items)
}

fn emit_mod_rs() -> Result<String> {
    // `http` is gated on the same feature the hand-written http
    // module is gated on. `queries` is always compiled because its
    // only deps (serde structs + panproto-expr via expr_eval) are
    // on the default path.
    let items = vec![quote! {
        pub mod queries;

        #[cfg(feature = "query-http")]
        pub mod http;
    }];
    render_file(
        "Generated orchestrator surface. See orchestrator-spec/queries.json for the source of truth.",
        items,
    )
}

fn emit_queries_rs(spec: &QuerySpec) -> Result<String> {
    let mut items: Vec<TokenStream> = vec![quote! {
        #![allow(unused_imports)]
        use crate::catalog::{Catalog, Entry};
    }];
    for q in &spec.queries {
        items.push(query_fn_item(q)?);
    }
    let inner_doc = spec
        .description
        .as_deref()
        .unwrap_or("Generated catalog queries. One `pub fn` per spec entry.");
    render_file(inner_doc, items)
}

/// Build the `pub fn <name>(…) -> Vec<&Entry<R>> { … }` item for one
/// query declaration.
fn query_fn_item(q: &QueryDecl) -> Result<TokenStream> {
    let fn_name = format_ident!("{}", q.name);
    let accessor = format_ident!("{}", q.entity.catalog_accessor());
    let record_ty: Type = parse_str(q.entity.rust_record_type())
        .with_context(|| format!("parse record type for {}", q.name))?;

    // A lifetime on the signature is needed when any borrowed
    // param is present — every current spec shape does take at
    // least one borrowed input, but the generator supports the
    // zero-param case too.
    let has_borrowed = !q.params.is_empty();

    // Build the `name: &Type` sequence.
    let mut param_tokens: Vec<TokenStream> = Vec::with_capacity(q.params.len());
    for p in &q.params {
        let name = format_ident!("{}", p.name);
        let borrowed: Type = parse_str(&p.rust_kind.borrowed_type())
            .with_context(|| format!("parse borrowed type for param {}", p.name))?;
        param_tokens.push(quote! { #name: #borrowed });
    }

    // Filter body.
    let filter_body = if let Some(pred) = &q.predicate {
        let pred_ident = format_ident!("{}", pred);
        let pred_args: Vec<TokenStream> = std::iter::once(quote! { &entry.record })
            .chain(q.params.iter().map(|p| {
                let n = format_ident!("{}", p.name);
                quote! { #n }
            }))
            .collect();
        quote! { crate::predicates::#pred_ident(#(#pred_args),*) }
    } else if let Some(expr) = &q.expression {
        quote! { crate::expr_eval::eval_bool_against_record(#expr, &entry.record) }
    } else {
        // Validated upstream.
        bail!("query {} missing predicate/expression", q.name);
    };

    let doc = q
        .description
        .as_deref()
        .map(|d| quote! { #[doc = #d] })
        .unwrap_or_default();

    let item = if has_borrowed {
        quote! {
            #doc
            #[must_use]
            pub fn #fn_name<'a>(
                catalog: &'a Catalog,
                #(#param_tokens),*
            ) -> Vec<&'a Entry<#record_ty>> {
                catalog
                    .#accessor()
                    .filter(|entry| #filter_body)
                    .collect()
            }
        }
    } else {
        quote! {
            #doc
            #[must_use]
            pub fn #fn_name(catalog: &Catalog) -> Vec<&Entry<#record_ty>> {
                catalog
                    .#accessor()
                    .filter(|entry| #filter_body)
                    .collect()
            }
        }
    };
    Ok(item)
}

fn emit_http_rs(spec: &QuerySpec) -> Result<String> {
    let mut items: Vec<TokenStream> = vec![quote! {
        #![allow(unused_imports)]
        use axum::Router;
        use axum::extract::{Query, State};
        use axum::routing::get;
        use serde::Deserialize;
        use crate::http::{ApiError, AppState, EnvelopedEntry, Page, Paged};
        use crate::generated::queries as q;
    }];

    for q in &spec.queries {
        if let Some(params_item) = http_params_struct(q) {
            items.push(params_item);
        }
        items.push(http_handler_fn(q)?);
    }

    items.push(register_routes_fn(spec));

    render_file(
        "Generated axum handlers for each catalog query. One handler per spec entry; a register_routes helper wires them into a router.",
        items,
    )
}

/// If the query declares params, emit its `struct <Pascal>Params`
/// with serde-renamed raw string fields plus a flattened `Page`.
fn http_params_struct(q: &QueryDecl) -> Option<TokenStream> {
    if q.params.is_empty() {
        return None;
    }
    let struct_name = format_ident!("{}Params", to_pascal(&q.name));
    let fields: Vec<TokenStream> = q
        .params
        .iter()
        .map(|p| {
            let field_ident = format_ident!("{}", raw_name_for(&p.name));
            let http_query = &p.http_query;
            quote! {
                #[serde(rename = #http_query)]
                #field_ident: String
            }
        })
        .collect();
    Some(quote! {
        #[derive(Debug, Deserialize)]
        struct #struct_name {
            #(#fields,)*
            #[serde(flatten)]
            page: Page,
        }
    })
}

/// Emit the `async fn handler_<name>(...) -> Result<...>` for one
/// query declaration.
fn http_handler_fn(q: &QueryDecl) -> Result<TokenStream> {
    let handler_ident = format_ident!("handler_{}", q.name);
    let query_ident = format_ident!("{}", q.name);
    let record_ty: Type = parse_str(q.entity.rust_record_type())
        .with_context(|| format!("parse record type for {}", q.name))?;
    let doc = q
        .description
        .as_deref()
        .map(|d| quote! { #[doc = #d] })
        .unwrap_or_default();

    if q.params.is_empty() {
        return Ok(quote! {
            #doc
            async fn #handler_ident(
                State(s): State<AppState>,
                Query(page): Query<Page>,
            ) -> Result<axum::Json<Paged<EnvelopedEntry<#record_ty>>>, ApiError> {
                let catalog = s.catalog.lock()?;
                let items: Vec<_> = q::#query_ident(&catalog)
                    .into_iter()
                    .map(EnvelopedEntry::from)
                    .collect();
                Ok(axum::Json(Paged::from_collected(items, &page)?))
            }
        });
    }

    let params_struct = format_ident!("{}Params", to_pascal(&q.name));

    // Build the param-extraction prelude: one `let <name>: <Ty> = …;`
    // per param, fallible parsers go through a `match` that returns
    // a 400 on error.
    let param_prelude: Vec<TokenStream> = q
        .params
        .iter()
        .map(param_binding)
        .collect::<Result<Vec<_>>>()?;

    let query_args: Vec<TokenStream> = q
        .params
        .iter()
        .map(|p| {
            let ident = format_ident!("{}", p.name);
            quote! { &#ident }
        })
        .collect();

    Ok(quote! {
        #doc
        async fn #handler_ident(
            State(s): State<AppState>,
            Query(p): Query<#params_struct>,
        ) -> Result<axum::Json<Paged<EnvelopedEntry<#record_ty>>>, ApiError> {
            #(#param_prelude)*
            let catalog = s.catalog.lock()?;
            let items: Vec<_> = q::#query_ident(&catalog, #(#query_args),*)
                .into_iter()
                .map(EnvelopedEntry::from)
                .collect();
            Ok(axum::Json(Paged::from_collected(items, &p.page)?))
        }
    })
}

/// Emit a single `let <typed>: <Ty> = ...;` binding converting the
/// raw string field on `p` into the typed param the query fn expects.
fn param_binding(p: &ParamDecl) -> Result<TokenStream> {
    let typed = format_ident!("{}", p.name);
    let raw = format_ident!("{}", raw_name_for(&p.name));
    let owned_ty: Type = parse_str(p.rust_kind.owned_type())
        .with_context(|| format!("parse owned type for param {}", p.name))?;

    match p.parser {
        ParserKind::String => Ok(quote! {
            let #typed: #owned_ty = p.#raw.clone();
        }),
        ParserKind::SchemaRefFromUri => Ok(quote! {
            let #typed: #owned_ty = crate::predicates::schema_ref_from_uri(p.#raw.clone());
        }),
        ParserKind::LensRefFromUri => Ok(quote! {
            let #typed: #owned_ty = crate::predicates::lens_ref_from_uri(p.#raw.clone());
        }),
        ParserKind::VerificationKind => Ok(quote! {
            let #typed: #owned_ty = match crate::predicates::parse_verification_kind(&p.#raw) {
                Ok(v) => v,
                Err(e) => return Err(ApiError::invalid_request(e)),
            };
        }),
        ParserKind::AdapterInvocationProtocolKind => Ok(quote! {
            let #typed: #owned_ty = match crate::predicates::parse_adapter_invocation_protocol_kind(&p.#raw) {
                Ok(v) => v,
                Err(e) => return Err(ApiError::invalid_request(e)),
            };
        }),
        ParserKind::VocabWorld => Ok(quote! {
            let #typed: #owned_ty = match crate::predicates::parse_vocab_world(&p.#raw) {
                Ok(v) => v,
                Err(e) => return Err(ApiError::invalid_request(e)),
            };
        }),
    }
}

fn register_routes_fn(spec: &QuerySpec) -> TokenStream {
    let routes: Vec<TokenStream> = spec
        .queries
        .iter()
        .map(|q| {
            let path = &q.http.path;
            let handler = format_ident!("handler_{}", q.name);
            quote! { .route(#path, get(#handler)) }
        })
        .collect();
    quote! {
        /// Register every generated route onto the caller-supplied router.
        #[must_use]
        pub fn register_routes(router: Router<AppState>) -> Router<AppState> {
            router #(#routes)*
        }
    }
}

fn to_pascal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper_next = true;
    for ch in s.chars() {
        if ch == '_' {
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

fn raw_name_for(name: &str) -> String {
    format!("raw_{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_conversion() {
        assert_eq!(to_pascal("open_bounties"), "OpenBounties");
        assert_eq!(to_pascal("bounties_by_requester"), "BountiesByRequester");
        assert_eq!(to_pascal("x"), "X");
    }

    #[test]
    fn parser_rust_kind_consistency_enforced() {
        let spec = QuerySpec {
            description: None,
            queries: vec![QueryDecl {
                name: "bad".into(),
                entity: EntityKind::Bounty,
                predicate: Some("nope".into()),
                expression: None,
                params: vec![ParamDecl {
                    name: "x".into(),
                    rust_kind: RustKind::String,
                    http_query: "x".into(),
                    parser: ParserKind::SchemaRefFromUri,
                }],
                http: HttpDecl {
                    path: "/bad".into(),
                },
                description: None,
                cli: None,
            }],
        };
        let err = validate_spec(&spec).unwrap_err();
        assert!(err.to_string().contains("does not match rust_kind"));
    }

    #[test]
    fn duplicate_query_names_rejected() {
        let spec = QuerySpec {
            description: None,
            queries: vec![
                QueryDecl {
                    name: "dup".into(),
                    entity: EntityKind::Bounty,
                    predicate: Some("p".into()),
                    expression: None,
                    params: vec![],
                    http: HttpDecl { path: "/a".into() },
                    description: None,
                    cli: None,
                },
                QueryDecl {
                    name: "dup".into(),
                    entity: EntityKind::Bounty,
                    predicate: Some("p".into()),
                    expression: None,
                    params: vec![],
                    http: HttpDecl { path: "/b".into() },
                    description: None,
                    cli: None,
                },
            ],
        };
        let err = validate_spec(&spec).unwrap_err();
        assert!(err.to_string().contains("duplicate query name"));
    }
}
