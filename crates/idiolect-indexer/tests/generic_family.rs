//! Tests for the family-parameterised loop introduced in v0.5.
//!
//! Exercises:
//!
//! - `drive_indexer::<F, _, _, _>` against a non-IdiolectFamily
//!   stub, end-to-end through stream → decode → handler → cursor.
//! - The `FamilyContract` error path, fired when a family's
//!   `contains` predicate accepts an NSID that its `decode` then
//!   refuses.
//! - `RecordHandler<F>` dispatched generically via `Arc`.

use std::sync::atomic::{AtomicUsize, Ordering};

use idiolect_indexer::{
    CursorStore, InMemoryCursorStore, InMemoryEventStream, IndexerAction, IndexerConfig,
    IndexerError, IndexerEvent, RawEvent, RecordHandler, drive_indexer,
};
use idiolect_records::{DecodeError, Nsid, RecordFamily};
use serde_json::json;

const STUB_NSID: &str = "com.stub.note";

#[derive(Debug, Clone, Copy)]
struct StubFamily;

#[derive(Debug, Clone)]
struct StubNote {
    body: String,
}

impl RecordFamily for StubFamily {
    type AnyRecord = StubNote;
    const ID: &'static str = "com.stub";

    fn contains(nsid: &Nsid) -> bool {
        nsid.as_str() == STUB_NSID
    }

    fn decode(
        nsid: &Nsid,
        body: serde_json::Value,
    ) -> Result<Option<Self::AnyRecord>, DecodeError> {
        if nsid.as_str() != STUB_NSID {
            return Ok(None);
        }
        let body = body
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        Ok(Some(StubNote { body }))
    }

    fn nsid_str(_record: &Self::AnyRecord) -> &'static str {
        STUB_NSID
    }

    fn to_typed_json(record: &Self::AnyRecord) -> Result<serde_json::Value, serde_json::Error> {
        Ok(json!({ "$type": STUB_NSID, "body": record.body }))
    }
}

struct CountingHandler {
    seen: AtomicUsize,
}

impl CountingHandler {
    const fn new() -> Self {
        Self {
            seen: AtomicUsize::new(0),
        }
    }
    fn count(&self) -> usize {
        self.seen.load(Ordering::SeqCst)
    }
}

impl RecordHandler<StubFamily> for CountingHandler {
    async fn handle(&self, _event: &IndexerEvent<StubFamily>) -> Result<(), IndexerError> {
        self.seen.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn note_event(seq: u64, collection: &str) -> RawEvent {
    RawEvent {
        seq,
        live: true,
        did: "did:plc:author".into(),
        rev: format!("rev{seq}"),
        collection: Nsid::parse(collection).expect("valid nsid"),
        rkey: format!("r{seq}"),
        action: IndexerAction::Create,
        cid: Some(format!("bafy{seq}")),
        body: Some(json!({ "body": "hello" })),
    }
}

#[tokio::test]
async fn drive_indexer_routes_through_a_non_idiolect_family() {
    let mut stream = InMemoryEventStream::new();
    stream.push(note_event(1, STUB_NSID));
    stream.push(note_event(2, "dev.idiolect.encounter")); // out-of-family for StubFamily
    stream.push(note_event(3, STUB_NSID));

    let handler = CountingHandler::new();
    let cursors = InMemoryCursorStore::new();
    drive_indexer::<StubFamily, _, _, _>(
        &mut stream,
        &handler,
        &cursors,
        &IndexerConfig::default(),
    )
    .await
    .expect("loop runs");

    assert_eq!(handler.count(), 2, "out-of-family event filtered");
    assert_eq!(
        cursors.load("idiolect-indexer").await.unwrap(),
        Some(3),
        "cursor follows the most recent live in-family event",
    );
}

// -----------------------------------------------------------------
// FamilyContract error path
// -----------------------------------------------------------------

/// A buggy family: `contains` says yes for `com.bug.everything`,
/// but `decode` returns `Ok(None)` for the same NSID. The indexer
/// must surface this as `IndexerError::FamilyContract`, not crash
/// or silently drop.
#[derive(Debug, Clone, Copy)]
struct BuggyFamily;

impl RecordFamily for BuggyFamily {
    type AnyRecord = ();
    const ID: &'static str = "com.bug";

    fn contains(_nsid: &Nsid) -> bool {
        true
    }

    fn decode(
        _nsid: &Nsid,
        _body: serde_json::Value,
    ) -> Result<Option<Self::AnyRecord>, DecodeError> {
        Ok(None)
    }

    fn nsid_str(_record: &()) -> &'static str {
        "com.bug.everything"
    }

    fn to_typed_json(_record: &()) -> Result<serde_json::Value, serde_json::Error> {
        Ok(json!({}))
    }
}

struct PanicHandler;

impl RecordHandler<BuggyFamily> for PanicHandler {
    async fn handle(&self, _event: &IndexerEvent<BuggyFamily>) -> Result<(), IndexerError> {
        panic!("handler should never fire when decode fails its contract");
    }
}

#[tokio::test]
async fn family_contract_violation_surfaces_dedicated_error() {
    let mut stream = InMemoryEventStream::new();
    stream.push(note_event(1, "com.bug.everything"));

    let cursors = InMemoryCursorStore::new();
    let err = drive_indexer::<BuggyFamily, _, _, _>(
        &mut stream,
        &PanicHandler,
        &cursors,
        &IndexerConfig::default(),
    )
    .await
    .expect_err("contract violation surfaces as error");

    assert!(
        matches!(err, IndexerError::FamilyContract(ref s) if s == "com.bug.everything"),
        "expected FamilyContract, got {err:?}",
    );
}
