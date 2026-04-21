//! Tests for the typed `RecordFetcher::list_records` helper plus the
//! underlying `PdsClient::list_records` wiring on `ReqwestPdsClient`.
//!
//! Uses wiremock to serve a canned `com.atproto.repo.listRecords`
//! response and exercises:
//! - empty-page + cursor returned
//! - single-page decode into typed records
//! - paginated follow-up via `cursor`
//! - error path when the body can't decode into the requested R.

#![cfg(feature = "pds-reqwest")]

use idiolect_lens::{LensError, RecordFetcher, ReqwestPdsClient};
use idiolect_records::Bounty;
use idiolect_records::generated::bounty::{BountyStatus, BountyWants, WantAdapter};
use serde_json::json;
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn bounty_body() -> serde_json::Value {
    json!({
        "constraints": "x",
        "occurredAt": "2026-04-21T00:00:00Z",
        "requester": "did:plc:alice",
        "status": "open",
        "wants": {
            "$type": "dev.idiolect.bounty#wantAdapter",
            "framework": "hasura"
        }
    })
}

#[tokio::test]
async fn empty_page_returns_empty_vec_and_no_cursor() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.listRecords"))
        .and(query_param("repo", "did:plc:alice"))
        .and(query_param("collection", "dev.idiolect.bounty"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "records": [] })))
        .mount(&server)
        .await;

    let client = ReqwestPdsClient::with_service_url(server.uri());
    let fetcher = RecordFetcher::new(client);
    let page = fetcher
        .list_records::<Bounty>("did:plc:alice", None, None)
        .await
        .unwrap();
    assert!(page.records.is_empty());
    assert!(page.cursor.is_none());
}

#[tokio::test]
async fn single_page_decodes_typed_records() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.listRecords"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "records": [
                {
                    "uri": "at://did:plc:alice/dev.idiolect.bounty/b1",
                    "cid": "bafyrei-b1",
                    "value": bounty_body(),
                },
                {
                    "uri": "at://did:plc:alice/dev.idiolect.bounty/b2",
                    "cid": "bafyrei-b2",
                    "value": bounty_body(),
                },
            ],
            "cursor": "next-page-cursor",
        })))
        .mount(&server)
        .await;

    let client = ReqwestPdsClient::with_service_url(server.uri());
    let fetcher = RecordFetcher::new(client);
    let page = fetcher
        .list_records::<Bounty>("did:plc:alice", None, None)
        .await
        .unwrap();
    assert_eq!(page.records.len(), 2);
    assert_eq!(page.records[0].uri, "at://did:plc:alice/dev.idiolect.bounty/b1");
    assert_eq!(page.records[0].cid, "bafyrei-b1");
    assert_eq!(page.records[0].record.status, Some(BountyStatus::Open));
    assert!(matches!(
        page.records[0].record.wants,
        BountyWants::WantAdapter(WantAdapter { .. })
    ));
    assert_eq!(page.cursor.as_deref(), Some("next-page-cursor"));
}

#[tokio::test]
async fn limit_and_cursor_params_are_forwarded() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.listRecords"))
        .and(query_param("limit", "25"))
        .and(query_param("cursor", "page-2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "records": [] })))
        .mount(&server)
        .await;

    let client = ReqwestPdsClient::with_service_url(server.uri());
    let fetcher = RecordFetcher::new(client);
    fetcher
        .list_records::<Bounty>("did:plc:alice", Some(25), Some("page-2"))
        .await
        .unwrap();
}

#[tokio::test]
async fn omitted_limit_and_cursor_are_not_forwarded() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.listRecords"))
        .and(query_param_is_missing("limit"))
        .and(query_param_is_missing("cursor"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "records": [] })))
        .mount(&server)
        .await;

    let client = ReqwestPdsClient::with_service_url(server.uri());
    let fetcher = RecordFetcher::new(client);
    fetcher
        .list_records::<Bounty>("did:plc:alice", None, None)
        .await
        .unwrap();
}

#[tokio::test]
async fn decode_error_names_the_failing_uri() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.listRecords"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "records": [
                {
                    "uri": "at://did:plc:alice/dev.idiolect.bounty/broken",
                    "cid": "bafyrei-broken",
                    // missing required fields for Bounty
                    "value": { "just": "wrong shape" },
                },
            ],
        })))
        .mount(&server)
        .await;

    let client = ReqwestPdsClient::with_service_url(server.uri());
    let fetcher = RecordFetcher::new(client);
    let err = fetcher
        .list_records::<Bounty>("did:plc:alice", None, None)
        .await
        .unwrap_err();
    match err {
        LensError::Transport(msg) => {
            assert!(msg.contains("broken"), "expected failing uri in error: {msg}");
        }
        other => panic!("expected Transport error, got {other:?}"),
    }
}
