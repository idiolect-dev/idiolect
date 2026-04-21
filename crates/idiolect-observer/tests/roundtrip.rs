//! End-to-end observer pipeline tests.
//!
//! Drive the [`drive_observer`] orchestrator against an in-memory
//! firehose stream and verify that:
//!
//! - The [`CorrectionRateMethod`] aggregator receives one encounter
//!   and one correction.
//! - The [`InMemoryPublisher`] accumulates one observation record
//!   with the expected method, version, observer did, scope, and
//!   output shape.
//! - The `EveryEvents(1)` schedule fires a flush per event, and a
//!   terminal flush after the stream closes.

use idiolect_indexer::{
    InMemoryCursorStore, InMemoryEventStream, IndexerAction, IndexerConfig, RawEvent,
};
use idiolect_observer::{
    CorrectionRateMethod, FlushSchedule, InMemoryPublisher, ObserverConfig, ObserverHandler,
    drive_observer,
};

fn fixture_encounter_body() -> serde_json::Value {
    serde_json::json!({
        "kind": "invocation-log",
        "lens": {
            "uri": "at://did:plc:lens-author/dev.panproto.schema.lens/l1"
        },
        "occurredAt": "2026-04-20T10:00:00Z",
        "purpose": "test-encounter",
        "sourceSchema": {
            "uri": "at://did:plc:lens-author/dev.panproto.schema.schema/s1"
        },
        "targetSchema": {
            "uri": "at://did:plc:lens-author/dev.panproto.schema.schema/s2"
        },
        "visibility": "public-detailed"
    })
}

fn fixture_correction_body() -> serde_json::Value {
    serde_json::json!({
        "encounter": {
            "uri": "at://did:plc:alice/dev.idiolect.encounter/3l5"
        },
        "occurredAt": "2026-04-20T10:05:00Z",
        "path": "/text",
        "reason": "lens-error",
        "visibility": "public-detailed"
    })
}

fn make_event(seq: u64, collection: &str, rkey: &str, body: serde_json::Value) -> RawEvent {
    RawEvent {
        seq,
        live: true,
        did: "did:plc:alice".to_owned(),
        rev: format!("3l{seq}"),
        collection: collection.to_owned(),
        rkey: rkey.to_owned(),
        action: IndexerAction::Create,
        cid: Some(format!("bafyrei{seq}")),
        body: Some(body),
    }
}

#[tokio::test]
async fn observer_publishes_observation_after_every_event() {
    // seed the in-memory firehose with an encounter and a correction.
    let mut stream = InMemoryEventStream::new();
    stream.push(make_event(
        1,
        "dev.idiolect.encounter",
        "3l5",
        fixture_encounter_body(),
    ));
    stream.push(make_event(
        2,
        "dev.idiolect.correction",
        "3l6",
        fixture_correction_body(),
    ));

    // method + publisher + clock override so the observation's
    // occurredAt is deterministic.
    let method = CorrectionRateMethod::new();
    let publisher = InMemoryPublisher::new();
    let cfg = ObserverConfig {
        observer_did: "did:plc:observer".to_owned(),
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

    // two events produce two flushes — plus one terminal flush when
    // the stream closes — but the terminal flush sees the same state
    // as the last event-count flush, so the publisher ends with at
    // least two observations.
    let observations = handler.publisher().observations();
    assert!(
        observations.len() >= 2,
        "expected ≥ 2 observations, got {}",
        observations.len()
    );

    let last = observations.last().expect("at least one observation");
    assert_eq!(last.observer, "did:plc:observer");
    assert_eq!(last.occurred_at, "2026-04-20T12:00:00Z");
    assert_eq!(last.version, "1.0.0");
    assert_eq!(last.method.name, "correction-rate");

    // output includes both the lens (from the encounter) and the
    // encounter-scope bucket (from the correction). the rate for the
    // lens is 0 (one encounter, zero corrections), and the encounter
    // bucket has one lens-error correction.
    let lenses = last
        .output
        .get("lenses")
        .and_then(|v| v.as_object())
        .expect("output.lenses is an object");
    assert!(lenses.contains_key("at://did:plc:lens-author/dev.panproto.schema.lens/l1"));
    assert!(lenses.contains_key("encounter:at://did:plc:alice/dev.idiolect.encounter/3l5"));
}

#[tokio::test]
async fn observer_manual_schedule_skips_flushes() {
    // manual schedule: no flushes fire from the driver. the publisher
    // stays empty even after the stream completes.
    let mut stream = InMemoryEventStream::new();
    stream.push(make_event(
        1,
        "dev.idiolect.encounter",
        "3l5",
        fixture_encounter_body(),
    ));

    let method = CorrectionRateMethod::new();
    let publisher = InMemoryPublisher::new();
    let cfg = ObserverConfig::default();
    let handler = ObserverHandler::new(method, publisher, cfg);

    let cursors = InMemoryCursorStore::new();
    let ix = IndexerConfig::default();

    drive_observer(&mut stream, &handler, &cursors, &ix, FlushSchedule::Manual)
        .await
        .expect("drive_observer");

    // nothing was flushed.
    assert!(handler.publisher().is_empty());

    // explicit flush works: the aggregator saw one encounter, so the
    // snapshot is non-empty.
    let stamped = handler.flush().await.expect("manual flush");
    assert!(stamped.is_some());
    assert_eq!(handler.publisher().len(), 1);
}

#[tokio::test]
async fn observer_skips_events_outside_nsid_prefix() {
    // an event on a non-dev.idiolect.* collection should not reach
    // the aggregator, so no flushes carry any state.
    let mut stream = InMemoryEventStream::new();
    stream.push(make_event(
        1,
        "app.bsky.feed.post",
        "3l5",
        serde_json::json!({ "text": "unrelated" }),
    ));

    let method = CorrectionRateMethod::new();
    let publisher = InMemoryPublisher::new();
    let handler = ObserverHandler::new(method, publisher, ObserverConfig::default());

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

    // the aggregator saw nothing, so the snapshot is None and no
    // observation is published — not even the terminal flush
    // produces one.
    assert!(handler.publisher().is_empty());
}
