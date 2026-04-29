//! Demonstrates the canonical pattern for consuming a federated
//! firehose carrying records from two record families: idiolect's own
//! `dev.idiolect.*` plus a downstream community's `community.*`
//! lexicons. The driver is `OrFamily<IdiolectFamily, OtherFamily>`.
//!
//! This test stands in for the `firehose-or-family.rs` example bin
//! described in the original plan: it actually runs, so the wiring
//! pattern stays correct as the codebase evolves. A real downstream
//! family (e.g. `BlackskyFamily` codegen-built from
//! `community.blacksky.*`) would replace `StubCommunityFamily`
//! verbatim.

use std::sync::Mutex;

use idiolect_indexer::{
    InMemoryCursorStore, InMemoryEventStream, IndexerAction, IndexerConfig, IndexerError,
    IndexerEvent, RawEvent, RecordHandler, drive_indexer,
};
use idiolect_records::{IdiolectFamily, Nsid, OrAny, OrFamily, RecordFamily, record::DecodeError};
use serde_json::Value;

// ---------------------------------------------------------------
// A minimal second family. In production this would be the
// codegen-emitted `BlackskyFamily` (or `LayersFamily`, etc.) over
// the downstream community's lexicons. For the purpose of
// demonstrating OrFamily wiring, a one-NSID stub is enough.
// ---------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct CommunityPost {
    text: String,
}

#[derive(Debug, Clone)]
enum CommunityRecord {
    Post(CommunityPost),
}

struct StubCommunityFamily;

const COMMUNITY_POST_NSID: &str = "community.example.feed.post";

impl RecordFamily for StubCommunityFamily {
    type AnyRecord = CommunityRecord;

    const ID: &'static str = "community.example";

    fn contains(nsid: &Nsid) -> bool {
        nsid.as_str() == COMMUNITY_POST_NSID
    }

    fn decode(nsid: &Nsid, body: Value) -> Result<Option<Self::AnyRecord>, DecodeError> {
        if nsid.as_str() != COMMUNITY_POST_NSID {
            return Ok(None);
        }
        // Use a real serde error to populate the variant when `text`
        // is missing. Trying to deserialize a JSON null into a
        // non-null `String` yields one without us hand-rolling.
        let Some(text) = body.get("text").and_then(Value::as_str).map(str::to_owned) else {
            let err: serde_json::Error =
                serde_json::from_value::<String>(Value::Null).expect_err("null is not a String");
            return Err(DecodeError::Serde(err));
        };
        Ok(Some(CommunityRecord::Post(CommunityPost { text })))
    }

    fn nsid_str(_record: &Self::AnyRecord) -> &'static str {
        COMMUNITY_POST_NSID
    }

    fn to_typed_json(record: &Self::AnyRecord) -> Result<Value, serde_json::Error> {
        let CommunityRecord::Post(p) = record;
        Ok(serde_json::json!({
            "$type": COMMUNITY_POST_NSID,
            "text":  p.text,
        }))
    }
}

// ---------------------------------------------------------------
// A handler that counts records observed per side of the OrFamily.
// `OrFamily<F1, F2>::AnyRecord = OrAny<F1::AnyRecord, F2::AnyRecord>`,
// so the handler dispatches on the tag.
// ---------------------------------------------------------------

#[derive(Default)]
struct CountingHandler {
    state: Mutex<HandlerState>,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
struct HandlerState {
    idiolect_records: u64,
    community_records: u64,
    deletes: u64,
}

impl CountingHandler {
    fn snapshot(&self) -> HandlerState {
        *self.state.lock().expect("not poisoned")
    }
}

type Combined = OrFamily<IdiolectFamily, StubCommunityFamily>;

impl RecordHandler<Combined> for CountingHandler {
    async fn handle(&self, event: &IndexerEvent<Combined>) -> Result<(), IndexerError> {
        let mut s = self.state.lock().expect("not poisoned");
        if matches!(event.action, IndexerAction::Delete) {
            s.deletes = s.deletes.saturating_add(1);
            return Ok(());
        }
        match event.record.as_ref() {
            Some(OrAny::Left(_)) => s.idiolect_records = s.idiolect_records.saturating_add(1),
            Some(OrAny::Right(_)) => s.community_records = s.community_records.saturating_add(1),
            None => {} // out-of-family NSID, silently filtered
        }
        Ok(())
    }
}

// ---------------------------------------------------------------
// Test: drive a mixed stream through OrFamily.
// ---------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn or_family_routes_records_to_each_side() {
    let mut stream = InMemoryEventStream::new();

    // An idiolect deliberation record (left side).
    stream.push(RawEvent {
        seq: 1,
        live: true,
        did: "did:plc:alice".to_owned(),
        rev: "rev1".to_owned(),
        collection: Nsid::parse("dev.idiolect.deliberation").expect("valid nsid"),
        rkey: "d1".to_owned(),
        action: IndexerAction::Create,
        cid: Some("bafyrei1".to_owned()),
        body: Some(serde_json::json!({
            "owningCommunity": "at://did:plc:c/dev.idiolect.community/main",
            "topic":           "Adopt v2?",
            "createdAt":       "2026-04-29T00:00:00.000Z",
        })),
    });

    // A community record (right side).
    stream.push(RawEvent {
        seq: 2,
        live: true,
        did: "did:plc:bob".to_owned(),
        rev: "rev2".to_owned(),
        collection: Nsid::parse(COMMUNITY_POST_NSID).expect("valid nsid"),
        rkey: "p1".to_owned(),
        action: IndexerAction::Create,
        cid: Some("bafyrei2".to_owned()),
        body: Some(serde_json::json!({ "text": "hello" })),
    });

    // An out-of-family record. Both families filter via `contains`,
    // so the indexer drops it without surfacing to the handler.
    stream.push(RawEvent {
        seq: 3,
        live: true,
        did: "did:plc:carol".to_owned(),
        rev: "rev3".to_owned(),
        collection: Nsid::parse("app.bsky.feed.post").expect("valid nsid"),
        rkey: "x1".to_owned(),
        action: IndexerAction::Create,
        cid: Some("bafyrei3".to_owned()),
        body: Some(serde_json::json!({ "text": "ignored" })),
    });

    // A delete on an idiolect record.
    stream.push(RawEvent {
        seq: 4,
        live: true,
        did: "did:plc:alice".to_owned(),
        rev: "rev4".to_owned(),
        collection: Nsid::parse("dev.idiolect.deliberation").expect("valid nsid"),
        rkey: "d1".to_owned(),
        action: IndexerAction::Delete,
        cid: None,
        body: None,
    });

    let handler = CountingHandler::default();
    let cursors = InMemoryCursorStore::new();
    let cfg = IndexerConfig::default();

    drive_indexer::<Combined, _, _, _>(&mut stream, &handler, &cursors, &cfg)
        .await
        .expect("drive succeeds");

    let s = handler.snapshot();
    assert_eq!(s.idiolect_records, 1, "exactly one idiolect record");
    assert_eq!(s.community_records, 1, "exactly one community record");
    assert_eq!(s.deletes, 1, "exactly one delete");
}

#[test]
fn or_family_overlap_detector_finds_no_overlap_for_disjoint_families() {
    use idiolect_records::detect_or_family_overlap;
    let probe = vec![
        Nsid::parse("dev.idiolect.deliberation").unwrap(),
        Nsid::parse("dev.idiolect.encounter").unwrap(),
        Nsid::parse(COMMUNITY_POST_NSID).unwrap(),
        Nsid::parse("app.bsky.feed.post").unwrap(),
    ];
    let overlaps = detect_or_family_overlap::<IdiolectFamily, StubCommunityFamily>(&probe);
    assert!(
        overlaps.is_empty(),
        "no NSID overlap expected: {overlaps:?}"
    );
}
