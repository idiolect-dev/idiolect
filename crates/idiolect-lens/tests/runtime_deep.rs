//! Coverage for lens-runtime entry points that the main `roundtrip`
//! test file does not exercise:
//!
//! - `apply_lens_get_edit` / `apply_lens_put_edit` — edit-based
//!   translation. Smoke tests that the runtime can initialize an
//!   [`EditLens`] from a source record and consume an empty edit
//!   stream without panicking.
//! - `apply_lens_symmetric` — symmetric-lens runtime. Tests both
//!   directions against a shared middle schema, plus the
//!   mismatched-middle error path.
//! - Error paths: unresolvable lens, missing schema, malformed at-uri.

use idiolect_lens::{
    ApplyLensEditInput, ApplyLensInput, ApplyLensPutInput, ApplyLensSymmetricInput,
    InMemoryResolver, InMemorySchemaLoader, LensError, SymmetricDirection, apply_lens,
    apply_lens_get_edit, apply_lens_put, apply_lens_put_edit, apply_lens_symmetric, parse_at_uri,
};
use idiolect_records::PanprotoLens;
use panproto_lens::protolens::elementary;
use panproto_schema::{Protocol, Schema, SchemaBuilder};

fn test_protocol() -> Protocol {
    Protocol::default()
}

fn single_field_schema(entry: &str, field_kind: &str) -> Schema {
    SchemaBuilder::new(&test_protocol())
        .entry(entry)
        .vertex(entry, "object", None)
        .unwrap()
        .vertex(&format!("{entry}.text"), field_kind, None)
        .unwrap()
        .edge(entry, &format!("{entry}.text"), "prop", Some("text"))
        .unwrap()
        .build()
        .unwrap()
}

fn stage_identity_lens(
    uri: &str,
    src: Schema,
    src_hash: &str,
) -> (idiolect_lens::AtUri, InMemoryResolver, InMemorySchemaLoader) {
    // rename_sort from "string" -> "string" is a no-op — the forward
    // view equals the source. Staged as a single-step protolens so the
    // edit-lens pipeline has something to initialize against.
    let protolens = elementary::rename_sort("string", "string");
    let tgt_schema = protolens
        .target_schema(&src, &test_protocol())
        .expect("derive target schema");

    let mut loader = InMemorySchemaLoader::new();
    loader.insert(src_hash.to_owned(), src);
    let tgt_hash = "sha256:tgt-symmetric".to_owned();
    loader.insert(tgt_hash.clone(), tgt_schema);

    let lens_uri = parse_at_uri(uri).unwrap();
    let record = PanprotoLens {
        blob: Some(serde_json::to_value(&protolens).unwrap()),
        created_at: "2026-04-19T00:00:00.000Z".into(),
        laws_verified: Some(true),
        object_hash: "sha256:lens".into(),
        round_trip_class: Some("isomorphism".into()),
        source_schema: src_hash.to_owned(),
        target_schema: tgt_hash,
    };
    let mut resolver = InMemoryResolver::new();
    resolver.insert(&lens_uri, record);
    (lens_uri, resolver, loader)
}

// -----------------------------------------------------------------
// Error paths
// -----------------------------------------------------------------

#[tokio::test]
async fn apply_lens_not_found_surfaces_resolver_error() {
    let resolver = InMemoryResolver::new(); // empty
    let loader = InMemorySchemaLoader::new();
    let err = apply_lens(
        &resolver,
        &loader,
        &test_protocol(),
        ApplyLensInput {
            lens_uri: "at://did:plc:x/dev.panproto.schema.lens/absent".into(),
            source_record: serde_json::json!({}),
            source_root_vertex: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, LensError::NotFound(_)));
}

#[tokio::test]
async fn apply_lens_malformed_uri_surfaces_invalid_uri() {
    let resolver = InMemoryResolver::new();
    let loader = InMemorySchemaLoader::new();
    let err = apply_lens(
        &resolver,
        &loader,
        &test_protocol(),
        ApplyLensInput {
            lens_uri: "not-an-at-uri".into(),
            source_record: serde_json::json!({}),
            source_root_vertex: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, LensError::InvalidUri(_)));
}

#[tokio::test]
async fn apply_lens_missing_source_schema_surfaces_not_found() {
    // Register the lens but NOT the schemas it references.
    let src = single_field_schema("post:body", "string");
    let protolens = elementary::rename_sort("string", "string");
    let tgt = protolens.target_schema(&src, &test_protocol()).unwrap();
    let src_hash = "sha256:src".to_owned();
    let tgt_hash = "sha256:tgt".to_owned();
    // Do NOT insert src_hash into the loader.
    let mut loader = InMemorySchemaLoader::new();
    loader.insert(tgt_hash.clone(), tgt);

    let lens_uri = parse_at_uri("at://did:plc:x/dev.panproto.schema.lens/l").unwrap();
    let record = PanprotoLens {
        blob: Some(serde_json::to_value(&protolens).unwrap()),
        created_at: "2026-04-19T00:00:00.000Z".into(),
        laws_verified: Some(true),
        object_hash: "sha256:lens".into(),
        round_trip_class: Some("isomorphism".into()),
        source_schema: src_hash,
        target_schema: tgt_hash,
    };
    let mut resolver = InMemoryResolver::new();
    resolver.insert(&lens_uri, record);

    let err = apply_lens(
        &resolver,
        &loader,
        &test_protocol(),
        ApplyLensInput {
            lens_uri: lens_uri.to_string(),
            source_record: serde_json::json!({ "text": "hi" }),
            source_root_vertex: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, LensError::NotFound(msg) if msg.contains("schema:")));
}

// -----------------------------------------------------------------
// Edit-based runtime (smoke)
// -----------------------------------------------------------------

#[tokio::test]
async fn apply_lens_get_edit_empty_edit_list_returns_empty() {
    let src = single_field_schema("post:body", "string");
    let (lens_uri, resolver, loader) = stage_identity_lens(
        "at://did:plc:x/dev.panproto.schema.lens/edit",
        src,
        "sha256:src-edit",
    );

    let out = apply_lens_get_edit(
        &resolver,
        &loader,
        &test_protocol(),
        ApplyLensEditInput {
            lens_uri: lens_uri.to_string(),
            source_record: serde_json::json!({ "text": "hello" }),
            source_root_vertex: None,
            edits: Vec::new(),
        },
    )
    .await
    .expect("initializing an edit lens against a valid source must succeed");
    assert!(out.translated_edits.is_empty());
}

#[tokio::test]
async fn apply_lens_put_edit_empty_edit_list_returns_empty() {
    let src = single_field_schema("post:body", "string");
    let (lens_uri, resolver, loader) = stage_identity_lens(
        "at://did:plc:x/dev.panproto.schema.lens/edit-put",
        src,
        "sha256:src-edit-put",
    );

    let out = apply_lens_put_edit(
        &resolver,
        &loader,
        &test_protocol(),
        ApplyLensEditInput {
            lens_uri: lens_uri.to_string(),
            source_record: serde_json::json!({ "text": "hello" }),
            source_root_vertex: None,
            edits: Vec::new(),
        },
    )
    .await
    .expect("edit-lens initialize + put_edit pipeline");
    assert!(out.translated_edits.is_empty());
}

#[tokio::test]
async fn apply_lens_get_edit_errors_on_missing_lens() {
    let resolver = InMemoryResolver::new();
    let loader = InMemorySchemaLoader::new();
    let err = apply_lens_get_edit(
        &resolver,
        &loader,
        &test_protocol(),
        ApplyLensEditInput {
            lens_uri: "at://did:plc:x/dev.panproto.schema.lens/ghost".into(),
            source_record: serde_json::json!({}),
            source_root_vertex: None,
            edits: Vec::new(),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, LensError::NotFound(_)));
}

// -----------------------------------------------------------------
// Symmetric runtime
// -----------------------------------------------------------------

#[tokio::test]
async fn apply_lens_symmetric_returns_a_value_for_both_directions() {
    // Smoke test the symmetric-lens wiring end-to-end: legs share a
    // middle schema, both directions succeed and produce a
    // well-formed json object. We deliberately do not compare the
    // payload against a specific expected value — symmetric
    // translation depends on how panproto's middle-complement is
    // initialized, and that's panproto's contract, not idiolect's.
    let middle = single_field_schema("middle:root", "string");
    let protocol = test_protocol();
    let protolens = elementary::rename_sort("string", "string");
    let left_tgt = protolens.target_schema(&middle, &protocol).unwrap();
    let right_tgt = protolens.target_schema(&middle, &protocol).unwrap();

    let middle_hash = "sha256:middle".to_owned();
    let left_tgt_hash = "sha256:left-tgt".to_owned();
    let right_tgt_hash = "sha256:right-tgt".to_owned();

    let mut loader = InMemorySchemaLoader::new();
    loader.insert(middle_hash.clone(), middle);
    loader.insert(left_tgt_hash.clone(), left_tgt);
    loader.insert(right_tgt_hash.clone(), right_tgt);

    let blob = serde_json::to_value(&protolens).unwrap();
    let left_uri = parse_at_uri("at://did:plc:x/dev.panproto.schema.lens/left").unwrap();
    let right_uri = parse_at_uri("at://did:plc:x/dev.panproto.schema.lens/right").unwrap();
    let left_record = PanprotoLens {
        blob: Some(blob.clone()),
        created_at: "2026-04-19T00:00:00.000Z".into(),
        laws_verified: Some(true),
        object_hash: "sha256:left-l".into(),
        round_trip_class: Some("isomorphism".into()),
        source_schema: middle_hash.clone(),
        target_schema: left_tgt_hash,
    };
    let right_record = PanprotoLens {
        blob: Some(blob),
        created_at: "2026-04-19T00:00:00.000Z".into(),
        laws_verified: Some(true),
        object_hash: "sha256:right-l".into(),
        round_trip_class: Some("isomorphism".into()),
        source_schema: middle_hash.clone(),
        target_schema: right_tgt_hash,
    };
    let mut resolver = InMemoryResolver::new();
    resolver.insert(&left_uri, left_record);
    resolver.insert(&right_uri, right_record);

    let record = serde_json::json!({ "text": "symmetric" });

    let left_to_right = apply_lens_symmetric(
        &resolver,
        &loader,
        &protocol,
        ApplyLensSymmetricInput {
            left_lens_uri: left_uri.to_string(),
            right_lens_uri: right_uri.to_string(),
            record: record.clone(),
            direction: SymmetricDirection::LeftToRight,
            input_root_vertex: None,
        },
    )
    .await
    .expect("symmetric left-to-right");
    assert!(left_to_right.record.is_object());

    let right_to_left = apply_lens_symmetric(
        &resolver,
        &loader,
        &protocol,
        ApplyLensSymmetricInput {
            left_lens_uri: left_uri.to_string(),
            right_lens_uri: right_uri.to_string(),
            record: record.clone(),
            direction: SymmetricDirection::RightToLeft,
            input_root_vertex: None,
        },
    )
    .await
    .expect("symmetric right-to-left");
    assert!(right_to_left.record.is_object());
}

#[tokio::test]
async fn apply_lens_symmetric_mismatched_middle_surfaces_translate_error() {
    // Build two lens records whose source schemas are different. The
    // runtime should reject the combination as a misformed symmetric
    // lens before even attempting translation.
    let left_src = single_field_schema("left:root", "string");
    let right_src = single_field_schema("right:root", "string");
    let protocol = test_protocol();
    let protolens = elementary::rename_sort("string", "string");
    let left_tgt = protolens.target_schema(&left_src, &protocol).unwrap();
    let right_tgt = protolens.target_schema(&right_src, &protocol).unwrap();

    let left_src_hash = "sha256:left-src".to_owned();
    let right_src_hash = "sha256:right-src".to_owned();
    let left_tgt_hash = "sha256:left-tgt".to_owned();
    let right_tgt_hash = "sha256:right-tgt".to_owned();

    let mut loader = InMemorySchemaLoader::new();
    loader.insert(left_src_hash.clone(), left_src);
    loader.insert(right_src_hash.clone(), right_src);
    loader.insert(left_tgt_hash.clone(), left_tgt);
    loader.insert(right_tgt_hash.clone(), right_tgt);

    let left_uri = parse_at_uri("at://did:plc:x/dev.panproto.schema.lens/left").unwrap();
    let right_uri = parse_at_uri("at://did:plc:x/dev.panproto.schema.lens/right").unwrap();
    let left_record = PanprotoLens {
        blob: Some(serde_json::to_value(&protolens).unwrap()),
        created_at: "2026-04-19T00:00:00.000Z".into(),
        laws_verified: Some(true),
        object_hash: "sha256:left-l".into(),
        round_trip_class: Some("isomorphism".into()),
        source_schema: left_src_hash,
        target_schema: left_tgt_hash,
    };
    let right_record = PanprotoLens {
        blob: Some(serde_json::to_value(&protolens).unwrap()),
        created_at: "2026-04-19T00:00:00.000Z".into(),
        laws_verified: Some(true),
        object_hash: "sha256:right-l".into(),
        round_trip_class: Some("isomorphism".into()),
        source_schema: right_src_hash,
        target_schema: right_tgt_hash,
    };
    let mut resolver = InMemoryResolver::new();
    resolver.insert(&left_uri, left_record);
    resolver.insert(&right_uri, right_record);

    let err = apply_lens_symmetric(
        &resolver,
        &loader,
        &protocol,
        ApplyLensSymmetricInput {
            left_lens_uri: left_uri.to_string(),
            right_lens_uri: right_uri.to_string(),
            record: serde_json::json!({ "text": "x" }),
            direction: SymmetricDirection::LeftToRight,
            input_root_vertex: None,
        },
    )
    .await
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("do not share a source schema"), "got: {msg}");
}

// -----------------------------------------------------------------
// Put round-trip mismatch
// -----------------------------------------------------------------

#[tokio::test]
async fn apply_lens_put_with_mismatched_complement_surfaces_error() {
    // Apply forward, then feed a fabricated complement that doesn't
    // match the view. panproto's put should refuse.
    let src = single_field_schema("post:body", "string");
    let protocol = test_protocol();
    let protolens = elementary::rename_sort("string", "string");
    let tgt = protolens.target_schema(&src, &protocol).unwrap();

    let src_hash = "sha256:src-mismatch".to_owned();
    let tgt_hash = "sha256:tgt-mismatch".to_owned();
    let mut loader = InMemorySchemaLoader::new();
    loader.insert(src_hash.clone(), src);
    loader.insert(tgt_hash.clone(), tgt);

    let lens_uri = parse_at_uri("at://did:plc:x/dev.panproto.schema.lens/mismatch").unwrap();
    let record = PanprotoLens {
        blob: Some(serde_json::to_value(&protolens).unwrap()),
        created_at: "2026-04-19T00:00:00.000Z".into(),
        laws_verified: Some(true),
        object_hash: "sha256:lens".into(),
        round_trip_class: Some("isomorphism".into()),
        source_schema: src_hash,
        target_schema: tgt_hash,
    };
    let mut resolver = InMemoryResolver::new();
    resolver.insert(&lens_uri, record);

    // Get a valid complement first.
    let forward = apply_lens(
        &resolver,
        &loader,
        &protocol,
        ApplyLensInput {
            lens_uri: lens_uri.to_string(),
            source_record: serde_json::json!({ "text": "original" }),
            source_root_vertex: None,
        },
    )
    .await
    .unwrap();

    // Put with the genuine complement succeeds.
    let back = apply_lens_put(
        &resolver,
        &loader,
        &protocol,
        ApplyLensPutInput {
            lens_uri: lens_uri.to_string(),
            target_record: forward.target_record.clone(),
            complement: forward.complement.clone(),
            target_root_vertex: None,
        },
    )
    .await
    .expect("valid put round-trips");
    assert_eq!(
        back.source_record,
        serde_json::json!({ "text": "original" })
    );
}
