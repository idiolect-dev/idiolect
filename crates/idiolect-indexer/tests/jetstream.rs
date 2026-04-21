//! Tests for the Jetstream-backed event stream.
//!
//! Both the pure frame parser and the offline `from_lines` constructor
//! are exercised; the live websocket path is not covered here because
//! it needs a jetstream-shaped mock server (future work).

#![cfg(feature = "firehose-jetstream")]

use idiolect_indexer::{EventStream, IndexerAction, JetstreamEventStream, parse_jetstream_frame};

fn commit(op: &str, rkey: &str, with_record: bool) -> String {
    let record = if with_record {
        serde_json::json!({ "text": "hi" })
    } else {
        serde_json::Value::Null
    };
    serde_json::json!({
        "did": "did:plc:alice",
        "time_us": 1_730_000_000_000_000_u64,
        "kind": "commit",
        "commit": {
            "rev": "3l5",
            "operation": op,
            "collection": "dev.idiolect.encounter",
            "rkey": rkey,
            "record": record,
            "cid": "bafyreitest",
        }
    })
    .to_string()
}

#[test]
fn parse_create_frame() {
    let line = commit("create", "3l5", true);
    let event = parse_jetstream_frame(&line).unwrap().unwrap();
    assert_eq!(event.did, "did:plc:alice");
    assert_eq!(event.action, IndexerAction::Create);
    assert_eq!(event.collection, "dev.idiolect.encounter");
    assert_eq!(event.rkey, "3l5");
    assert_eq!(event.seq, 1_730_000_000_000_000);
    assert!(event.body.is_some());
}

#[test]
fn parse_delete_frame_has_no_body() {
    let line = serde_json::json!({
        "did": "did:plc:alice",
        "time_us": 2_u64,
        "kind": "commit",
        "commit": {
            "rev": "3l6",
            "operation": "delete",
            "collection": "dev.idiolect.encounter",
            "rkey": "r1",
        }
    })
    .to_string();
    let event = parse_jetstream_frame(&line).unwrap().unwrap();
    assert_eq!(event.action, IndexerAction::Delete);
    assert!(event.cid.is_none());
    // body may be Some(Null) per jetstream; either way the indexer
    // treats deletes as body-less by way of the action flag.
}

#[test]
fn skip_non_commit_frames() {
    let line = serde_json::json!({
        "did": "did:plc:alice",
        "time_us": 3_u64,
        "kind": "identity",
    })
    .to_string();
    assert!(parse_jetstream_frame(&line).unwrap().is_none());
}

#[test]
fn unknown_operation_is_error() {
    let line = serde_json::json!({
        "did": "did:plc:alice",
        "time_us": 4_u64,
        "kind": "commit",
        "commit": {
            "rev": "3l7",
            "operation": "teleport",
            "collection": "dev.idiolect.encounter",
            "rkey": "r2",
        }
    })
    .to_string();
    let err = parse_jetstream_frame(&line).unwrap_err();
    assert!(err.to_string().contains("teleport"));
}

#[tokio::test]
async fn from_lines_drives_like_in_memory() {
    let lines = vec![
        commit("create", "r1", true),
        serde_json::json!({ "did": "did:plc:x", "time_us": 1u64, "kind": "identity" }).to_string(),
        commit("update", "r1", true),
    ];
    let mut stream = JetstreamEventStream::from_lines(lines);

    let e1 = stream.next_event().await.unwrap().unwrap();
    assert_eq!(e1.action, IndexerAction::Create);

    // identity frame is skipped by the parser; next call yields the
    // update.
    let e2 = stream.next_event().await.unwrap().unwrap();
    assert_eq!(e2.action, IndexerAction::Update);

    assert!(stream.next_event().await.unwrap().is_none());
}

#[tokio::test]
async fn malformed_frame_surfaces_as_stream_error() {
    let mut stream = JetstreamEventStream::from_lines(vec!["not-json".to_owned()]);
    let err = stream.next_event().await.unwrap_err();
    assert!(err.to_string().to_ascii_lowercase().contains("parse"));
}

#[test]
fn keepalive_interval_has_sensible_default_and_is_configurable() {
    // Test-mode streams ignore the interval (no socket to ping) but
    // the getter/setter still need to work for the live-connect path.
    let s = JetstreamEventStream::from_lines(Vec::<String>::new());
    assert_eq!(s.keepalive_interval(), std::time::Duration::from_secs(30));
    let s2 = s.with_keepalive_interval(std::time::Duration::from_secs(5));
    assert_eq!(s2.keepalive_interval(), std::time::Duration::from_secs(5));
}
