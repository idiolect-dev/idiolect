//! Integration test for the atrium-backed [`PdsWriter`].
//!
//! Stands up a [`wiremock`] server, serves responses for
//! `com.atproto.repo.createRecord`, `putRecord`, and `deleteRecord`,
//! and drives them through [`AtriumPdsClient`]. Verifies:
//!
//! 1. Create returns the server-echoed `uri` + `cid` and sends the
//!    request body the PDS expects (with the record body, collection,
//!    repo, and optional rkey).
//! 2. Put round-trips `swap_record` verbatim.
//! 3. Delete succeeds with a json envelope body.
//! 4. Lexicon-shaped `InvalidSwap` errors collapse to
//!    [`LensError::Transport`] with the atrium message intact.
//!
//! Gated behind `required-features = ["pds-atrium"]` in Cargo.toml so
//! cargo only tries to build this test when the feature is on.

use idiolect_lens::{
    AtriumPdsClient, CreateRecordRequest, DeleteRecordRequest, LensError, PdsWriter,
    PutRecordRequest,
};
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Valid `CIDv1` over an empty block using raw multicodec — useful as
/// fixture output so atrium's CID parser accepts what the stub echoes.
const FIXTURE_CID: &str = "bafkreibme22gw2h7y2h7tg2fhbaxtpwgveaj5ilwaklsnpvzwvh3pqeala";

#[tokio::test(flavor = "current_thread")]
async fn create_record_returns_uri_and_cid() {
    let server = MockServer::start().await;
    let did = "did:plc:observer";
    let collection = "dev.idiolect.observation";

    let expected_uri = format!("at://{did}/{collection}/3l8");
    let stub_body = json!({
        "uri": expected_uri,
        "cid": FIXTURE_CID,
    });

    Mock::given(method("POST"))
        .and(path("/xrpc/com.atproto.repo.createRecord"))
        .and(body_partial_json(json!({
            "repo": did,
            "collection": collection,
            "record": {
                "$type": "dev.idiolect.observation",
                "observer": did,
            },
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(stub_body.clone()))
        .mount(&server)
        .await;

    let client = AtriumPdsClient::with_service_url(server.uri());

    let got = client
        .create_record(CreateRecordRequest {
            repo: did.to_owned(),
            collection: collection.to_owned(),
            rkey: None,
            record: json!({
                "$type": "dev.idiolect.observation",
                "observer": did,
                "occurredAt": "2026-04-20T10:00:00Z",
            }),
            validate: None,
        })
        .await
        .expect("create_record succeeds against stub");

    assert_eq!(got.uri, expected_uri);
    assert_eq!(got.cid, FIXTURE_CID);
}

#[tokio::test(flavor = "current_thread")]
async fn put_record_round_trips_swap_record() {
    let server = MockServer::start().await;
    let did = "did:plc:observer";
    let collection = "dev.idiolect.observation";
    let rkey = "3l8";
    let expected_uri = format!("at://{did}/{collection}/{rkey}");

    // the PDS echoes back the new cid; we don't care that it's the same
    // as the swap cid, only that the request carried `swapRecord`.
    Mock::given(method("POST"))
        .and(path("/xrpc/com.atproto.repo.putRecord"))
        .and(body_partial_json(json!({
            "repo": did,
            "collection": collection,
            "rkey": rkey,
            "swapRecord": FIXTURE_CID,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uri": expected_uri,
            "cid": FIXTURE_CID,
        })))
        .mount(&server)
        .await;

    let client = AtriumPdsClient::with_service_url(server.uri());

    let got = client
        .put_record(PutRecordRequest {
            repo: did.to_owned(),
            collection: collection.to_owned(),
            rkey: rkey.to_owned(),
            record: json!({
                "$type": "dev.idiolect.observation",
                "observer": did,
                "occurredAt": "2026-04-20T11:00:00Z",
            }),
            validate: Some(true),
            swap_record: Some(FIXTURE_CID.to_owned()),
        })
        .await
        .expect("put_record succeeds against stub");

    assert_eq!(got.uri, expected_uri);
    assert_eq!(got.cid, FIXTURE_CID);
}

#[tokio::test(flavor = "current_thread")]
async fn delete_record_succeeds() {
    let server = MockServer::start().await;
    let did = "did:plc:observer";
    let collection = "dev.idiolect.observation";
    let rkey = "3l8";

    Mock::given(method("POST"))
        .and(path("/xrpc/com.atproto.repo.deleteRecord"))
        .and(body_partial_json(json!({
            "repo": did,
            "collection": collection,
            "rkey": rkey,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;

    let client = AtriumPdsClient::with_service_url(server.uri());

    client
        .delete_record(DeleteRecordRequest {
            repo: did.to_owned(),
            collection: collection.to_owned(),
            rkey: rkey.to_owned(),
            swap_record: None,
        })
        .await
        .expect("delete_record succeeds against stub");
}

#[tokio::test(flavor = "current_thread")]
async fn create_invalid_swap_becomes_transport_error() {
    let server = MockServer::start().await;

    // createRecord's only typed error is InvalidSwap. the PDS
    // returns http 400 with the lexicon error envelope; atrium decodes
    // that and we translate it to LensError::Transport.
    Mock::given(method("POST"))
        .and(path("/xrpc/com.atproto.repo.createRecord"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "InvalidSwap",
            "message": "commit cid did not match",
        })))
        .mount(&server)
        .await;

    let client = AtriumPdsClient::with_service_url(server.uri());

    let err = client
        .create_record(CreateRecordRequest {
            repo: "did:plc:observer".to_owned(),
            collection: "dev.idiolect.observation".to_owned(),
            rkey: None,
            record: json!({ "$type": "dev.idiolect.observation" }),
            validate: None,
        })
        .await
        .expect_err("invalid swap should error");

    assert!(
        matches!(err, LensError::Transport(ref msg) if msg.contains("InvalidSwap")),
        "expected Transport(InvalidSwap...), got {err:?}"
    );
}
