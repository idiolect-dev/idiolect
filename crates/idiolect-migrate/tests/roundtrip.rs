//! End-to-end migration tests.

use idiolect_lens::{InMemoryResolver, InMemorySchemaLoader, parse_at_uri};
use idiolect_migrate::error::PlannerError;
use idiolect_migrate::{MigrateError, classify, migrate_record, plan_auto};
use idiolect_records::PanprotoLens;
use panproto_lens::protolens::elementary;
use panproto_schema::{Protocol, Schema, SchemaBuilder};

fn protocol() -> Protocol {
    Protocol::default()
}

fn schema(entry: &str, kind: &str) -> Schema {
    SchemaBuilder::new(&protocol())
        .entry(entry)
        .vertex(entry, "object", None)
        .unwrap()
        .vertex(&format!("{entry}.text"), kind, None)
        .unwrap()
        .edge(entry, &format!("{entry}.text"), "prop", Some("text"))
        .unwrap()
        .build()
        .unwrap()
}

#[tokio::test]
async fn migrate_record_translates_through_identity_lens() {
    let src = schema("body", "string");
    let protolens = elementary::rename_sort("string", "string");
    let tgt = protolens.target_schema(&src, &protocol()).unwrap();

    let src_hash = "sha256:src".to_owned();
    let tgt_hash = "sha256:tgt".to_owned();
    let mut loader = InMemorySchemaLoader::new();
    loader.insert(src_hash.clone(), src);
    loader.insert(tgt_hash.clone(), tgt);

    let uri = parse_at_uri("at://did:plc:x/dev.panproto.schema.lens/id").unwrap();
    let record = PanprotoLens {
        blob: Some(serde_json::to_value(&protolens).unwrap()),
        created_at: "2026-04-21T00:00:00.000Z".into(),
        laws_verified: Some(true),
        object_hash: "sha256:lens".into(),
        round_trip_class: Some("isomorphism".into()),
        source_schema: src_hash,
        target_schema: tgt_hash,
    };
    let mut resolver = InMemoryResolver::new();
    resolver.insert(&uri, record);

    let input = serde_json::json!({ "text": "hello" });
    let output = migrate_record(
        &resolver,
        &loader,
        &protocol(),
        &uri.to_string(),
        input.clone(),
    )
    .await
    .unwrap();
    // rename_sort with identical source/target kinds preserves the body.
    assert_eq!(output, input);
}

#[tokio::test]
async fn migrate_record_unresolvable_lens_surfaces_lens_error() {
    let loader = InMemorySchemaLoader::new();
    let resolver = InMemoryResolver::new();
    let err = migrate_record(
        &resolver,
        &loader,
        &protocol(),
        "at://did:plc:x/dev.panproto.schema.lens/absent",
        serde_json::json!({}),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MigrateError::Lens(_)));
}

#[test]
fn classify_reports_no_change_for_identical_schemas() {
    let s = schema("body", "string");
    let report = classify(&s, &s, &protocol());
    assert!(report.compatible);
}

#[test]
fn plan_auto_rejects_identical_schemas() {
    let s = schema("body", "string");
    let err = plan_auto(&s, &s, &protocol(), "h1", "h1").unwrap_err();
    assert!(matches!(err, PlannerError::NoChange));
}
