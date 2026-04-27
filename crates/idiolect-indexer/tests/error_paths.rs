//! Error-path tests for the indexer loop.
//!
//! These cover behaviors the happy-path suite does not exercise:
//!
//! - A handler that returns `IndexerError::Handler` surfaces the error
//!   and halts the loop.
//! - A decode failure on a `dev.idiolect.*` collection surfaces as
//!   `IndexerError::Decode`.
//! - An unknown-but-matching nsid is classified as decode failure.
//! - A cursor store that errors on commit halts the loop.
//! - A stream that errors surfaces as `IndexerError::Stream`.

use idiolect_indexer::{
    CursorStore, EventStream, IndexerAction, IndexerConfig, IndexerError, RawEvent, RecordHandler,
    drive_idiolect_indexer,
};

/// Stream that returns one `Err(Stream)` on first call.
struct ExplodingStream {
    fired: bool,
}

impl EventStream for ExplodingStream {
    async fn next_event(&mut self) -> Result<Option<RawEvent>, IndexerError> {
        if self.fired {
            Ok(None)
        } else {
            self.fired = true;
            Err(IndexerError::Stream("boom".to_owned()))
        }
    }
}

/// Handler that always errors.
struct FailingHandler;

impl RecordHandler for FailingHandler {
    async fn handle(&self, _event: &idiolect_indexer::IndexerEvent) -> Result<(), IndexerError> {
        Err(IndexerError::Handler("handler rejected".to_owned()))
    }
}

/// Cursor store that fails on commit.
struct FailingCursorStore;

impl CursorStore for FailingCursorStore {
    async fn load(&self, _subscription_id: &str) -> Result<Option<u64>, IndexerError> {
        Ok(None)
    }

    async fn commit(&self, _subscription_id: &str, _seq: u64) -> Result<(), IndexerError> {
        Err(IndexerError::Cursor("disk full".to_owned()))
    }
}

/// Accepting handler used when we want commits to be tested.
struct OkHandler;

impl RecordHandler for OkHandler {
    async fn handle(&self, _event: &idiolect_indexer::IndexerEvent) -> Result<(), IndexerError> {
        Ok(())
    }
}

/// Single-shot event stream backed by a Vec we drain.
struct PreloadedStream {
    events: std::collections::VecDeque<RawEvent>,
}

impl EventStream for PreloadedStream {
    async fn next_event(&mut self) -> Result<Option<RawEvent>, IndexerError> {
        Ok(self.events.pop_front())
    }
}

fn preloaded(events: Vec<RawEvent>) -> PreloadedStream {
    PreloadedStream {
        events: events.into_iter().collect(),
    }
}

fn encounter_event(seq: u64) -> RawEvent {
    RawEvent {
        seq,
        live: true,
        did: "did:plc:alice".to_owned(),
        rev: "3l5".to_owned(),
        collection: idiolect_records::Nsid::parse("dev.idiolect.encounter").expect("nsid"),
        rkey: "r1".to_owned(),
        action: IndexerAction::Create,
        cid: Some("bafyrei".to_owned()),
        body: Some(serde_json::json!({
            "kind": "invocation-log",
            "lens": { "uri": "at://did:plc:x/dev.panproto.schema.lens/l1" },
            "occurredAt": "2026-04-20T10:00:00Z",
            "use": { "action": "t" },
            "sourceSchema": { "uri": "at://did:plc:x/dev.panproto.schema.schema/s" },
            "visibility": "public-detailed"
        })),
    }
}

#[tokio::test]
async fn stream_error_halts_loop() {
    use idiolect_indexer::InMemoryCursorStore;
    let mut stream = ExplodingStream { fired: false };
    let handler = OkHandler;
    let cursors = InMemoryCursorStore::new();
    let err = drive_idiolect_indexer(&mut stream, &handler, &cursors, &IndexerConfig::default())
        .await
        .unwrap_err();
    assert!(matches!(err, IndexerError::Stream(msg) if msg == "boom"));
}

#[tokio::test]
async fn handler_error_halts_loop_and_does_not_commit() {
    use idiolect_indexer::InMemoryCursorStore;
    let mut stream = preloaded(vec![encounter_event(1)]);
    let handler = FailingHandler;
    let cursors = InMemoryCursorStore::new();
    let err = drive_idiolect_indexer(&mut stream, &handler, &cursors, &IndexerConfig::default())
        .await
        .unwrap_err();
    assert!(matches!(err, IndexerError::Handler(_)));
    // cursor was never committed because the handler failed first.
    assert!(cursors.load("idiolect-indexer").await.unwrap().is_none());
}

#[tokio::test]
async fn cursor_commit_error_halts_loop() {
    let mut stream = preloaded(vec![encounter_event(1)]);
    let handler = OkHandler;
    let cursors = FailingCursorStore;
    let err = drive_idiolect_indexer(&mut stream, &handler, &cursors, &IndexerConfig::default())
        .await
        .unwrap_err();
    assert!(matches!(err, IndexerError::Cursor(msg) if msg.contains("disk full")));
}

#[tokio::test]
async fn malformed_body_on_idiolect_collection_surfaces_decode_error() {
    use idiolect_indexer::InMemoryCursorStore;
    let mut evt = encounter_event(1);
    // missing required `kind` field.
    evt.body = Some(serde_json::json!({ "purpose": "t" }));
    let mut stream = preloaded(vec![evt]);
    let handler = OkHandler;
    let cursors = InMemoryCursorStore::new();
    let err = drive_idiolect_indexer(&mut stream, &handler, &cursors, &IndexerConfig::default())
        .await
        .unwrap_err();
    assert!(matches!(err, IndexerError::Decode(_)), "got {err:?}");
}

#[tokio::test]
async fn unknown_idiolect_nsid_is_dropped_silently() {
    // Post-v0.5 (issues #38/#39): family membership is the
    // family's `contains` predicate, which enumerates the family's
    // exact NSID set. A `dev.idiolect.*` NSID that is not a known
    // record type is no longer blown up on; it's just out-of-family
    // for this codegen output and silently dropped, same as
    // `app.bsky.feed.post` would be. This is a correctness
    // improvement: an upstream PDS adding a new record type ahead
    // of our codegen no longer halts the loop.
    use idiolect_indexer::InMemoryCursorStore;
    let mut evt = encounter_event(1);
    evt.collection =
        idiolect_records::Nsid::parse("dev.idiolect.notARealRecord").expect("valid nsid");
    let mut stream = preloaded(vec![evt]);
    let handler = OkHandler;
    let cursors = InMemoryCursorStore::new();
    drive_idiolect_indexer(&mut stream, &handler, &cursors, &IndexerConfig::default())
        .await
        .unwrap();
    // Cursor untouched: the event was filtered before processing.
    assert!(cursors.load("idiolect-indexer").await.unwrap().is_none());
}
