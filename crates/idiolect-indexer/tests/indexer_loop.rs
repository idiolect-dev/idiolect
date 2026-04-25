//! End-to-end orchestrator tests for `drive_indexer`.
//!
//! These tests stand up an `InMemoryEventStream` preloaded with real
//! `dev.idiolect.*` fixtures from `idiolect_records::generated::examples`,
//! drive it through `drive_indexer` with a counting handler and an
//! in-memory cursor store, and assert:
//!
//! - every fixture decodes into the correct `AnyRecord` variant
//! - delete events bypass decode and still advance the cursor
//! - the cursor advances only on `live = true` events
//! - events whose collection falls outside the configured prefix are
//!   silently dropped by the filter
//! - create / update events without a body raise `MissingBody` so a
//!   misbehaving upstream is surfaced loudly.

use idiolect_indexer::{
    CursorStore, InMemoryCursorStore, InMemoryEventStream, IndexerAction, IndexerConfig,
    IndexerError, NoopRecordHandler, RawEvent, RecordHandler, drive_indexer,
};
use idiolect_records::{AnyRecord, generated::examples};

/// Build a create event for a `dev.idiolect.*` fixture.
///
/// `seq` and `live` are test-specified; the rest is synthetic
/// metadata that keeps the event realistic without pulling in
/// atproto signing machinery.
fn create_event(seq: u64, live: bool, collection: &str, body_json: &str) -> RawEvent {
    RawEvent {
        seq,
        live,
        did: "did:plc:alice".to_owned(),
        rev: format!("rev-{seq}"),
        collection: idiolect_records::Nsid::parse(collection).expect("valid nsid"),
        rkey: format!("rkey-{seq}"),
        action: IndexerAction::Create,
        cid: Some(format!("bafy-{seq}")),
        body: Some(serde_json::from_str(body_json).expect("fixture json parses")),
    }
}

/// Build a delete event. No body; no cid.
fn delete_event(seq: u64, live: bool, collection: &str) -> RawEvent {
    RawEvent {
        seq,
        live,
        did: "did:plc:alice".to_owned(),
        rev: format!("rev-{seq}"),
        collection: idiolect_records::Nsid::parse(collection).expect("valid nsid"),
        rkey: format!("rkey-{seq}"),
        action: IndexerAction::Delete,
        cid: None,
        body: None,
    }
}

/// A handler that records the nsids of every event it sees, so the
/// test can assert the right `AnyRecord` variant was produced.
#[derive(Debug, Default)]
struct RecordingHandler {
    /// Nsid of every observed record; `"<delete>"` for delete events.
    observed: std::sync::Mutex<Vec<String>>,
}

impl RecordingHandler {
    fn new() -> Self {
        Self::default()
    }

    fn nsids(&self) -> Vec<String> {
        self.observed
            .lock()
            .expect("observed mutex poisoned")
            .clone()
    }
}

impl RecordHandler for RecordingHandler {
    async fn handle(&self, event: &idiolect_indexer::IndexerEvent) -> Result<(), IndexerError> {
        let label = event
            .record
            .as_ref()
            .map_or_else(|| "<delete>".to_owned(), |record| record.nsid_str().to_owned());

        self.observed
            .lock()
            .expect("observed mutex poisoned")
            .push(label);

        Ok(())
    }
}

#[tokio::test(flavor = "current_thread")]
async fn drive_indexer_decodes_every_record_kind() {
    // one fixture per dev.idiolect.* record type, interleaved with a
    // delete to exercise the delete branch too.
    let mut stream = InMemoryEventStream::new();
    stream.push(create_event(
        1,
        true,
        "dev.idiolect.adapter",
        examples::ADAPTER_JSON,
    ));
    stream.push(create_event(
        2,
        true,
        "dev.idiolect.bounty",
        examples::BOUNTY_JSON,
    ));
    stream.push(create_event(
        3,
        true,
        "dev.idiolect.community",
        examples::COMMUNITY_JSON,
    ));
    stream.push(create_event(
        4,
        true,
        "dev.idiolect.correction",
        examples::CORRECTION_JSON,
    ));
    stream.push(create_event(
        5,
        true,
        "dev.idiolect.dialect",
        examples::DIALECT_JSON,
    ));
    stream.push(create_event(
        6,
        true,
        "dev.idiolect.encounter",
        examples::ENCOUNTER_JSON,
    ));
    stream.push(create_event(
        7,
        true,
        "dev.idiolect.observation",
        examples::OBSERVATION_JSON,
    ));
    stream.push(create_event(
        8,
        true,
        "dev.idiolect.recommendation",
        examples::RECOMMENDATION_JSON,
    ));
    stream.push(create_event(
        9,
        true,
        "dev.idiolect.retrospection",
        examples::RETROSPECTION_JSON,
    ));
    stream.push(create_event(
        10,
        true,
        "dev.idiolect.verification",
        examples::VERIFICATION_JSON,
    ));
    stream.push(delete_event(11, true, "dev.idiolect.encounter"));

    let handler = RecordingHandler::new();
    let cursors = InMemoryCursorStore::new();
    let cfg = IndexerConfig::default();

    drive_indexer(&mut stream, &handler, &cursors, &cfg)
        .await
        .expect("indexer drains stream");

    let nsids = handler.nsids();
    assert_eq!(nsids.len(), 11);
    assert!(nsids.contains(&"dev.idiolect.adapter".to_owned()));
    assert!(nsids.contains(&"dev.idiolect.bounty".to_owned()));
    assert!(nsids.contains(&"dev.idiolect.community".to_owned()));
    assert!(nsids.contains(&"dev.idiolect.correction".to_owned()));
    assert!(nsids.contains(&"dev.idiolect.dialect".to_owned()));
    assert!(nsids.contains(&"dev.idiolect.encounter".to_owned()));
    assert!(nsids.contains(&"dev.idiolect.observation".to_owned()));
    assert!(nsids.contains(&"dev.idiolect.recommendation".to_owned()));
    assert!(nsids.contains(&"dev.idiolect.retrospection".to_owned()));
    assert!(nsids.contains(&"dev.idiolect.verification".to_owned()));
    assert_eq!(nsids.iter().filter(|s| s.as_str() == "<delete>").count(), 1);

    // cursor advanced to the last live event.
    let last = cursors
        .load(&cfg.subscription_id)
        .await
        .expect("cursor load succeeds");
    assert_eq!(last, Some(11));
}

#[tokio::test(flavor = "current_thread")]
async fn drive_indexer_skips_cursor_commit_on_backfill() {
    // mix of backfill (live=false) and live events; cursor should
    // pin to the last *live* seq, not the last event seen.
    let mut stream = InMemoryEventStream::new();
    stream.push(create_event(
        100,
        false,
        "dev.idiolect.encounter",
        examples::ENCOUNTER_JSON,
    ));
    stream.push(create_event(
        101,
        false,
        "dev.idiolect.encounter",
        examples::ENCOUNTER_JSON,
    ));
    stream.push(create_event(
        102,
        true,
        "dev.idiolect.encounter",
        examples::ENCOUNTER_JSON,
    ));
    stream.push(create_event(
        103,
        false,
        "dev.idiolect.encounter",
        examples::ENCOUNTER_JSON,
    ));

    let handler = NoopRecordHandler::new();
    let cursors = InMemoryCursorStore::new();
    let cfg = IndexerConfig::default();

    drive_indexer(&mut stream, &handler, &cursors, &cfg)
        .await
        .expect("indexer drains stream");

    // all four events observed.
    assert_eq!(handler.observed(), 4);
    assert_eq!(handler.deletes(), 0);

    // only the one live event advanced the cursor.
    let last = cursors
        .load(&cfg.subscription_id)
        .await
        .expect("cursor load succeeds");
    assert_eq!(last, Some(102));
}

#[tokio::test(flavor = "current_thread")]
async fn drive_indexer_filters_out_of_prefix_collections() {
    // events outside the configured nsid prefix are silently dropped
    // at the prefix-filter stage, before decode. the handler should
    // never see them.
    let mut stream = InMemoryEventStream::new();
    stream.push(create_event(
        1,
        true,
        "app.bsky.feed.post",
        "{\"text\":\"hello\"}",
    ));
    stream.push(create_event(
        2,
        true,
        "dev.idiolect.encounter",
        examples::ENCOUNTER_JSON,
    ));
    stream.push(create_event(
        3,
        true,
        "com.atproto.server.somethingElse",
        "{\"foo\":\"bar\"}",
    ));

    let handler = NoopRecordHandler::new();
    let cursors = InMemoryCursorStore::new();
    let cfg = IndexerConfig::default();

    drive_indexer(&mut stream, &handler, &cursors, &cfg)
        .await
        .expect("indexer drains stream");

    // only the dev.idiolect.encounter event made it through.
    assert_eq!(handler.observed(), 1);

    // cursor pinned to the one in-prefix event.
    let last = cursors
        .load(&cfg.subscription_id)
        .await
        .expect("cursor load succeeds");
    assert_eq!(last, Some(2));
}

#[tokio::test(flavor = "current_thread")]
async fn drive_indexer_surfaces_missing_body_on_create_without_body() {
    // a create action with body=None is a malformed upstream. the
    // indexer halts with MissingBody rather than silently skipping,
    // so the operator can investigate.
    let mut stream = InMemoryEventStream::new();
    stream.push(RawEvent {
        seq: 1,
        live: true,
        did: "did:plc:alice".to_owned(),
        rev: "rev-1".to_owned(),
        collection: idiolect_records::Nsid::parse("dev.idiolect.encounter").expect("nsid"),
        rkey: "rkey-1".to_owned(),
        action: IndexerAction::Create,
        cid: Some("bafy-1".to_owned()),
        body: None,
    });

    let handler = NoopRecordHandler::new();
    let cursors = InMemoryCursorStore::new();
    let cfg = IndexerConfig::default();

    let err = drive_indexer(&mut stream, &handler, &cursors, &cfg)
        .await
        .expect_err("indexer raises MissingBody");

    match err {
        IndexerError::MissingBody(msg) => {
            assert!(msg.contains("dev.idiolect.encounter"));
            assert!(msg.contains("rkey-1"));
        }
        other => panic!("expected MissingBody, got {other:?}"),
    }

    // handler never saw the malformed event.
    assert_eq!(handler.observed(), 0);
    // cursor did not advance.
    let last = cursors
        .load(&cfg.subscription_id)
        .await
        .expect("cursor load succeeds");
    assert_eq!(last, None);
}

#[tokio::test(flavor = "current_thread")]
async fn drive_indexer_preserves_decoded_record_type() {
    // spot-check that decode_record actually produces the right
    // AnyRecord variant — drive a single observation through and
    // inspect the decoded record.
    struct AssertingHandler;

    impl RecordHandler for AssertingHandler {
        async fn handle(&self, event: &idiolect_indexer::IndexerEvent) -> Result<(), IndexerError> {
            let record = event
                .record
                .as_ref()
                .expect("observation event carries a record");

            match record {
                AnyRecord::Observation(obs) => {
                    assert_eq!(obs.observer, "did:plc:observer");
                    assert_eq!(obs.version, "1.0.0");
                    assert_eq!(obs.method.name, "weighted-correction-rate");
                }
                other => panic!("expected Observation variant, got {:?}", other.nsid()),
            }

            Ok(())
        }
    }

    let mut stream = InMemoryEventStream::new();
    stream.push(create_event(
        42,
        true,
        "dev.idiolect.observation",
        examples::OBSERVATION_JSON,
    ));

    let handler = AssertingHandler;
    let cursors = InMemoryCursorStore::new();
    let cfg = IndexerConfig::default();

    drive_indexer(&mut stream, &handler, &cursors, &cfg)
        .await
        .expect("indexer processes observation fixture");
}
