//! Coverage for [`IndexerConfig`] edge cases the main loop test does not touch.
//!
//! Specifically:
//!
//! - `nsid_prefix = ""` disables the filter so every collection reaches
//!   decode. Setting an empty prefix on the default config is a
//!   debugging knob.
//! - A prefix that matches nothing drops everything without error.
//! - The default config's `subscription_id` and `nsid_prefix` values.
//! - Backfill events never advance the cursor even when their
//!   collection is in-prefix.

use idiolect_indexer::{
    CursorStore, InMemoryCursorStore, InMemoryEventStream, IndexerAction, IndexerConfig,
    IndexerError, IndexerEvent, RawEvent, RecordHandler, drive_indexer,
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
        collection: collection.into(),
        rkey: format!("r{seq}"),
        action: IndexerAction::Create,
        cid: Some(format!("bafy{seq}")),
        body: Some(serde_json::json!({
            "kind": "invocation-log",
            "lens": { "uri": "at://did:plc:x/dev.panproto.schema.lens/l1" },
            "occurredAt": "2026-04-20T10:00:00Z",
            "purpose": "t",
            "sourceSchema": { "uri": "at://did:plc:x/dev.panproto.schema.schema/s" },
            "visibility": "public-detailed",
        })),
    }
}

#[test]
fn default_config_values() {
    let cfg = IndexerConfig::default();
    assert_eq!(cfg.subscription_id, "idiolect-indexer");
    assert_eq!(cfg.nsid_prefix, "dev.idiolect.");
}

#[tokio::test]
async fn prefix_matching_nothing_drops_every_event() {
    let mut stream = InMemoryEventStream::new();
    stream.push(encounter(1, true, "dev.idiolect.encounter"));
    stream.push(encounter(2, true, "app.bsky.feed.post"));

    let handler = CountingHandler::new();
    let cursors = InMemoryCursorStore::new();
    let cfg = IndexerConfig {
        subscription_id: "test".into(),
        nsid_prefix: "com.example.nothing.".into(),
    };
    drive_indexer(&mut stream, &handler, &cursors, &cfg)
        .await
        .unwrap();
    // Nothing routed through handler.
    assert_eq!(handler.observed(), 0);
    assert!(cursors.load("test").await.unwrap().is_none());
}

#[tokio::test]
async fn empty_prefix_disables_filter_but_decode_still_required() {
    // An empty prefix means "route every collection". Non-idiolect
    // collections then try to decode and fail, surfacing as a decode
    // error — the empty prefix is only safe for debugging against a
    // stream known to carry only dev.idiolect.* traffic.
    let mut stream = InMemoryEventStream::new();
    stream.push(encounter(1, true, "app.bsky.feed.post"));

    let handler = CountingHandler::new();
    let cursors = InMemoryCursorStore::new();
    let cfg = IndexerConfig {
        subscription_id: "test".into(),
        nsid_prefix: String::new(),
    };
    let err = drive_indexer(&mut stream, &handler, &cursors, &cfg)
        .await
        .unwrap_err();
    assert!(matches!(err, IndexerError::Decode(_)));
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
    drive_indexer(&mut stream, &handler, &cursors, &IndexerConfig::default())
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
        collection: "dev.idiolect.encounter".into(),
        rkey: "r1".into(),
        action: IndexerAction::Delete,
        cid: None,
        body: None,
    });

    let handler = CountingHandler::new();
    let cursors = InMemoryCursorStore::new();
    drive_indexer(&mut stream, &handler, &cursors, &IndexerConfig::default())
        .await
        .unwrap();
    assert_eq!(handler.observed(), 1);
    assert_eq!(cursors.load("idiolect-indexer").await.unwrap(), Some(42));
}
