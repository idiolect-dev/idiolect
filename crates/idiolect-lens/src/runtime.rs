//! `apply_lens` runtime — the idiolect counterpart to
//! `dev.panproto.translate.applyLens`.
//!
//! The runtime is a thin orchestrator over three pluggable boundaries:
//!
//! - a [`Resolver`] (fetches the lens record),
//! - a [`SchemaLoader`] (fetches the source and target schemas by
//!   their content-addressed hashes),
//! - a caller-supplied [`panproto_schema::Protocol`] (identifies the
//!   schema theory the lens is interpreted under — for atproto,
//!   `panproto_protocols::atproto::protocol()`).
//!
//! # Lens types supported
//!
//! The runtime decodes a `dev.panproto.schema.lens` blob into a
//! [`LensBody`], which covers the two protolens-level shapes panproto
//! serializes:
//!
//! - A single [`Protolens`] (one elementary step — `rename_sort`,
//!   `add_sort`, etc.).
//! - A [`ProtolensChain`] (a sequential pipeline of elementary steps,
//!   e.g. as emitted by `panproto_lens::auto_generate`).
//!
//! On top of that decoded body, the runtime exposes four pipelines:
//!
//! - [`apply_lens`] / [`apply_lens_put`] — state-based `get`/`put` on
//!   asymmetric [`panproto_lens::Lens`] values. Also the carrier of
//!   panproto's Grothendieck-fibration structure: `get` is the
//!   fibration projection and `put` the cartesian lift, so dependent
//!   optics (see [`panproto_lens::fibration`]) are covered by the same
//!   entry points.
//! - [`apply_lens_get_edit`] / [`apply_lens_put_edit`] — edit-based
//!   translation through [`panproto_lens::EditLens`]. Takes a source
//!   record plus a sequence of [`TreeEdit`] values and returns the
//!   translated edits (and the updated complement).
//! - [`apply_lens_symmetric`] — compose two resolved lenses into a
//!   [`SymmetricLens`] over a shared middle schema, then run either
//!   leg.

use panproto_inst::parse::{parse_json, to_json};
use panproto_inst::{TreeEdit, WInstance};
use panproto_lens::protolens::{Protolens, ProtolensChain};
use panproto_lens::{Complement, EditLens, Lens, SymmetricLens};
use panproto_schema::{Protocol, Schema, primary_entry};
use serde::{Deserialize, Serialize};

use crate::error::LensError;
use crate::resolver::Resolver;
use crate::schema_loader::SchemaLoader;

// -----------------------------------------------------------------
// lens body decoding
// -----------------------------------------------------------------

/// The schema-parameterized lens expression stored inside a
/// `dev.panproto.schema.lens` record's `blob`.
///
/// panproto serializes both single protolenses and protolens chains as
/// plain json; they are structurally distinct (`Protolens` is a struct
/// with `name`, `source`, `target`, `complement_constructor` fields;
/// `ProtolensChain` is a struct with a `steps` field), so the
/// `#[serde(untagged)]` dispatch is unambiguous.
///
/// The `Single` variant is boxed because `Protolens` is substantially
/// larger than `ProtolensChain` (which is only a `Vec`), and
/// clippy's `large_enum_variant` rule would otherwise flag the size
/// disparity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LensBody {
    /// A pipeline of elementary protolens steps, as emitted by
    /// `panproto_lens::auto_generate` or user-composed via
    /// [`ProtolensChain::new`].
    Chain(ProtolensChain),
    /// A single elementary protolens step.
    Single(Box<Protolens>),
}

impl LensBody {
    /// Instantiate this lens body at a specific source schema under
    /// a protocol, producing a concrete [`Lens`].
    ///
    /// # Errors
    ///
    /// Returns [`LensError::Instantiate`] if any step's theory
    /// transform cannot be applied at the given schema.
    pub fn instantiate(&self, schema: &Schema, protocol: &Protocol) -> Result<Lens, LensError> {
        match self {
            Self::Chain(chain) => chain
                .instantiate(schema, protocol)
                .map_err(|e| LensError::Instantiate(e.to_string())),
            Self::Single(protolens) => protolens
                .instantiate(schema, protocol)
                .map_err(|e| LensError::Instantiate(e.to_string())),
        }
    }
}

// -----------------------------------------------------------------
// inputs / outputs (state-based)
// -----------------------------------------------------------------

/// Input to the forward (`get`) direction of the state-based runtime.
///
/// The source record is the json value already in hand (from a
/// firehose event, an xrpc call, disk, etc.); this runtime does not
/// own record fetching. The source root vertex is optional: by
/// default the runtime consults
/// [`panproto_schema::primary_entry`] on the source schema.
#[derive(Debug, Clone)]
pub struct ApplyLensInput {
    /// At-uri of the `dev.panproto.schema.lens` record to apply.
    pub lens_uri: crate::AtUri,
    /// The source record body as json.
    pub source_record: serde_json::Value,
    /// Override the source root vertex. When `None`, the runtime uses
    /// [`panproto_schema::primary_entry`] on the source schema.
    pub source_root_vertex: Option<String>,
}

/// Output of the forward (`get`) direction of the state-based runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyLensOutput {
    /// The translated target-schema record as json.
    pub target_record: serde_json::Value,
    /// Everything `get` discarded. Round-trip through [`apply_lens_put`]
    /// reconstructs the original source.
    pub complement: Complement,
}

/// Input to the backward (`put`) direction of the state-based runtime.
#[derive(Debug, Clone)]
pub struct ApplyLensPutInput {
    /// At-uri of the same `dev.panproto.schema.lens` record used in
    /// the forward direction.
    pub lens_uri: crate::AtUri,
    /// The (possibly modified) target record as json.
    pub target_record: serde_json::Value,
    /// Complement produced by the forward direction for this record.
    pub complement: Complement,
    /// Override the target root vertex. When `None`, the runtime uses
    /// [`panproto_schema::primary_entry`] on the target schema.
    pub target_root_vertex: Option<String>,
}

/// Output of the backward (`put`) direction of the state-based runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyLensPutOutput {
    /// The reconstructed source-schema record as json.
    pub source_record: serde_json::Value,
}

// -----------------------------------------------------------------
// inputs / outputs (edit-based)
// -----------------------------------------------------------------

/// Input to the edit-based runtime entry points.
///
/// The source record seeds an [`EditLens`]'s internal complement state
/// (via `EditLens::initialize`); the edits are then translated one by
/// one through the migration pipeline.
#[derive(Debug, Clone)]
pub struct ApplyLensEditInput {
    /// At-uri of the lens to apply.
    pub lens_uri: crate::AtUri,
    /// The source record body, used to prime the edit lens's
    /// complement state. Required because edit lenses maintain a
    /// stateful complement derived from a whole-state `get`.
    pub source_record: serde_json::Value,
    /// Override the source root vertex. When `None`, the runtime uses
    /// [`panproto_schema::primary_entry`] on the source schema.
    pub source_root_vertex: Option<String>,
    /// The sequence of source-side edits to translate forward (for
    /// [`apply_lens_get_edit`]) or target-side edits to translate
    /// backward (for [`apply_lens_put_edit`]).
    pub edits: Vec<TreeEdit>,
}

/// Output of the edit-based runtime entry points.
#[derive(Debug, Clone)]
pub struct ApplyLensEditOutput {
    /// The translated edit sequence.
    ///
    /// One translated edit per input edit, in order. Pipeline steps
    /// that absorb an edit into the complement produce
    /// [`TreeEdit::Identity`] at that index.
    pub translated_edits: Vec<TreeEdit>,
    /// The edit lens's complement after all edits have been applied.
    pub final_complement: Complement,
}

// -----------------------------------------------------------------
// inputs / outputs (symmetric)
// -----------------------------------------------------------------

/// Input to the symmetric-lens runtime.
///
/// A symmetric lens is built from two asymmetric lenses that share a
/// source (the "middle") schema. The runtime resolves both lens
/// records, constructs a [`SymmetricLens`] via
/// [`SymmetricLens::from_span`], and runs the requested direction.
#[derive(Debug, Clone)]
pub struct ApplyLensSymmetricInput {
    /// At-uri of the lens whose source schema is the middle and whose
    /// target schema is the "left" leg's target.
    pub left_lens_uri: crate::AtUri,
    /// At-uri of the lens whose source schema is the same middle and
    /// whose target schema is the "right" leg's target.
    pub right_lens_uri: crate::AtUri,
    /// The input record for the chosen direction, as json.
    pub record: serde_json::Value,
    /// Which direction to run:
    /// - [`SymmetricDirection::LeftToRight`] consumes a left-target
    ///   record and returns a right-target record.
    /// - [`SymmetricDirection::RightToLeft`] is the dual.
    pub direction: SymmetricDirection,
    /// Override the input record's root vertex. When `None`, the
    /// runtime uses [`panproto_schema::primary_entry`] on the
    /// appropriate target schema.
    pub input_root_vertex: Option<String>,
}

/// Direction to run a symmetric lens in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymmetricDirection {
    /// Consume a record typed at the left leg's target; produce a
    /// record typed at the right leg's target.
    LeftToRight,
    /// Consume a record typed at the right leg's target; produce a
    /// record typed at the left leg's target.
    RightToLeft,
}

/// Output of the symmetric-lens runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyLensSymmetricOutput {
    /// The record translated into the opposite leg's schema.
    pub record: serde_json::Value,
}

// -----------------------------------------------------------------
// forward (get)
// -----------------------------------------------------------------

/// Run the forward (`get`) direction of a lens.
///
/// Resolves the lens record, loads both schemas, instantiates the
/// protolens (or protolens chain) against the source schema under
/// `protocol`, parses the input json into a panproto w-type instance,
/// projects it through the lens, and serializes the view back to json
/// under the target schema. The complement captures everything the
/// projection discarded.
///
/// # Errors
///
/// Any [`LensError`] variant — see each `Resolver` / `SchemaLoader`
/// implementation and the panproto crates for the underlying failure
/// modes.
pub async fn apply_lens<R, S>(
    resolver: &R,
    schema_loader: &S,
    protocol: &Protocol,
    input: ApplyLensInput,
) -> Result<ApplyLensOutput, LensError>
where
    R: Resolver,
    S: SchemaLoader,
{
    let (lens, _src_schema, tgt_schema, src_instance) = build_lens_and_source(
        resolver,
        schema_loader,
        protocol,
        &input.lens_uri,
        &input.source_record,
        input.source_root_vertex.as_deref(),
    )
    .await?;

    let (view, complement) = panproto_lens::get(&lens, &src_instance)
        .map_err(|e| LensError::Translate(e.to_string()))?;

    let target_record = to_json(&tgt_schema, &view);

    Ok(ApplyLensOutput {
        target_record,
        complement,
    })
}

// -----------------------------------------------------------------
// backward (put)
// -----------------------------------------------------------------

/// Run the backward (`put`) direction of a lens.
///
/// Given a target-view record and the complement produced when that
/// view was obtained via [`apply_lens`], reconstruct the source
/// record.
///
/// # Errors
///
/// Returns any [`LensError`] variant produced by the resolver,
/// schema loader, instantiation, parse, or lens-apply step.
pub async fn apply_lens_put<R, S>(
    resolver: &R,
    schema_loader: &S,
    protocol: &Protocol,
    input: ApplyLensPutInput,
) -> Result<ApplyLensPutOutput, LensError>
where
    R: Resolver,
    S: SchemaLoader,
{
    let lens_record = resolver.resolve(&input.lens_uri).await?;
    let body = decode_lens_body(&lens_record, input.lens_uri.as_str())?;
    let src_schema = schema_loader
        .load(lens_record.source_schema.as_str())
        .await?;
    let tgt_schema = schema_loader
        .load(lens_record.target_schema.as_str())
        .await?;

    let lens = body.instantiate(&src_schema, protocol)?;

    // parse the target view against the target schema.
    let target_root = pick_root(&tgt_schema, input.target_root_vertex.as_deref())?;
    let view = parse_json(&tgt_schema, &target_root, &input.target_record)
        .map_err(|e| LensError::InstanceParse(e.to_string()))?;

    let reconstructed = panproto_lens::put(&lens, &view, &input.complement)
        .map_err(|e| LensError::Translate(e.to_string()))?;

    let source_record = to_json(&lens.src_schema, &reconstructed);

    Ok(ApplyLensPutOutput { source_record })
}

// -----------------------------------------------------------------
// edit-based forward (get_edit)
// -----------------------------------------------------------------

/// Translate a sequence of source-side edits forward through an
/// [`EditLens`].
///
/// The source record seeds the edit lens's internal complement state;
/// each edit is then passed through the five-step translation
/// pipeline (anchor survival, reachability, ancestor contraction,
/// edge resolution, fan reconstruction). The output is the
/// corresponding sequence of view-side edits plus the final
/// complement.
///
/// # Errors
///
/// Returns [`LensError::Translate`] for any pipeline failure.
pub async fn apply_lens_get_edit<R, S>(
    resolver: &R,
    schema_loader: &S,
    protocol: &Protocol,
    input: ApplyLensEditInput,
) -> Result<ApplyLensEditOutput, LensError>
where
    R: Resolver,
    S: SchemaLoader,
{
    let (mut edit_lens, src_instance) = build_edit_lens_and_source(
        resolver,
        schema_loader,
        protocol,
        &input.lens_uri,
        &input.source_record,
        input.source_root_vertex.as_deref(),
    )
    .await?;

    edit_lens
        .initialize(&src_instance)
        .map_err(|e| LensError::Translate(e.to_string()))?;

    let mut translated_edits = Vec::with_capacity(input.edits.len());
    for edit in input.edits {
        let out = edit_lens
            .get_edit(edit)
            .map_err(|e| LensError::Translate(e.to_string()))?;
        translated_edits.push(out);
    }

    Ok(ApplyLensEditOutput {
        translated_edits,
        final_complement: edit_lens.complement.clone(),
    })
}

/// Translate a sequence of target-side edits backward through an
/// [`EditLens`] into source-side edits.
///
/// # Errors
///
/// Returns [`LensError::Translate`] for any pipeline failure.
pub async fn apply_lens_put_edit<R, S>(
    resolver: &R,
    schema_loader: &S,
    protocol: &Protocol,
    input: ApplyLensEditInput,
) -> Result<ApplyLensEditOutput, LensError>
where
    R: Resolver,
    S: SchemaLoader,
{
    let (mut edit_lens, src_instance) = build_edit_lens_and_source(
        resolver,
        schema_loader,
        protocol,
        &input.lens_uri,
        &input.source_record,
        input.source_root_vertex.as_deref(),
    )
    .await?;

    edit_lens
        .initialize(&src_instance)
        .map_err(|e| LensError::Translate(e.to_string()))?;

    let mut translated_edits = Vec::with_capacity(input.edits.len());
    for edit in input.edits {
        let out = edit_lens
            .put_edit(edit)
            .map_err(|e| LensError::Translate(e.to_string()))?;
        translated_edits.push(out);
    }

    Ok(ApplyLensEditOutput {
        translated_edits,
        final_complement: edit_lens.complement.clone(),
    })
}

// -----------------------------------------------------------------
// symmetric
// -----------------------------------------------------------------

/// Run a symmetric lens built from two panproto lens records that
/// share a source (middle) schema.
///
/// Resolves both lens records, instantiates them against the shared
/// middle schema, composes them into a [`SymmetricLens`], then runs
/// the requested direction. The middle complement is initialized from
/// the input record via the appropriate leg's `get`.
///
/// # Errors
///
/// Returns [`LensError::Translate`] if span construction or
/// translation fails; propagates resolver / schema-loader /
/// instantiation errors.
pub async fn apply_lens_symmetric<R, S>(
    resolver: &R,
    schema_loader: &S,
    protocol: &Protocol,
    input: ApplyLensSymmetricInput,
) -> Result<ApplyLensSymmetricOutput, LensError>
where
    R: Resolver,
    S: SchemaLoader,
{
    // resolve both records; both must reference the same source-schema
    // hash (the middle of the span).
    let left_record = resolver.resolve(&input.left_lens_uri).await?;
    let right_record = resolver.resolve(&input.right_lens_uri).await?;

    if left_record.source_schema != right_record.source_schema {
        return Err(LensError::Translate(format!(
            "symmetric lens legs do not share a source schema: left={}, right={}",
            left_record.source_schema, right_record.source_schema,
        )));
    }

    let middle_schema = schema_loader
        .load(left_record.source_schema.as_str())
        .await?;
    let left_tgt = schema_loader
        .load(left_record.target_schema.as_str())
        .await?;
    let right_tgt = schema_loader
        .load(right_record.target_schema.as_str())
        .await?;

    let left_body = decode_lens_body(&left_record, input.left_lens_uri.as_str())?;
    let right_body = decode_lens_body(&right_record, input.right_lens_uri.as_str())?;

    let left_lens = left_body.instantiate(&middle_schema, protocol)?;
    let right_lens = right_body.instantiate(&middle_schema, protocol)?;

    let sym = SymmetricLens::from_span(left_lens, right_lens)
        .map_err(|e| LensError::Translate(e.to_string()))?;

    match input.direction {
        SymmetricDirection::LeftToRight => {
            let root = pick_root(&left_tgt, input.input_root_vertex.as_deref())?;
            let left_view = parse_json(&left_tgt, &root, &input.record)
                .map_err(|e| LensError::InstanceParse(e.to_string()))?;

            // step one: `put` the left-view back to the middle (the span's
            // shared source).
            let middle_instance = panproto_lens::put(&sym.left, &left_view, &Complement::empty())
                .map_err(|e| LensError::Translate(e.to_string()))?;
            // step two: `get` the middle forward to the right view.
            let (right_view, _) = panproto_lens::get(&sym.right, &middle_instance)
                .map_err(|e| LensError::Translate(e.to_string()))?;

            Ok(ApplyLensSymmetricOutput {
                record: to_json(&right_tgt, &right_view),
            })
        }
        SymmetricDirection::RightToLeft => {
            let root = pick_root(&right_tgt, input.input_root_vertex.as_deref())?;
            let right_view = parse_json(&right_tgt, &root, &input.record)
                .map_err(|e| LensError::InstanceParse(e.to_string()))?;

            let middle_instance = panproto_lens::put(&sym.right, &right_view, &Complement::empty())
                .map_err(|e| LensError::Translate(e.to_string()))?;
            let (left_view, _) = panproto_lens::get(&sym.left, &middle_instance)
                .map_err(|e| LensError::Translate(e.to_string()))?;

            Ok(ApplyLensSymmetricOutput {
                record: to_json(&left_tgt, &left_view),
            })
        }
    }
}

// -----------------------------------------------------------------
// shared pipeline pieces
// -----------------------------------------------------------------

/// Shared front half of the state-based forward direction: resolve +
/// load + instantiate + parse source.
async fn build_lens_and_source<R, S>(
    resolver: &R,
    schema_loader: &S,
    protocol: &Protocol,
    lens_uri: &crate::AtUri,
    source_record: &serde_json::Value,
    source_root_vertex: Option<&str>,
) -> Result<(Lens, Schema, Schema, WInstance), LensError>
where
    R: Resolver,
    S: SchemaLoader,
{
    let lens_record = resolver.resolve(lens_uri).await?;

    let body = decode_lens_body(&lens_record, lens_uri.as_str())?;

    let src_schema = schema_loader
        .load(lens_record.source_schema.as_str())
        .await?;
    let tgt_schema = schema_loader
        .load(lens_record.target_schema.as_str())
        .await?;

    let lens = body.instantiate(&src_schema, protocol)?;

    let source_root = pick_root(&lens.src_schema, source_root_vertex)?;
    let src_instance = parse_json(&lens.src_schema, &source_root, source_record)
        .map_err(|e| LensError::InstanceParse(e.to_string()))?;

    Ok((lens, src_schema, tgt_schema, src_instance))
}

/// Shared front half of the edit-based pipelines: resolve + load +
/// instantiate + parse source + wrap as an [`EditLens`].
async fn build_edit_lens_and_source<R, S>(
    resolver: &R,
    schema_loader: &S,
    protocol: &Protocol,
    lens_uri: &crate::AtUri,
    source_record: &serde_json::Value,
    source_root_vertex: Option<&str>,
) -> Result<(EditLens, WInstance), LensError>
where
    R: Resolver,
    S: SchemaLoader,
{
    let (lens, _src_schema, _tgt_schema, src_instance) = build_lens_and_source(
        resolver,
        schema_loader,
        protocol,
        lens_uri,
        source_record,
        source_root_vertex,
    )
    .await?;

    Ok((EditLens::from_lens(lens, protocol.clone()), src_instance))
}

/// Resolve the root vertex for a schema: use the override when
/// provided, otherwise fall back to
/// [`panproto_schema::primary_entry`].
fn pick_root(schema: &Schema, override_root: Option<&str>) -> Result<String, LensError> {
    if let Some(root) = override_root {
        return Ok(root.to_owned());
    }
    primary_entry(schema)
        .map(std::string::ToString::to_string)
        .ok_or_else(|| {
            LensError::InstanceParse(
                "schema declares no entries and no root override was supplied".to_owned(),
            )
        })
}

/// Deserialize the lens body stashed in a `PanprotoLens::blob`.
fn decode_lens_body(
    lens_record: &idiolect_records::PanprotoLens,
    lens_uri: &str,
) -> Result<LensBody, LensError> {
    let blob = lens_record
        .blob
        .clone()
        .ok_or_else(|| LensError::MissingBody(lens_uri.to_owned()))?;

    serde_json::from_value::<LensBody>(blob).map_err(LensError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::InMemoryResolver;
    use crate::schema_loader::InMemorySchemaLoader;
    use idiolect_records::PanprotoLens;
    use panproto_lens::protolens::elementary;
    use panproto_schema::Protocol;

    fn fixture_lens_record(blob: Option<serde_json::Value>) -> PanprotoLens {
        PanprotoLens {
            blob,
            created_at: idiolect_records::Datetime::parse("2026-04-19T00:00:00.000Z")
                .expect("valid datetime"),
            laws_verified: None,
            object_hash: "sha256:deadbeef".to_owned(),
            round_trip_class: None,
            source_schema: idiolect_records::AtUri::parse(
                "at://did:plc:x/dev.panproto.schema.schema/src",
            )
            .expect("valid at-uri"),
            target_schema: idiolect_records::AtUri::parse(
                "at://did:plc:x/dev.panproto.schema.schema/tgt",
            )
            .expect("valid at-uri"),
        }
    }

    #[tokio::test]
    async fn forward_errors_when_lens_not_found() {
        let resolver = InMemoryResolver::new();
        let loader = InMemorySchemaLoader::new();
        let protocol = Protocol::default();

        let out = apply_lens(
            &resolver,
            &loader,
            &protocol,
            ApplyLensInput {
                lens_uri: crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/missing")
                    .expect("valid at-uri"),
                source_record: serde_json::json!({}),
                source_root_vertex: None,
            },
        )
        .await;

        assert!(matches!(out.unwrap_err(), LensError::NotFound(_)));
    }

    #[tokio::test]
    async fn forward_errors_when_blob_missing() {
        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/l1").unwrap();
        let mut resolver = InMemoryResolver::new();
        resolver.insert(&uri, fixture_lens_record(None));

        let loader = InMemorySchemaLoader::new();
        let protocol = Protocol::default();

        let out = apply_lens(
            &resolver,
            &loader,
            &protocol,
            ApplyLensInput {
                lens_uri: uri.clone(),
                source_record: serde_json::json!({}),
                source_root_vertex: None,
            },
        )
        .await;

        assert!(matches!(out.unwrap_err(), LensError::MissingBody(s) if s == uri.to_string()));
    }

    #[tokio::test]
    async fn forward_errors_when_blob_decode_fails() {
        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/l1").unwrap();
        let mut resolver = InMemoryResolver::new();
        // not a valid protolens or chain shape
        resolver.insert(
            &uri,
            fixture_lens_record(Some(serde_json::json!({"not": "a protolens"}))),
        );

        let loader = InMemorySchemaLoader::new();
        let protocol = Protocol::default();

        let out = apply_lens(
            &resolver,
            &loader,
            &protocol,
            ApplyLensInput {
                lens_uri: uri.clone(),
                source_record: serde_json::json!({}),
                source_root_vertex: None,
            },
        )
        .await;

        assert!(matches!(out.unwrap_err(), LensError::Decode(_)));
    }

    #[tokio::test]
    async fn forward_errors_when_source_schema_not_found() {
        // a protolens with the simplest possible shape — it only matters
        // that it deserializes; we fail upstream at schema load.
        let protolens = elementary::rename_sort("a", "b");
        let blob = serde_json::to_value(&protolens).unwrap();

        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/l1").unwrap();
        let mut resolver = InMemoryResolver::new();
        resolver.insert(&uri, fixture_lens_record(Some(blob)));

        let loader = InMemorySchemaLoader::new();
        let protocol = Protocol::default();

        let out = apply_lens(
            &resolver,
            &loader,
            &protocol,
            ApplyLensInput {
                lens_uri: uri.clone(),
                source_record: serde_json::json!({}),
                source_root_vertex: None,
            },
        )
        .await;

        assert!(
            matches!(out.unwrap_err(), LensError::NotFound(m) if m.contains("at://did:plc:x/dev.panproto.schema.schema/src"))
        );
    }

    #[tokio::test]
    async fn lens_body_decodes_chain_blob() {
        // a chain of one step deserializes into `LensBody::Chain`, not
        // `LensBody::Single`.
        let chain = ProtolensChain::new(vec![elementary::rename_sort("a", "b")]);
        let blob = serde_json::to_value(&chain).unwrap();
        let body: LensBody = serde_json::from_value(blob).unwrap();
        assert!(matches!(body, LensBody::Chain(_)));
    }

    #[tokio::test]
    async fn lens_body_decodes_single_blob() {
        let protolens = elementary::rename_sort("a", "b");
        let blob = serde_json::to_value(&protolens).unwrap();
        let body: LensBody = serde_json::from_value(blob).unwrap();
        assert!(matches!(body, LensBody::Single(_)));
    }

    fn tiny_schema() -> Schema {
        use panproto_schema::SchemaBuilder;
        SchemaBuilder::new(&Protocol::default())
            .entry("canonical")
            .vertex("canonical", "object", None)
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    fn pick_root_uses_override_when_supplied() {
        let schema = tiny_schema();
        let got = pick_root(&schema, Some("override")).unwrap();
        assert_eq!(got, "override");
    }

    #[test]
    fn pick_root_falls_back_to_primary_entry() {
        let schema = tiny_schema();
        let got = pick_root(&schema, None).unwrap();
        assert_eq!(got, "canonical");
    }
}
