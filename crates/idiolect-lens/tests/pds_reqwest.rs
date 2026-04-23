//! Integration tests for the reqwest-backed [`PdsClient`] / [`PdsWriter`].
//!
//! Stands up a [`wiremock`] server and drives the client directly.

#![cfg(feature = "pds-reqwest")]

use idiolect_lens::{
    CreateRecordRequest, DeleteRecordRequest, LensError, PdsClient, PdsWriter, PutRecordRequest,
    ReqwestPdsClient,
};
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn lens_body() -> serde_json::Value {
    json!({
        "createdAt": "2026-04-19T00:00:00.000Z",
        "objectHash": "sha256:deadbeef",
        "sourceSchema": "sha256:aaa",
        "targetSchema": "sha256:bbb"
    })
}

#[tokio::test]
async fn get_record_returns_value_on_2xx() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.getRecord"))
        .and(query_param("repo", "did:plc:x"))
        .and(query_param("collection", "dev.panproto.schema.lens"))
        .and(query_param("rkey", "l1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uri": "at://did:plc:x/dev.panproto.schema.lens/l1",
            "value": lens_body(),
        })))
        .mount(&server)
        .await;

    let client = ReqwestPdsClient::with_service_url(server.uri());
    let got = client
        .get_record("did:plc:x", "dev.panproto.schema.lens", "l1")
        .await
        .unwrap();
    assert_eq!(got, lens_body());
}

#[tokio::test]
async fn get_record_maps_record_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.getRecord"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "RecordNotFound",
            "message": "Could not locate record: at://did:plc:x/dev.panproto.schema.lens/l1",
        })))
        .mount(&server)
        .await;

    let client = ReqwestPdsClient::with_service_url(server.uri());
    let err = client
        .get_record("did:plc:x", "dev.panproto.schema.lens", "l1")
        .await
        .unwrap_err();
    assert!(matches!(err, LensError::NotFound(_)), "got {err:?}");
}

#[tokio::test]
async fn get_record_maps_generic_error_to_transport() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.getRecord"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "error": "InternalServerError",
            "message": "boom",
        })))
        .mount(&server)
        .await;

    let client = ReqwestPdsClient::with_service_url(server.uri());
    let err = client
        .get_record("did:plc:x", "dev.panproto.schema.lens", "l1")
        .await
        .unwrap_err();
    assert!(matches!(err, LensError::Transport(_)), "got {err:?}");
}

#[tokio::test]
async fn create_record_posts_json_and_returns_uri_cid() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/xrpc/com.atproto.repo.createRecord"))
        .and(body_partial_json(json!({
            "repo": "did:plc:x",
            "collection": "dev.idiolect.observation",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uri": "at://did:plc:x/dev.idiolect.observation/3l5",
            "cid": "bafyrei-created",
        })))
        .mount(&server)
        .await;

    let client = ReqwestPdsClient::with_service_url(server.uri());
    let resp = client
        .create_record(CreateRecordRequest {
            repo: "did:plc:x".to_owned(),
            collection: "dev.idiolect.observation".to_owned(),
            rkey: None,
            record: json!({ "$type": "dev.idiolect.observation" }),
            validate: None,
        })
        .await
        .unwrap();
    assert_eq!(resp.cid, "bafyrei-created");
    assert!(resp.uri.contains("dev.idiolect.observation"));
}

#[tokio::test]
async fn put_record_posts_json_and_returns_uri_cid() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/xrpc/com.atproto.repo.putRecord"))
        .and(body_partial_json(json!({
            "repo": "did:plc:x",
            "collection": "dev.idiolect.observation",
            "rkey": "3l5",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uri": "at://did:plc:x/dev.idiolect.observation/3l5",
            "cid": "bafyrei-put",
        })))
        .mount(&server)
        .await;

    let client = ReqwestPdsClient::with_service_url(server.uri());
    let resp = client
        .put_record(PutRecordRequest {
            repo: "did:plc:x".to_owned(),
            collection: "dev.idiolect.observation".to_owned(),
            rkey: "3l5".to_owned(),
            record: json!({}),
            validate: Some(true),
            swap_record: None,
        })
        .await
        .unwrap();
    assert_eq!(resp.cid, "bafyrei-put");
}

#[tokio::test]
async fn delete_record_succeeds_on_2xx() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/xrpc/com.atproto.repo.deleteRecord"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;

    let client = ReqwestPdsClient::with_service_url(server.uri());
    client
        .delete_record(DeleteRecordRequest {
            repo: "did:plc:x".to_owned(),
            collection: "dev.idiolect.observation".to_owned(),
            rkey: "3l5".to_owned(),
            swap_record: None,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn delete_record_surfaces_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/xrpc/com.atproto.repo.deleteRecord"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "InvalidSwap",
            "message": "mismatched swap",
        })))
        .mount(&server)
        .await;

    let client = ReqwestPdsClient::with_service_url(server.uri());
    let err = client
        .delete_record(DeleteRecordRequest {
            repo: "did:plc:x".to_owned(),
            collection: "dev.idiolect.observation".to_owned(),
            rkey: "3l5".to_owned(),
            swap_record: Some("bafyrei-stale".to_owned()),
        })
        .await
        .unwrap_err();
    assert!(matches!(err, LensError::Transport(_)));
}

#[tokio::test]
async fn publisher_and_fetcher_round_trip_via_wiremock() {
    use idiolect_lens::{RecordFetcher, RecordPublisher};
    use idiolect_records::generated::bounty::{Bounty, BountyStatus, BountyWants, WantAdapter};

    let server = MockServer::start().await;

    // Publisher: expect POST to createRecord with our body.
    Mock::given(method("POST"))
        .and(path("/xrpc/com.atproto.repo.createRecord"))
        .and(body_partial_json(serde_json::json!({
            "repo": "did:plc:alice",
            "collection": "dev.idiolect.bounty",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "uri": "at://did:plc:alice/dev.idiolect.bounty/3l5",
            "cid": "bafyrei-ok",
        })))
        .mount(&server)
        .await;

    // Fetcher: expect GET to getRecord returning the envelope.
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.getRecord"))
        .and(query_param("repo", "did:plc:alice"))
        .and(query_param("collection", "dev.idiolect.bounty"))
        .and(query_param("rkey", "3l5"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "uri": "at://did:plc:alice/dev.idiolect.bounty/3l5",
            "value": {
                "constraints": [],
                "occurredAt": "2026-04-21T00:00:00Z",
                "requester": "did:plc:alice",
                "status": "open",
                "wants": {
                    "$type": "dev.idiolect.bounty#wantAdapter",
                    "framework": "hasura"
                }
            },
        })))
        .mount(&server)
        .await;

    let client = ReqwestPdsClient::with_service_url(server.uri());
    let publisher = RecordPublisher::new(client.clone(), "did:plc:alice".to_owned());
    let fetcher = RecordFetcher::new(client);

    let bounty = Bounty {
        basis: None,
        constraints: None,
        eligibility: None,
        fulfillment: None,
        occurred_at: "2026-04-21T00:00:00Z".into(),
        requester: "did:plc:alice".into(),
        reward: None,
        status: Some(BountyStatus::Open),
        wants: BountyWants::WantAdapter(WantAdapter {
            framework: "hasura".into(),
            version_range: None,
        }),
    };

    let resp = publisher.create(&bounty).await.unwrap();
    assert_eq!(resp.cid, "bafyrei-ok");

    let retrieved: Bounty = fetcher.fetch("did:plc:alice", "3l5").await.unwrap();
    assert_eq!(retrieved.requester, bounty.requester);
    assert_eq!(retrieved.status, Some(BountyStatus::Open));
}

#[tokio::test]
async fn trailing_slash_in_base_url_is_tolerated() {
    // extra trailing slash should not produce a double-slashed URL.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.getRecord"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uri": "at://did:plc:x/c/r",
            "value": { "x": 1 },
        })))
        .mount(&server)
        .await;
    let base = format!("{}/", server.uri());
    let client = ReqwestPdsClient::with_service_url(base);
    let got = client.get_record("did:plc:x", "c", "r").await.unwrap();
    assert_eq!(got, json!({"x": 1}));
}
