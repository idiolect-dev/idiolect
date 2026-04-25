//! End-to-end lens invocation tests.
//!
//! Exercises the full `apply_lens` / `apply_lens_put` pipeline against
//! minimal in-memory fixtures:
//!
//! - A single-field schema round-trip: forward `get` plus backward
//!   `put` reconstructing the original source verbatim.
//! - A two-field schema forward translation, showing the pipeline
//!   handles a realistic record with multiple named children.
//! - A protolens-chain forward translation, showing the runtime also
//!   accepts a multi-step `ProtolensChain` blob.
//!
//! In all cases the underlying steps are `rename_sort` variants:
//! lossless, so the complement is empty and the json shape is
//! preserved (only vertex kinds change).

use idiolect_lens::{
    ApplyLensInput, ApplyLensPutInput, InMemoryResolver, InMemorySchemaLoader, apply_lens,
    apply_lens_put,
};
use idiolect_records::PanprotoLens;
use panproto_lens::protolens::{ProtolensChain, elementary};
use panproto_schema::{Protocol, Schema, SchemaBuilder};

/// Permissive protocol: empty `obj_kinds` and `edge_rules` means the
/// schema builder accepts any vertex kind and any edge kind. Good
/// enough for rename-sort fixtures that only need "object", "string",
/// "datetime", and the "prop" edge kind to exist.
fn test_protocol() -> Protocol {
    Protocol::default()
}

/// Assemble the runtime inputs a test needs: the at-uri of a lens
/// record, an `InMemoryResolver` that answers with the given record,
/// and an `InMemorySchemaLoader` holding the source + target schemas
/// keyed by the hashes the record references.
fn stage_fixture(
    lens_uri_str: &str,
    src_schema: Schema,
    tgt_schema: Schema,
    protolens: &panproto_lens::protolens::Protolens,
) -> (idiolect_lens::AtUri, InMemoryResolver, InMemorySchemaLoader) {
    let src_hash = "sha256:src".to_owned();
    let tgt_hash = "sha256:tgt".to_owned();

    let mut loader = InMemorySchemaLoader::new();
    loader.insert(src_hash.clone(), src_schema);
    loader.insert(tgt_hash.clone(), tgt_schema);

    let blob = serde_json::to_value(protolens).expect("serialize protolens");
    let lens_uri = idiolect_lens::AtUri::parse(lens_uri_str).expect("parse lens at-uri");
    let lens_record = PanprotoLens {
        blob: Some(blob),
        created_at: "2026-04-19T00:00:00.000Z".to_owned(),
        laws_verified: Some(true),
        object_hash: "sha256:deadbeef".to_owned(),
        round_trip_class: Some("isomorphism".to_owned()),
        source_schema: src_hash,
        target_schema: tgt_hash,
    };

    let mut resolver = InMemoryResolver::new();
    resolver.insert(&lens_uri, lens_record);

    (lens_uri, resolver, loader)
}

/// Minimal single-child source schema: a `post:body` object with one
/// `string`-kinded `text` child reached by the named edge `text`.
fn single_field_source_schema() -> Schema {
    SchemaBuilder::new(&test_protocol())
        .entry("post:body")
        .vertex("post:body", "object", None)
        .unwrap()
        .vertex("post:body.text", "string", None)
        .unwrap()
        .edge("post:body", "post:body.text", "prop", Some("text"))
        .unwrap()
        .build()
        .unwrap()
}

/// Two-child source schema: adds a `datetime`-kinded `createdAt`
/// sibling, so the forward path exercises a record with multiple
/// named fields of distinct kinds.
fn two_field_source_schema() -> Schema {
    SchemaBuilder::new(&test_protocol())
        .entry("post:body")
        .vertex("post:body", "object", None)
        .unwrap()
        .vertex("post:body.text", "string", None)
        .unwrap()
        .vertex("post:body.createdAt", "datetime", None)
        .unwrap()
        .edge("post:body", "post:body.text", "prop", Some("text"))
        .unwrap()
        .edge(
            "post:body",
            "post:body.createdAt",
            "prop",
            Some("createdAt"),
        )
        .unwrap()
        .build()
        .unwrap()
}

#[tokio::test]
async fn apply_lens_round_trips_single_field_rename_sort() {
    let protocol = test_protocol();
    let src_schema = single_field_source_schema();

    // construct the protolens and derive the target schema from it so
    // the schema loader hands back exactly what the runtime's
    // instantiate step produces.
    let protolens = elementary::rename_sort("string", "text");
    let tgt_schema = protolens
        .target_schema(&src_schema, &protocol)
        .expect("derive target schema");

    let (lens_uri, resolver, loader) = stage_fixture(
        "at://did:plc:xyz/dev.panproto.schema.lens/l1",
        src_schema,
        tgt_schema,
        &protolens,
    );

    let source_record = serde_json::json!({
        "text": "hello, world",
    });

    // forward direction: get.
    let forward = apply_lens(
        &resolver,
        &loader,
        &protocol,
        ApplyLensInput {
            lens_uri: lens_uri.to_string(),
            source_record: source_record.clone(),
            source_root_vertex: None,
        },
    )
    .await
    .expect("forward apply_lens");

    // rename_sort only relabels kinds; edge names (and therefore json
    // shape) are unchanged, so the view equals the source.
    assert_eq!(forward.target_record, source_record);

    // backward direction: put. feed the unmodified view + complement
    // back through and check we reconstruct the source verbatim.
    let backward = apply_lens_put(
        &resolver,
        &loader,
        &protocol,
        ApplyLensPutInput {
            lens_uri: lens_uri.to_string(),
            target_record: forward.target_record,
            complement: forward.complement,
            target_root_vertex: None,
        },
    )
    .await
    .expect("backward apply_lens_put");

    assert_eq!(backward.source_record, source_record);
}

#[tokio::test]
async fn apply_lens_forward_preserves_multi_field_record() {
    let protocol = test_protocol();
    let src_schema = two_field_source_schema();

    let protolens = elementary::rename_sort("string", "text");
    let tgt_schema = protolens
        .target_schema(&src_schema, &protocol)
        .expect("derive target schema");

    let (lens_uri, resolver, loader) = stage_fixture(
        "at://did:plc:xyz/dev.panproto.schema.lens/l2",
        src_schema,
        tgt_schema,
        &protolens,
    );

    let source_record = serde_json::json!({
        "text": "hello, world",
        "createdAt": "2026-04-19T00:00:00Z",
    });

    let forward = apply_lens(
        &resolver,
        &loader,
        &protocol,
        ApplyLensInput {
            lens_uri: lens_uri.to_string(),
            source_record: source_record.clone(),
            source_root_vertex: None,
        },
    )
    .await
    .expect("forward apply_lens");

    // both fields survive the forward translation with values
    // attached to the correct edge names.
    assert_eq!(forward.target_record, source_record);
}

#[tokio::test]
async fn apply_lens_accepts_protolens_chain_blob() {
    // chain two `rename_sort` steps: string -> intermediate -> text.
    // the runtime must decode the blob as `LensBody::Chain`,
    // instantiate the whole chain against the source schema, and run
    // the fused lens end-to-end.
    let protocol = test_protocol();
    let src_schema = single_field_source_schema();

    let chain = ProtolensChain::new(vec![
        elementary::rename_sort("string", "intermediate"),
        elementary::rename_sort("intermediate", "text"),
    ]);

    // derive the target schema by instantiating the chain and asking
    // the lens for its `tgt_schema`. this mirrors what `auto_generate`
    // would produce in practice.
    let chain_lens = chain
        .instantiate(&src_schema, &protocol)
        .expect("instantiate chain");
    let tgt_schema = chain_lens.tgt_schema.clone();

    // stage the fixture. `stage_fixture` wants a `Protolens`, but the
    // untagged `LensBody` serde format lets us drop in a chain blob by
    // serializing the chain directly.
    let src_hash = "sha256:src".to_owned();
    let tgt_hash = "sha256:tgt".to_owned();

    let mut loader = InMemorySchemaLoader::new();
    loader.insert(src_hash.clone(), src_schema);
    loader.insert(tgt_hash.clone(), tgt_schema);

    let blob = serde_json::to_value(&chain).expect("serialize chain");
    let lens_uri = idiolect_lens::AtUri::parse("at://did:plc:xyz/dev.panproto.schema.lens/l3")
        .expect("parse lens at-uri");
    let lens_record = PanprotoLens {
        blob: Some(blob),
        created_at: "2026-04-19T00:00:00.000Z".to_owned(),
        laws_verified: Some(true),
        object_hash: "sha256:chain".to_owned(),
        round_trip_class: Some("isomorphism".to_owned()),
        source_schema: src_hash,
        target_schema: tgt_hash,
    };

    let mut resolver = InMemoryResolver::new();
    resolver.insert(&lens_uri, lens_record);

    let source_record = serde_json::json!({
        "text": "hello, chain",
    });

    let forward = apply_lens(
        &resolver,
        &loader,
        &protocol,
        ApplyLensInput {
            lens_uri: lens_uri.to_string(),
            source_record: source_record.clone(),
            source_root_vertex: None,
        },
    )
    .await
    .expect("forward apply_lens on chain");

    // rename_sort preserves edge labels, so the json shape is
    // unchanged through the chain.
    assert_eq!(forward.target_record, source_record);
}
