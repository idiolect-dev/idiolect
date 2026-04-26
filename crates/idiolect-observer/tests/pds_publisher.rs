//! Integration test for [`PdsPublisher`].
//!
//! The test uses a hand-rolled [`PdsWriter`] fake rather than
//! wiremock-backed atrium client because the observer crate does not
//! depend on atrium directly (the `idiolect-lens/pds-atrium` feature
//! is opt-in and covered by its own integration test). The fake
//! captures every outgoing [`CreateRecordRequest`] so the assertions
//! can inspect what the publisher tried to write, without tying the
//! observer test to an HTTP transport.

use std::sync::Mutex;

use idiolect_indexer::{
    InMemoryCursorStore, InMemoryEventStream, IndexerAction, IndexerConfig, RawEvent,
};
use idiolect_lens::{
    CreateRecordRequest, DeleteRecordRequest, LensError, PdsWriter, PutRecordRequest,
    WriteRecordResponse,
};
use idiolect_observer::{
    CorrectionRateMethod, FlushSchedule, OBSERVATION_NSID, ObserverConfig, ObserverHandler,
    PdsPublisher, drive_observer,
};

/// In-memory [`PdsWriter`] fake.
///
/// Records every call, returns the preconfigured response for create
/// (a fixed at-uri + cid), and panics on put/delete so drift in the
/// publisher's code path shows up immediately as a test failure
/// instead of an ignored call.
#[derive(Debug, Default)]
struct RecordingWriter {
    created: Mutex<Vec<CreateRecordRequest>>,
}

impl PdsWriter for RecordingWriter {
    async fn create_record(
        &self,
        request: CreateRecordRequest,
    ) -> Result<WriteRecordResponse, LensError> {
        let uri = format!(
            "at://{}/{}/3l{}",
            request.repo,
            request.collection,
            self.created.lock().unwrap().len()
        );
        self.created.lock().unwrap().push(request);

        Ok(WriteRecordResponse {
            uri,
            cid: "bafyreicidplaceholder".to_owned(),
        })
    }

    async fn put_record(
        &self,
        _request: PutRecordRequest,
    ) -> Result<WriteRecordResponse, LensError> {
        panic!("PdsPublisher should not call put_record")
    }

    async fn delete_record(&self, _request: DeleteRecordRequest) -> Result<(), LensError> {
        panic!("PdsPublisher should not call delete_record")
    }
}

fn fixture_encounter_body() -> serde_json::Value {
    serde_json::json!({
        "kind": "invocation-log",
        "lens": {
            "uri": "at://did:plc:lens-author/dev.panproto.schema.lens/l1"
        },
        "occurredAt": "2026-04-20T10:00:00Z",
        "use": { "action": "test-encounter" },
        "sourceSchema": {
            "uri": "at://did:plc:lens-author/dev.panproto.schema.schema/s1"
        },
        "targetSchema": {
            "uri": "at://did:plc:lens-author/dev.panproto.schema.schema/s2"
        },
        "visibility": "public-detailed"
    })
}

fn make_event(seq: u64, collection: &str, rkey: &str, body: serde_json::Value) -> RawEvent {
    RawEvent {
        seq,
        live: true,
        did: "did:plc:alice".to_owned(),
        rev: format!("3l{seq}"),
        collection: idiolect_records::Nsid::parse(collection).expect("valid nsid"),
        rkey: rkey.to_owned(),
        action: IndexerAction::Create,
        cid: Some(format!("bafyrei{seq}")),
        body: Some(body),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn pds_publisher_writes_observation_with_type_tag() {
    // seed the firehose with a single encounter; the rate method will
    // see it and snapshot a non-empty observation on flush.
    let mut stream = InMemoryEventStream::new();
    stream.push(make_event(
        1,
        "dev.idiolect.encounter",
        "3l5",
        fixture_encounter_body(),
    ));

    let method = CorrectionRateMethod::new();
    let observer_did = "did:plc:observer".to_owned();
    let writer = RecordingWriter::default();
    let publisher = PdsPublisher::new(writer, observer_did.clone());
    let cfg = ObserverConfig {
        observer_did: observer_did.clone(),
        ..ObserverConfig::default()
    };

    let handler = ObserverHandler::new(method, publisher, cfg)
        .with_clock(|| "2026-04-20T12:00:00Z".to_owned());

    let cursors = InMemoryCursorStore::new();
    let ix = IndexerConfig::default();

    drive_observer(
        &mut stream,
        &handler,
        &cursors,
        &ix,
        FlushSchedule::EveryEvents(1),
    )
    .await
    .expect("drive_observer");

    // EveryEvents(1) flushes once per event plus a terminal flush when
    // the stream closes. the aggregator state is non-empty from event
    // 1 onward, so both flushes should hit the publisher. snapshot the
    // last request out from under the mutex so we can assert against a
    // plain value and the guard drops before the asserts run.
    let last = {
        let writer = handler.publisher().writer();
        let created = writer.created.lock().unwrap();
        assert!(
            !created.is_empty(),
            "expected at least one create_record call"
        );
        created.last().cloned().unwrap()
    };

    assert_eq!(last.repo, observer_did);
    assert_eq!(last.collection, OBSERVATION_NSID);
    assert!(
        last.rkey.is_none(),
        "publisher should let the PDS assign rkey"
    );

    // the record body must carry $type so a downstream reader parses
    // it as a lexicon-typed observation.
    let body = &last.record;
    assert_eq!(
        body.get("$type").and_then(|v| v.as_str()),
        Some(OBSERVATION_NSID)
    );
    assert_eq!(
        body.get("observer").and_then(|v| v.as_str()),
        Some(observer_did.as_str())
    );
    assert_eq!(
        body.get("occurredAt").and_then(|v| v.as_str()),
        Some("2026-04-20T12:00:00Z")
    );
    // method descriptor survives the serialize round-trip.
    assert_eq!(
        body.pointer("/method/name").and_then(|v| v.as_str()),
        Some("correction-rate")
    );
    // output carries the encounter's lens under the `lenses` map.
    assert!(
        body.pointer("/output/lenses/at:~1~1did:plc:lens-author~1dev.panproto.schema.lens~1l1")
            .is_some(),
        "expected lens bucket in output.lenses, got body: {body}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn pds_publisher_skips_empty_snapshots() {
    // no events → snapshot is None → handler.flush returns Ok(None) →
    // publisher is never called. verifies the publisher does not write
    // a no-op record on every flush tick.
    let mut stream = InMemoryEventStream::new();

    let method = CorrectionRateMethod::new();
    let observer_did = "did:plc:observer".to_owned();
    let writer = RecordingWriter::default();
    let publisher = PdsPublisher::new(writer, observer_did.clone());
    let cfg = ObserverConfig {
        observer_did,
        ..ObserverConfig::default()
    };
    let handler = ObserverHandler::new(method, publisher, cfg);

    let cursors = InMemoryCursorStore::new();
    let ix = IndexerConfig::default();

    drive_observer(
        &mut stream,
        &handler,
        &cursors,
        &ix,
        FlushSchedule::EveryEvents(1),
    )
    .await
    .expect("drive_observer");

    assert!(
        handler
            .publisher()
            .writer()
            .created
            .lock()
            .unwrap()
            .is_empty(),
        "empty aggregator state should not produce any PDS writes"
    );
}
