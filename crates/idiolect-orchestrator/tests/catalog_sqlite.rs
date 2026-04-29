//! Tests for the sqlite-backed orchestrator catalog store.

#![cfg(feature = "catalog-sqlite")]

use std::sync::Arc;

use idiolect_indexer::{IndexerAction, IndexerEvent, RecordHandler};
use idiolect_orchestrator::{CatalogHandler, SqliteCatalogStore, handler::CatalogPersist, query};
use idiolect_records::generated::dev::idiolect::adapter::{
    AdapterInvocationProtocol, AdapterInvocationProtocolKind, AdapterIsolation,
    AdapterIsolationKind,
};
use idiolect_records::{Adapter, AnyRecord};
use tempfile::tempdir;

fn adapter(framework: &str) -> Adapter {
    Adapter {
        author: idiolect_records::Did::parse("did:plc:author").expect("valid DID"),
        framework: framework.into(),
        invocation_protocol: AdapterInvocationProtocol {
            entry_point: Some("bin/x".into()),
            input_schema: None,
            kind: AdapterInvocationProtocolKind::Subprocess,
            output_schema: None,
            kind_vocab: None,
        },
        isolation: AdapterIsolation {
            kind: AdapterIsolationKind::Process,
            filesystem_policy: None,
            network_policy: None,
            resource_limits: None,
            filesystem_policy_vocab: None,
            kind_vocab: None,
            network_policy_vocab: None,
        },
        occurred_at: idiolect_records::Datetime::parse("2026-04-21T00:00:00Z")
            .expect("valid datetime"),
        verification: None,
        version_range: ">=1 <2".into(),
    }
}

#[test]
fn open_creates_schema_and_starts_empty() {
    let dir = tempdir().unwrap();
    let store = SqliteCatalogStore::open(dir.path().join("c.db")).unwrap();
    assert_eq!(store.len().unwrap(), 0);
}

#[test]
fn upsert_then_load_roundtrips() {
    let dir = tempdir().unwrap();
    let store = SqliteCatalogStore::open(dir.path().join("c.db")).unwrap();
    store
        .upsert(
            "at://did:plc:x/dev.idiolect.adapter/a1",
            "did:plc:author",
            "rev1",
            &AnyRecord::Adapter(adapter("hasura")),
        )
        .unwrap();
    store
        .upsert(
            "at://did:plc:x/dev.idiolect.adapter/a2",
            "did:plc:author",
            "rev2",
            &AnyRecord::Adapter(adapter("prisma")),
        )
        .unwrap();

    let catalog = store.load_catalog().unwrap();
    assert_eq!(catalog.len(), 2);
    let hasura = query::adapters_for_framework(&catalog, "hasura");
    assert_eq!(hasura.len(), 1);
    let prisma = query::adapters_for_framework(&catalog, "prisma");
    assert_eq!(prisma.len(), 1);
}

#[test]
fn upsert_replaces_existing() {
    let dir = tempdir().unwrap();
    let store = SqliteCatalogStore::open(dir.path().join("c.db")).unwrap();
    let uri = "at://did:plc:x/dev.idiolect.adapter/a1";
    store
        .upsert(
            uri,
            "did:plc:author",
            "rev1",
            &AnyRecord::Adapter(adapter("hasura")),
        )
        .unwrap();
    store
        .upsert(
            uri,
            "did:plc:author",
            "rev2",
            &AnyRecord::Adapter(adapter("prisma")),
        )
        .unwrap();
    assert_eq!(store.len().unwrap(), 1);
    let catalog = store.load_catalog().unwrap();
    let a = catalog.adapters().next().unwrap();
    assert_eq!(a.record.framework, "prisma");
    assert_eq!(a.rev, "rev2");
}

#[test]
fn remove_deletes_row() {
    let dir = tempdir().unwrap();
    let store = SqliteCatalogStore::open(dir.path().join("c.db")).unwrap();
    let uri = "at://did:plc:x/dev.idiolect.adapter/a1";
    store
        .upsert(
            uri,
            "did:plc:author",
            "rev1",
            &AnyRecord::Adapter(adapter("hasura")),
        )
        .unwrap();
    store.remove(uri).unwrap();
    assert_eq!(store.len().unwrap(), 0);
}

#[test]
fn reopen_sees_committed_rows() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("c.db");
    {
        let store = SqliteCatalogStore::open(&path).unwrap();
        store
            .upsert(
                "at://a/dev.idiolect.adapter/x",
                "did:plc:a",
                "r",
                &AnyRecord::Adapter(adapter("fhir")),
            )
            .unwrap();
    }
    let store = SqliteCatalogStore::open(&path).unwrap();
    assert_eq!(store.len().unwrap(), 1);
    let catalog = store.load_catalog().unwrap();
    assert_eq!(catalog.adapters().count(), 1);
}

#[tokio::test]
async fn handler_with_persist_mirrors_ingest() {
    let dir = tempdir().unwrap();
    let store = Arc::new(SqliteCatalogStore::open(dir.path().join("c.db")).unwrap());
    let handler = CatalogHandler::new().with_persist(store.clone() as Arc<dyn CatalogPersist>);

    let event = IndexerEvent {
        seq: 1,
        live: true,
        did: "did:plc:author".into(),
        rev: "r1".into(),
        collection: idiolect_records::Nsid::parse("dev.idiolect.adapter").expect("valid nsid"),
        rkey: "a1".into(),
        action: IndexerAction::Create,
        cid: None,
        record: Some(AnyRecord::Adapter(adapter("datomic"))),
    };
    RecordHandler::handle(&handler, &event).await.unwrap();

    // sqlite has the row.
    assert_eq!(store.len().unwrap(), 1);
    // in-memory catalog also has it.
    assert_eq!(handler.catalog().lock().unwrap().adapters().count(), 1);

    // delete mirrors to both layers.
    let mut del = event.clone();
    del.action = IndexerAction::Delete;
    del.record = None;
    RecordHandler::handle(&handler, &del).await.unwrap();
    assert_eq!(store.len().unwrap(), 0);
    assert_eq!(handler.catalog().lock().unwrap().adapters().count(), 0);
}

#[tokio::test]
async fn handler_with_persist_halts_on_store_error() {
    // Supply a store pointed at a non-writable path to force a failure.
    struct AlwaysFailPersist;
    impl CatalogPersist for AlwaysFailPersist {
        fn persist_upsert(
            &self,
            _uri: &str,
            _author: &str,
            _rev: &str,
            _record: &AnyRecord,
        ) -> Result<(), String> {
            Err("disk full".into())
        }

        fn persist_remove(&self, _uri: &str) -> Result<(), String> {
            Err("disk full".into())
        }
    }

    let handler =
        CatalogHandler::new().with_persist(Arc::new(AlwaysFailPersist) as Arc<dyn CatalogPersist>);
    let event = IndexerEvent {
        seq: 1,
        live: true,
        did: "did:plc:author".into(),
        rev: "r1".into(),
        collection: idiolect_records::Nsid::parse("dev.idiolect.adapter").expect("valid nsid"),
        rkey: "a1".into(),
        action: IndexerAction::Create,
        cid: None,
        record: Some(AnyRecord::Adapter(adapter("datomic"))),
    };
    let err = RecordHandler::handle(&handler, &event).await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("disk full"), "got: {msg}");
    // In-memory catalog stayed empty because the persist error halted
    // the handler before the upsert landed.
    assert_eq!(handler.catalog().lock().unwrap().adapters().count(), 0);
}
