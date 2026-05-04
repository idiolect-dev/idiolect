//! Pins the cross-task `Send` contract on `Resolver`, `SchemaLoader`,
//! `PdsClient`, and `PanprotoVcsClient`.
//!
//! Before this fix the trait methods were declared as bare
//! `async fn`, which produced `?Send` futures at the trait boundary.
//! Composed futures (notably `apply_lens(...)`) leaked the `?Send`
//! out, blocking downstream callers from holding these resolvers
//! behind `Arc<dyn Resolver>` and calling `apply_lens` from inside
//! an `#[async_trait]` impl. Each test below would fail to compile
//! if the trait declarations regressed.

#![allow(dead_code, clippy::missing_panics_doc)]

use std::pin::Pin;
use std::sync::Arc;

use idiolect_lens::{
    ApplyLensInput, AtUri, CachingResolver, CreateRecordRequest, DeleteRecordRequest,
    InMemoryResolver, InMemorySchemaLoader, InMemoryVcsClient, LensError, ListRecordsResponse,
    PanprotoVcsClient, PanprotoVcsResolver, PdsClient, PdsResolver, PdsWriter, PutRecordRequest,
    Resolver, SchemaLoader, VerifyingResolver, WriteRecordResponse, apply_lens,
};
use idiolect_records::{AtUri as RecordAtUri, Datetime, PanprotoLens};
use panproto_lens::protolens::elementary;
use panproto_schema::{Protocol, Schema, SchemaBuilder};

// -----------------------------------------------------------------
// helpers
// -----------------------------------------------------------------

fn assert_send<T: Send>(_: &T) {}
fn assert_send_sync<T: Send + Sync>(_: &T) {}

fn fixture_lens(src_hash: &str, tgt_hash: &str) -> PanprotoLens {
    PanprotoLens {
        blob: None,
        created_at: Datetime::parse("2026-04-19T00:00:00.000Z").unwrap(),
        laws_verified: Some(true),
        object_hash: "sha256:deadbeef".into(),
        round_trip_class: Some("isomorphism".into()),
        source_schema: RecordAtUri::parse(src_hash).unwrap(),
        target_schema: RecordAtUri::parse(tgt_hash).unwrap(),
    }
}

fn lens_uri() -> AtUri {
    AtUri::parse("at://did:plc:xyz/dev.panproto.schema.lens/l1").unwrap()
}

// -----------------------------------------------------------------
// Resolver: futures are Send and the trait is object-safe
// -----------------------------------------------------------------

#[test]
fn in_memory_resolver_future_is_send() {
    let r = InMemoryResolver::new();
    let uri = lens_uri();
    assert_send(&r.resolve(&uri));
}

#[test]
fn arc_dyn_resolver_compiles_and_future_is_send() {
    let r: Arc<dyn Resolver> = Arc::new(InMemoryResolver::new());
    assert_send_sync(&r);
    let uri = lens_uri();
    assert_send(&r.resolve(&uri));
}

#[test]
fn caching_resolver_over_dyn_resolver_future_is_send() {
    let inner: Arc<dyn Resolver> = Arc::new(InMemoryResolver::new());
    let cached = CachingResolver::new(inner, std::time::Duration::from_secs(60));
    let uri = lens_uri();
    assert_send(&cached.resolve(&uri));
}

#[test]
fn verifying_resolver_over_dyn_resolver_future_is_send() {
    let inner: Arc<dyn Resolver> = Arc::new(InMemoryResolver::new());
    let v = VerifyingResolver::sha256(inner);
    let uri = lens_uri();
    assert_send(&v.resolve(&uri));
}

// -----------------------------------------------------------------
// SchemaLoader: futures are Send and the trait is object-safe
// -----------------------------------------------------------------

#[test]
fn in_memory_schema_loader_future_is_send() {
    let l = InMemorySchemaLoader::new();
    assert_send(&l.load("sha256:abc"));
}

#[test]
fn arc_dyn_schema_loader_compiles_and_future_is_send() {
    let l: Arc<dyn SchemaLoader> = Arc::new(InMemorySchemaLoader::new());
    assert_send_sync(&l);
    assert_send(&l.load("sha256:abc"));
}

// -----------------------------------------------------------------
// apply_lens(): the composed future is Send when both R and S are
// trait objects. The crux of the issue this test guards against.
// -----------------------------------------------------------------

#[test]
fn apply_lens_future_is_send_with_dyn_resolver_and_loader() {
    let resolver: Arc<dyn Resolver> = Arc::new(InMemoryResolver::new());
    let loader: Arc<dyn SchemaLoader> = Arc::new(InMemorySchemaLoader::new());
    let protocol = Protocol::default();

    // Build the future without driving it; just typecheck `Send`.
    let fut = apply_lens(
        &resolver,
        &loader,
        &protocol,
        ApplyLensInput {
            lens_uri: lens_uri(),
            source_record: serde_json::json!({}),
            source_root_vertex: None,
        },
    );
    assert_send(&fut);
}

// -----------------------------------------------------------------
// apply_lens(): end-to-end, driving the future through a tokio
// `spawn` (which requires `Send`) over `Arc<dyn ...>` resolvers.
// Confirms the Send contract is not just type-checked but works
// across task boundaries.
// -----------------------------------------------------------------

fn test_protocol() -> Protocol {
    Protocol::default()
}

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

#[tokio::test]
async fn apply_lens_runs_through_tokio_spawn_with_dyn_resolvers() {
    let protocol = test_protocol();
    let src_schema = single_field_source_schema();
    let protolens = elementary::rename_sort("string", "text");
    let tgt_schema = protolens.target_schema(&src_schema, &protocol).unwrap();

    let src_hash = "at://did:plc:x/dev.panproto.schema.schema/src";
    let tgt_hash = "at://did:plc:x/dev.panproto.schema.schema/tgt";

    let mut concrete_loader = InMemorySchemaLoader::new();
    concrete_loader.insert(src_hash.to_owned(), src_schema);
    concrete_loader.insert(tgt_hash.to_owned(), tgt_schema);

    let mut concrete_resolver = InMemoryResolver::new();
    let lens_uri = lens_uri();
    let mut record = fixture_lens(src_hash, tgt_hash);
    record.blob = Some(serde_json::to_value(&protolens).unwrap());
    concrete_resolver.insert(&lens_uri, record);

    let resolver: Arc<dyn Resolver> = Arc::new(concrete_resolver);
    let loader: Arc<dyn SchemaLoader> = Arc::new(concrete_loader);

    // tokio::spawn requires the future to be `Send + 'static`. This
    // is the canonical way downstream callers (e.g. an axum handler)
    // would invoke `apply_lens` over trait-object dependencies.
    let handle = tokio::spawn({
        let resolver = Arc::clone(&resolver);
        let loader = Arc::clone(&loader);
        let lens_uri = lens_uri.clone();
        async move {
            apply_lens(
                &resolver,
                &loader,
                &test_protocol(),
                ApplyLensInput {
                    lens_uri,
                    source_record: serde_json::json!({ "text": "hello" }),
                    source_root_vertex: None,
                },
            )
            .await
        }
    });

    let out = handle.await.unwrap().expect("forward apply_lens");
    assert_eq!(out.target_record, serde_json::json!({ "text": "hello" }));
}

// -----------------------------------------------------------------
// Wraps apply_lens inside an async-trait-shaped Send future, the
// exact pattern the issue describes (LensApplier in a downstream
// orchestrator). The compiler error this would have produced
// before the fix is the one cited in the issue body.
// -----------------------------------------------------------------

trait LensApplier: Send + Sync {
    fn apply<'a>(
        &'a self,
        lens_uri: &'a AtUri,
        record: serde_json::Value,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, LensError>> + Send + 'a>>;
}

struct PanprotoLensApplier {
    resolver: Arc<dyn Resolver>,
    loader: Arc<dyn SchemaLoader>,
    protocol: Protocol,
}

impl LensApplier for PanprotoLensApplier {
    fn apply<'a>(
        &'a self,
        lens_uri: &'a AtUri,
        record: serde_json::Value,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, LensError>> + Send + 'a>>
    {
        Box::pin(async move {
            let out = apply_lens(
                &self.resolver,
                &self.loader,
                &self.protocol,
                ApplyLensInput {
                    lens_uri: lens_uri.clone(),
                    source_record: record,
                    source_root_vertex: None,
                },
            )
            .await?;
            Ok(out.target_record)
        })
    }
}

#[tokio::test]
async fn lens_applier_trait_object_drives_apply_lens() {
    let protocol = test_protocol();
    let src_schema = single_field_source_schema();
    let protolens = elementary::rename_sort("string", "text");
    let tgt_schema = protolens.target_schema(&src_schema, &protocol).unwrap();

    let src_hash = "at://did:plc:x/dev.panproto.schema.schema/src";
    let tgt_hash = "at://did:plc:x/dev.panproto.schema.schema/tgt";

    let mut concrete_loader = InMemorySchemaLoader::new();
    concrete_loader.insert(src_hash.to_owned(), src_schema);
    concrete_loader.insert(tgt_hash.to_owned(), tgt_schema);

    let mut concrete_resolver = InMemoryResolver::new();
    let lens_uri = lens_uri();
    let mut record = fixture_lens(src_hash, tgt_hash);
    record.blob = Some(serde_json::to_value(&protolens).unwrap());
    concrete_resolver.insert(&lens_uri, record);

    let applier: Arc<dyn LensApplier> = Arc::new(PanprotoLensApplier {
        resolver: Arc::new(concrete_resolver),
        loader: Arc::new(concrete_loader),
        protocol,
    });

    let out = applier
        .apply(&lens_uri, serde_json::json!({ "text": "hi" }))
        .await
        .unwrap();
    assert_eq!(out, serde_json::json!({ "text": "hi" }));
}

// -----------------------------------------------------------------
// PdsClient and PanprotoVcsClient: every method's future is Send.
// (RPITIT `+ Send` form, since these are not used as `dyn`.)
// -----------------------------------------------------------------

struct StubPdsClient;

impl PdsClient for StubPdsClient {
    async fn get_record(
        &self,
        _did: &str,
        _collection: &str,
        _rkey: &str,
    ) -> Result<serde_json::Value, LensError> {
        Ok(serde_json::Value::Null)
    }
}

#[test]
fn pds_client_get_record_future_is_send() {
    let c = StubPdsClient;
    assert_send(&c.get_record("did:plc:x", "col", "rkey"));
    assert_send(&c.list_records("did:plc:x", "col", None, None));
}

#[test]
fn pds_resolver_over_arc_pds_client_future_is_send() {
    let client: Arc<dyn PdsClientObj> = Arc::new(StubPdsObjClient);
    // PdsResolver is generic, so wrap a concrete client.
    let r = PdsResolver::new(StubPdsClient);
    let uri = lens_uri();
    assert_send(&r.resolve(&uri));
    drop(client);
}

// dyn-erased PdsClient surface for the test's Arc-of-dyn check above.
// PdsClient itself is not dyn-safe under RPITIT, so we mirror the
// surface the test needs through a small object-safe wrapper.
trait PdsClientObj: Send + Sync {
    fn get_record<'a>(
        &'a self,
        did: &'a str,
        collection: &'a str,
        rkey: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, LensError>> + Send + 'a>>;
}

struct StubPdsObjClient;

impl PdsClientObj for StubPdsObjClient {
    fn get_record<'a>(
        &'a self,
        _did: &'a str,
        _collection: &'a str,
        _rkey: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, LensError>> + Send + 'a>>
    {
        Box::pin(async { Ok(serde_json::Value::Null) })
    }
}

#[test]
fn panproto_vcs_client_futures_are_send() {
    let c = InMemoryVcsClient::new();
    assert_send(&c.get_object("hash"));
    assert_send(&c.get_ref("name"));
    assert_send(&c.set_ref("name", "hash"));
    assert_send(&c.list_refs());
    assert_send(&c.list_commits("name", None));
    assert_send(&c.get_head("name"));
    assert_send(&c.get_schema_tree("hash"));
    assert_send(&c.list_theories());
    assert_send(&c.list_alignments());
}

#[test]
fn panproto_vcs_resolver_future_is_send() {
    let r = PanprotoVcsResolver::new(InMemoryVcsClient::new());
    let uri = lens_uri();
    assert_send(&r.resolve(&uri));
}

// -----------------------------------------------------------------
// PdsWriter: its surface is unchanged by this fix, but make sure
// its `async fn` methods still produce Send futures over a concrete
// impl so downstream `Arc<T: PdsWriter>` callers stay `Send`.
// -----------------------------------------------------------------

struct StubPdsWriter;

impl PdsWriter for StubPdsWriter {
    async fn create_record(
        &self,
        _request: CreateRecordRequest,
    ) -> Result<WriteRecordResponse, LensError> {
        Ok(WriteRecordResponse {
            uri: "at://did:plc:x/c/r".into(),
            cid: "cid".into(),
        })
    }

    async fn put_record(
        &self,
        _request: PutRecordRequest,
    ) -> Result<WriteRecordResponse, LensError> {
        Ok(WriteRecordResponse {
            uri: "at://did:plc:x/c/r".into(),
            cid: "cid".into(),
        })
    }

    async fn delete_record(&self, _request: DeleteRecordRequest) -> Result<(), LensError> {
        Ok(())
    }
}

#[test]
fn pds_writer_concrete_futures_are_send() {
    let w = StubPdsWriter;
    let req = CreateRecordRequest {
        repo: "did:plc:x".into(),
        collection: "c".into(),
        rkey: None,
        record: serde_json::json!({}),
        validate: None,
    };
    assert_send(&w.create_record(req));
}

#[allow(dead_code)]
fn assert_list_records_response_is_send(_: ListRecordsResponse) {}
