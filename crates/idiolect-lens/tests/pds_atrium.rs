//! Integration test for the atrium-backed [`PdsClient`].
//!
//! Stands up a [`wiremock`] server, serves a
//! `com.atproto.repo.getRecord` response whose `value` is a
//! `dev.panproto.schema.lens` record, and pipes it through
//! [`AtriumPdsClient`] → [`PdsResolver`] → [`apply_lens`] against an
//! in-memory schema loader. Verifies:
//!
//! 1. The happy path returns the translated record (a `rename_sort`
//!    protolens is lossless, so the view equals the source json).
//! 2. A lexicon-shaped `RecordNotFound` XRPC error is mapped to
//!    [`LensError::NotFound`], distinguishable from generic
//!    transport failures.
//!
//! The test target is gated behind `required-features = ["pds-atrium"]`
//! in Cargo.toml so cargo only tries to build it when the feature is
//! on.

use idiolect_lens::{
    ApplyLensInput, AtriumPdsClient, InMemorySchemaLoader, LensError, PdsResolver, apply_lens,
    parse_at_uri,
};
use idiolect_records::PanprotoLens;
use panproto_lens::protolens::elementary;
use panproto_schema::{Protocol, Schema, SchemaBuilder};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn test_protocol() -> Protocol {
    Protocol::default()
}

fn fixture_schema() -> Schema {
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

/// Build a wiremock server and install a `getRecord` stub whose
/// `value` field carries the given lens record body.
async fn stub_get_record(
    did: &str,
    collection: &str,
    rkey: &str,
    lens_value: serde_json::Value,
) -> MockServer {
    let server = MockServer::start().await;

    // omit `cid` entirely: atrium validates it as a real CID when
    // present, and the field is optional in the lexicon. a fake cid
    // would trip multihash decoding on the client side.
    let body = json!({
        "uri": format!("at://{did}/{collection}/{rkey}"),
        "value": lens_value,
    });

    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.getRecord"))
        .and(query_param("repo", did))
        .and(query_param("collection", collection))
        .and(query_param("rkey", rkey))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;

    server
}

#[tokio::test(flavor = "current_thread")]
async fn apply_lens_reads_lens_record_from_mock_pds() {
    let protocol = test_protocol();
    let src_schema = fixture_schema();
    let protolens = elementary::rename_sort("string", "text");
    let tgt_schema = protolens
        .target_schema(&src_schema, &protocol)
        .expect("derive target schema");

    let src_hash = "sha256:src".to_owned();
    let tgt_hash = "sha256:tgt".to_owned();

    let mut loader = InMemorySchemaLoader::new();
    loader.insert(src_hash.clone(), src_schema.clone());
    loader.insert(tgt_hash.clone(), tgt_schema);

    // the lens record body the PDS will return. includes $type
    // because that is what a real PDS serves back on any record fetch.
    let lens_record_value = json!({
        "$type": "dev.panproto.schema.lens",
        "blob": serde_json::to_value(&protolens).unwrap(),
        "createdAt": "2026-04-19T00:00:00.000Z",
        "objectHash": "sha256:deadbeef",
        "sourceSchema": src_hash,
        "targetSchema": tgt_hash,
    });

    let did = "did:plc:xyz";
    let collection = "dev.panproto.schema.lens";
    let rkey = "l1";

    let server = stub_get_record(did, collection, rkey, lens_record_value).await;

    let client = AtriumPdsClient::with_service_url(server.uri());
    let resolver = PdsResolver::new(client);

    let source_record = json!({ "text": "hello, world" });
    let lens_uri = parse_at_uri(&format!("at://{did}/{collection}/{rkey}")).unwrap();

    let out = apply_lens(
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
    .expect("apply_lens via mock pds");

    // rename_sort is lossless; the view shape matches the source.
    assert_eq!(out.target_record, source_record);
}

#[tokio::test(flavor = "current_thread")]
async fn record_not_found_is_mapped_to_not_found_error() {
    // build a PanprotoLens we don't actually serve — just need something
    // to dereference to decide which variant we hit.
    let did = "did:plc:absent";
    let collection = "dev.panproto.schema.lens";
    let rkey = "missing";

    let server = MockServer::start().await;
    let body = json!({
        "error": "RecordNotFound",
        "message": "Could not locate record",
    });
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.getRecord"))
        .respond_with(ResponseTemplate::new(400).set_body_json(body))
        .mount(&server)
        .await;

    let client = AtriumPdsClient::with_service_url(server.uri());
    let err = idiolect_lens::Resolver::resolve(
        &PdsResolver::new(client),
        &parse_at_uri(&format!("at://{did}/{collection}/{rkey}")).unwrap(),
    )
    .await
    .expect_err("missing record should be an error");

    assert!(
        matches!(err, LensError::NotFound(_)),
        "expected LensError::NotFound, got {err:?}"
    );
}

// unused-import guard: `PanprotoLens` is pulled in for the doc link
// on the module-level rustdoc above; keep a direct use so future
// changes to the test keep the import tree consistent with the crate.
#[allow(dead_code)]
fn _keep_imports_warm(_: PanprotoLens) {}
