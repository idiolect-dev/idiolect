//! Lenses that carry value transforms, published and applied end to end.
//!
//! The fixtures are lens bodies as panproto 0.78.0 writes them
//! (`ProtolensChain.to_json` on a compiled lens document). `rule` rewrites
//! the record's `label`, `rule-behind-ref` rewrites the `label` of each
//! item of `tags`, and `two-groups` renames a field and then rewrites it,
//! which panproto does not fuse into one lens.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use idiolect_lens::{
    ApplyLensInput, ApplyLensOutput, ApplyLensPutInput, AtUri, CreateRecordRequest,
    DeleteRecordRequest, InMemorySchemaLoader, LensBody, LensError, PdsClient, PdsResolver,
    PdsWriter, PutRecordRequest, RecordPublisher, WriteRecordResponse, apply_lens, apply_lens_put,
};
use idiolect_records::PanprotoLens;
use panproto_lens::protolens::{ProtolensChain, combinators};
use panproto_protocols::web_document::atproto;
use panproto_schema::{Protocol, Schema};
use serde_json::{Value, json};

const DID: &str = "did:plc:aaaaaaaaaaaaaaaaaaaaaaaa";
const ROOT: &str = "com.example.note:body";
const SOURCE_SCHEMA: &str = "at://did:plc:aaaaaaaaaaaaaaaaaaaaaaaa/dev.panproto.schema.schema/src";
const TARGET_SCHEMA: &str = "at://did:plc:aaaaaaaaaaaaaaaaaaaaaaaa/dev.panproto.schema.schema/tgt";

/// A PDS held in memory, so that the lens record is written and read back
/// through the same boundaries a live one uses.
#[derive(Default)]
struct MemoryPds(Mutex<HashMap<String, Value>>);

impl PdsClient for MemoryPds {
    async fn get_record(
        &self,
        did: &str,
        collection: &str,
        rkey: &str,
    ) -> Result<Value, LensError> {
        let uri = format!("at://{did}/{collection}/{rkey}");
        let records = self.0.lock().expect("lock");
        records.get(&uri).cloned().ok_or(LensError::NotFound(uri))
    }
}

impl PdsWriter for MemoryPds {
    async fn create_record(
        &self,
        request: CreateRecordRequest,
    ) -> Result<WriteRecordResponse, LensError> {
        let rkey = request.rkey.unwrap_or_else(|| "self".to_owned());
        let uri = format!("at://{}/{}/{rkey}", request.repo, request.collection);
        self.0
            .lock()
            .expect("lock")
            .insert(uri.clone(), request.record);
        Ok(WriteRecordResponse {
            uri,
            cid: "bafyreigh2akiscaildcqabsyg3dfr6chu3fgpregiymsck7e7aqa4s52zy".to_owned(),
        })
    }

    async fn put_record(&self, _: PutRecordRequest) -> Result<WriteRecordResponse, LensError> {
        Err(LensError::Transport("put_record is not used".to_owned()))
    }

    async fn delete_record(&self, _: DeleteRecordRequest) -> Result<(), LensError> {
        Err(LensError::Transport("delete_record is not used".to_owned()))
    }
}

fn fixture(name: &str) -> Value {
    let path = format!(
        "{}/tests/fixtures/value_transforms/{name}.json",
        env!("CARGO_MANIFEST_DIR"),
    );
    serde_json::from_str(&std::fs::read_to_string(path).expect("read fixture")).expect("json")
}

fn schema() -> Schema {
    atproto::parse_lexicon(&fixture("note.lexicon")).expect("parse lexicon")
}

fn record() -> Value {
    json!({
        "text": "t",
        "label": "old",
        "author": { "name": "a" },
        "tags": [{ "label": "old", "rank": 1 }, { "label": "x", "rank": 2 }],
    })
}

/// A published lens and what is needed to run it.
struct Published {
    uri: AtUri,
    resolver: PdsResolver<Arc<MemoryPds>>,
    loader: InMemorySchemaLoader,
    protocol: Protocol,
}

impl Published {
    async fn new(blob: Value, target: Schema) -> Self {
        let mut loader = InMemorySchemaLoader::new();
        loader.insert(SOURCE_SCHEMA.to_owned(), schema());
        loader.insert(TARGET_SCHEMA.to_owned(), target);
        let pds = Arc::new(MemoryPds::default());
        let lens = PanprotoLens {
            blob: Some(blob),
            created_at: idiolect_records::Datetime::parse("2026-11-25T00:00:00.000Z")
                .expect("datetime"),
            laws_verified: None,
            object_hash: "sha256:00".to_owned(),
            round_trip_class: None,
            source_schema: idiolect_records::AtUri::parse(SOURCE_SCHEMA).expect("uri"),
            target_schema: idiolect_records::AtUri::parse(TARGET_SCHEMA).expect("uri"),
        };
        let written = RecordPublisher::new(Arc::clone(&pds), DID.to_owned())
            .create_with_rkey(&lens, Some("lens".to_owned()), None)
            .await
            .expect("publish");
        Self {
            uri: AtUri::parse(&written.uri).expect("uri"),
            resolver: PdsResolver::new(pds),
            loader,
            protocol: atproto::protocol(),
        }
    }

    async fn get(&self, source: Value) -> Result<ApplyLensOutput, LensError> {
        apply_lens(
            &self.resolver,
            &self.loader,
            &self.protocol,
            ApplyLensInput {
                lens_uri: self.uri.clone(),
                source_record: source,
                source_root_vertex: Some(ROOT.to_owned()),
            },
        )
        .await
    }

    async fn put(&self, output: ApplyLensOutput) -> Result<Value, LensError> {
        apply_lens_put(
            &self.resolver,
            &self.loader,
            &self.protocol,
            ApplyLensPutInput {
                lens_uri: self.uri.clone(),
                target_record: output.target_record,
                complement: output.complement,
                target_root_vertex: Some(ROOT.to_owned()),
            },
        )
        .await
        .map(|out| out.source_record)
    }
}

async fn published_chain(chain: &ProtolensChain) -> Published {
    let lens = chain
        .instantiate(&schema(), &atproto::protocol())
        .expect("instantiate");
    let blob = serde_json::to_value(chain).expect("serialize chain");
    Published::new(blob, lens.tgt_schema).await
}

#[tokio::test]
async fn a_value_rule_is_applied_and_put_back() {
    let published = Published::new(fixture("rule"), schema()).await;
    let out = published.get(record()).await.expect("get");
    let mut expected = record();
    expected["label"] = json!("new");
    assert_eq!(out.target_record, expected);
    assert_eq!(published.put(out).await.expect("put"), record());
}

#[tokio::test]
async fn a_value_rule_reaches_a_referenced_definition() {
    let published = Published::new(fixture("rule-behind-ref"), schema()).await;
    let out = published.get(record()).await.expect("get");
    let mut expected = record();
    expected["tags"][0]["label"] = json!("new");
    assert_eq!(out.target_record, expected);
    assert_eq!(published.put(out).await.expect("put"), record());
}

#[tokio::test]
async fn an_edit_to_the_view_is_carried_back() {
    let published = Published::new(fixture("rule"), schema()).await;
    let mut out = published.get(record()).await.expect("get");
    out.target_record["text"] = json!("edited");
    let mut expected = record();
    expected["text"] = json!("edited");
    assert_eq!(published.put(out).await.expect("put"), expected);
}

#[tokio::test]
async fn flat_transforms_without_stages_are_one_stage() {
    let mut body = fixture("rule");
    body.as_object_mut().expect("object").remove("stages");
    let published = Published::new(body, schema()).await;
    let out = published.get(record()).await.expect("get");
    assert_eq!(out.target_record["label"], "new");
}

#[tokio::test]
async fn a_compiled_lens_artifact_is_read() {
    let rule = fixture("rule");
    let artifact = json!({
        "format": "panproto-compiled-lens-v1",
        "chain": { "steps": rule["steps"] },
        "field_transforms": rule["field_transforms"],
        "stages": rule["stages"],
        "symmetric": null,
    });
    let published = Published::new(artifact, schema()).await;
    let out = published.get(record()).await.expect("get");
    assert_eq!(out.target_record["label"], "new");
}

#[tokio::test]
async fn stages_that_do_not_fuse_are_refused() {
    let published = Published::new(fixture("two-groups"), schema()).await;
    let error = published.get(record()).await.expect_err("refused");
    assert!(matches!(error, LensError::Instantiate(_)), "{error}");
}

#[test]
fn a_body_that_cannot_be_represented_is_refused() {
    for body in [
        json!({ "format": "panproto-compiled-lens-v2", "steps": [] }),
        json!({ "steps": [], "symmetric": { "left": { "steps": [] }, "right": { "steps": [] } } }),
        json!({ "steps": [], "field_transforms": { "v": [{ "NoSuchTransform": {} }] } }),
        json!({ "unrelated": true }),
    ] {
        assert!(
            serde_json::from_value::<LensBody>(body.clone()).is_err(),
            "{body}"
        );
    }
}

#[test]
fn a_staged_body_survives_serialization() {
    let body: LensBody = serde_json::from_value(fixture("rule")).expect("decode");
    let again: LensBody =
        serde_json::from_value(serde_json::to_value(&body).expect("encode")).expect("decode");
    let LensBody::Staged(staged) = again else {
        panic!("expected a staged body");
    };
    assert_eq!(staged.stages.len(), 1);
    assert_eq!(staged.stages[0].field_transforms.len(), 1);
}

/// Reading the view from json numbers its nodes in key order, which a
/// rename changes; the fields must still go back to their own values.
#[tokio::test]
async fn put_restores_fields_whose_key_order_a_rename_changes() {
    for (old, new) in [("label", "zLabel"), ("text", "aText"), ("tags", "aTags")] {
        let chain = combinators::rename_field(ROOT, format!("{ROOT}.{old}"), old, new);
        for _ in 0..16 {
            let published = published_chain(&chain).await;
            let out = published.get(record()).await.expect("get");
            assert!(out.target_record.get(old).is_none(), "{old}");
            assert_eq!(published.put(out).await.expect("put"), record(), "{old}");
        }
    }
}

#[tokio::test]
async fn put_restores_a_field_the_lens_drops() {
    let chain = combinators::remove_field(format!("{ROOT}.label"));
    let published = published_chain(&chain).await;
    let out = published.get(record()).await.expect("get");
    assert!(out.target_record.get("label").is_none());
    assert_eq!(published.put(out).await.expect("put"), record());
}
