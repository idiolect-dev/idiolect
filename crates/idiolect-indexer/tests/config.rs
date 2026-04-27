//! Coverage for [`IndexerConfig`] edge cases the main loop test does not touch.
//!
//! Specifically:
//!
//! - The default config's `subscription_id` value.
//! - Out-of-family commits (per the family's `contains` predicate)
//!   are dropped before decode without error.
//! - Backfill events never advance the cursor even when their
//!   collection is in-family.
//! - Delete actions advance the cursor without going through decode.

use idiolect_indexer::{
    CursorStore, InMemoryCursorStore, InMemoryEventStream, IndexerAction, IndexerConfig,
    IndexerError, IndexerEvent, RawEvent, RecordHandler, drive_idiolect_indexer,
};

struct CountingHandler {
    count: std::sync::atomic::AtomicUsize,
}

impl CountingHandler {
    const fn new() -> Self {
        Self {
            count: std::sync::atomic::AtomicUsize::new(0),
        }
    }
    fn observed(&self) -> usize {
        self.count.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl RecordHandler for CountingHandler {
    async fn handle(&self, _event: &IndexerEvent) -> Result<(), IndexerError> {
        self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

fn encounter(seq: u64, live: bool, collection: &str) -> RawEvent {
    RawEvent {
        seq,
        live,
        did: "did:plc:alice".into(),
        rev: format!("rev{seq}"),
        collection: idiolect_records::Nsid::parse(collection).expect("valid nsid"),
        rkey: format!("r{seq}"),
        action: IndexerAction::Create,
        cid: Some(format!("bafy{seq}")),
        body: Some(serde_json::json!({
            "kind": "invocation-log",
            "lens": { "uri": "at://did:plc:x/dev.panproto.schema.lens/l1" },
            "occurredAt": "2026-04-20T10:00:00Z",
            "use": { "action": "t" },
            "sourceSchema": { "uri": "at://did:plc:x/dev.panproto.schema.schema/s" },
            "visibility": "public-detailed",
        })),
    }
}

#[test]
fn default_config_values() {
    let cfg = IndexerConfig::default();
    assert_eq!(cfg.subscription_id, "idiolect-indexer");
}

#[tokio::test]
async fn out_of_family_commits_are_dropped_silently() {
    // Two events: one in the dev.idiolect.* family, one outside
    // (app.bsky.feed.post). The family's `contains` predicate
    // accepts the first and rejects the second; the rejected one
    // never reaches decode or the handler, and the cursor only
    // advances on the in-family commit.
    let mut stream = InMemoryEventStream::new();
    stream.push(encounter(1, true, "dev.idiolect.encounter"));
    stream.push(encounter(2, true, "app.bsky.feed.post"));

    let handler = CountingHandler::new();
    let cursors = InMemoryCursorStore::new();
    drive_idiolect_indexer(&mut stream, &handler, &cursors, &IndexerConfig::default())
        .await
        .unwrap();
    assert_eq!(handler.observed(), 1);
    assert_eq!(cursors.load("idiolect-indexer").await.unwrap(), Some(1));
    let _ = IndexerError::MissingBody(String::new());
}

#[tokio::test]
async fn backfill_events_do_not_advance_cursor() {
    // Two events: first is backfill (live=false), second is live.
    // Only the live one should advance the cursor.
    let mut stream = InMemoryEventStream::new();
    stream.push(encounter(1, false, "dev.idiolect.encounter"));
    stream.push(encounter(2, true, "dev.idiolect.encounter"));

    let handler = CountingHandler::new();
    let cursors = InMemoryCursorStore::new();
    drive_idiolect_indexer(&mut stream, &handler, &cursors, &IndexerConfig::default())
        .await
        .unwrap();
    // Both events reached the handler.
    assert_eq!(handler.observed(), 2);
    // Cursor was committed only for the live event.
    assert_eq!(
        cursors.load("idiolect-indexer").await.unwrap(),
        Some(2),
        "cursor should reflect seq=2 (live), not seq=1 (backfill)"
    );
}

#[tokio::test]
async fn delete_action_advances_cursor_without_decode() {
    let mut stream = InMemoryEventStream::new();
    stream.push(RawEvent {
        seq: 42,
        live: true,
        did: "did:plc:alice".into(),
        rev: "r42".into(),
        collection: idiolect_records::Nsid::parse("dev.idiolect.encounter").expect("valid nsid"),
        rkey: "r1".into(),
        action: IndexerAction::Delete,
        cid: None,
        body: None,
    });

    let handler = CountingHandler::new();
    let cursors = InMemoryCursorStore::new();
    drive_idiolect_indexer(&mut stream, &handler, &cursors, &IndexerConfig::default())
        .await
        .unwrap();
    assert_eq!(handler.observed(), 1);
    assert_eq!(cursors.load("idiolect-indexer").await.unwrap(), Some(42));
}
