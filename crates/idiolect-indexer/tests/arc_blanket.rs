//! Smoke tests for the `Arc<T>` forwarding impls on the indexer's
//! boundary traits. The blanket impls let a caller share a single
//! store/handler instance across tasks without a manual delegate.
//!
//! Note: the async-fn-in-trait traits here are not `dyn`-compatible
//! (native async fn cannot build a vtable), so these tests use
//! concrete `Arc<T>` rather than `Arc<dyn Trait>`. A caller that
//! needs a trait object today has to adopt the `async-trait` crate
//! or box the future manually — idiolect deliberately stays on
//! native async fn to avoid the runtime cost.

use std::sync::Arc;

use idiolect_indexer::{
    CursorStore, InMemoryCursorStore, InMemoryEventStream, IndexerAction, IndexerConfig,
    IndexerError, IndexerEvent, NoopRecordHandler, RawEvent, RecordHandler, drive_idiolect_indexer,
};

#[tokio::test]
async fn arc_cursor_store_works() {
    let store = Arc::new(InMemoryCursorStore::new());
    assert!(store.load("sub").await.unwrap().is_none());
    store.commit("sub", 42).await.unwrap();
    assert_eq!(store.load("sub").await.unwrap(), Some(42));
}

#[tokio::test]
async fn arc_record_handler_works() {
    let handler = Arc::new(NoopRecordHandler::new());
    let event = IndexerEvent::<idiolect_records::IdiolectFamily> {
        seq: 1,
        live: true,
        did: "did:plc:x".to_owned(),
        rev: "r".to_owned(),
        collection: idiolect_records::Nsid::parse("dev.idiolect.encounter").expect("nsid"),
        rkey: "k".to_owned(),
        action: IndexerAction::Create,
        cid: None,
        record: None,
    };
    handler.handle(&event).await.unwrap();
    assert_eq!(handler.observed(), 1);
}

#[tokio::test]
async fn arc_wrapped_types_drive_idiolect_indexer() {
    // Representative shape: the daemon holds `Arc<Handler>` + `Arc<Store>`
    // and passes references to drive_indexer. The Arc forwarding impls
    // let `&Arc<...>` satisfy the `&impl Trait` bound without a deref
    // cast at the call site.
    let store = Arc::new(InMemoryCursorStore::new());
    let handler = Arc::new(NoopRecordHandler::new());

    let mut stream = InMemoryEventStream::new();
    stream.push(RawEvent {
        seq: 7,
        live: true,
        did: "did:plc:x".to_owned(),
        rev: "r".to_owned(),
        collection: idiolect_records::Nsid::parse("dev.idiolect.encounter").expect("nsid"),
        rkey: "k".to_owned(),
        action: IndexerAction::Delete,
        cid: None,
        body: None,
    });

    drive_idiolect_indexer(&mut stream, &handler, &store, &IndexerConfig::default())
        .await
        .unwrap();
    assert_eq!(handler.observed(), 1);
    assert_eq!(store.load("idiolect-indexer").await.unwrap(), Some(7));
}

/// Validates that an Arc wrapper forwards errors (not swallowed).
#[tokio::test]
async fn arc_cursor_store_forwards_errors() {
    struct Failing;
    impl CursorStore for Failing {
        async fn load(&self, _id: &str) -> Result<Option<u64>, IndexerError> {
            Err(IndexerError::Cursor("disk offline".into()))
        }
        async fn commit(&self, _id: &str, _seq: u64) -> Result<(), IndexerError> {
            Err(IndexerError::Cursor("disk offline".into()))
        }
    }
    let store = Arc::new(Failing);
    let err = store.load("s").await.unwrap_err();
    assert!(matches!(err, IndexerError::Cursor(msg) if msg.contains("disk offline")));
}

/// Cloning the Arc and using clones concurrently exercises the shared
/// state the blanket impl enables.
#[tokio::test]
async fn arc_cursor_store_state_is_shared_across_clones() {
    let store = Arc::new(InMemoryCursorStore::new());
    let a = Arc::clone(&store);
    let b = Arc::clone(&store);
    a.commit("sub", 10).await.unwrap();
    // Both clones see the same state because the Arc points at the
    // same inner Mutex-guarded HashMap.
    assert_eq!(b.load("sub").await.unwrap(), Some(10));
    assert_eq!(store.load("sub").await.unwrap(), Some(10));
}
