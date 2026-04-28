//! End-to-end test: DID → identity resolve → PDS → publisher/fetcher.
//! Exercises `publisher_for_did` and `fetcher_for_did`.

#![cfg(feature = "pds-resolve")]

use idiolect_identity::{Did, DidDocument, InMemoryIdentityResolver, Service};
use idiolect_lens::{fetcher_for_did, publisher_for_did};
use idiolect_records::generated::dev::idiolect::bounty::{
    Bounty, BountyStatus, BountyWants, WantAdapter,
};
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture_bounty() -> Bounty {
    Bounty {
        basis: None,
        constraints: None,
        eligibility: None,
        fulfillment: None,
        occurred_at: idiolect_records::Datetime::parse("2026-04-21T00:00:00Z")
            .expect("valid datetime"),
        requester: idiolect_records::Did::parse("did:plc:alice").expect("valid DID"),
        reward: None,
        status: Some(BountyStatus::Open),
        status_vocab: None,
        wants: BountyWants::WantAdapter(WantAdapter {
            framework: "hasura".into(),
            version_range: None,
        }),
    }
}

fn fixture_bounty_json() -> serde_json::Value {
    json!({
        "constraints": [],
        "occurredAt": "2026-04-21T00:00:00Z",
        "requester": "did:plc:alice",
        "status": "open",
        "wants": {
            "$type": "dev.idiolect.bounty#wantAdapter",
            "framework": "hasura",
        },
    })
}

fn in_memory_resolver_pointing_at(did: &str, pds_url: &str) -> InMemoryIdentityResolver {
    let parsed = Did::parse(did).unwrap();
    let mut r = InMemoryIdentityResolver::new();
    r.insert(
        &parsed,
        DidDocument {
            id: did.to_owned(),
            also_known_as: vec!["at://alice.test".into()],
            service: vec![Service {
                id: "#atproto_pds".into(),
                service_type: "AtprotoPersonalDataServer".into(),
                service_endpoint: pds_url.to_owned(),
            }],
            verification_method: vec![],
            extras: serde_json::Map::default(),
        },
    );
    r
}

#[tokio::test]
async fn publisher_for_did_writes_to_resolved_pds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/xrpc/com.atproto.repo.createRecord"))
        .and(body_partial_json(json!({
            "repo": "did:plc:alice",
            "collection": "dev.idiolect.bounty",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uri": "at://did:plc:alice/dev.idiolect.bounty/3l5",
            "cid": "bafyrei-ok",
        })))
        .mount(&server)
        .await;

    let resolver = in_memory_resolver_pointing_at("did:plc:alice", &server.uri());
    let did = Did::parse("did:plc:alice").unwrap();
    let publisher = publisher_for_did(&resolver, &did).await.unwrap();
    let resp = publisher.create(&fixture_bounty()).await.unwrap();
    assert_eq!(resp.cid, "bafyrei-ok");
    assert_eq!(publisher.repo(), "did:plc:alice");
}

#[tokio::test]
async fn fetcher_for_did_reads_from_resolved_pds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.getRecord"))
        .and(query_param("repo", "did:plc:alice"))
        .and(query_param("collection", "dev.idiolect.bounty"))
        .and(query_param("rkey", "b1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uri": "at://did:plc:alice/dev.idiolect.bounty/b1",
            "value": fixture_bounty_json(),
        })))
        .mount(&server)
        .await;

    let resolver = in_memory_resolver_pointing_at("did:plc:alice", &server.uri());
    let did = Did::parse("did:plc:alice").unwrap();
    let fetcher = fetcher_for_did(&resolver, &did).await.unwrap();
    let got: Bounty = fetcher.fetch("did:plc:alice", "b1").await.unwrap();
    assert_eq!(got.requester.as_str(), "did:plc:alice");
    assert_eq!(got.status, Some(BountyStatus::Open));
}

#[tokio::test]
async fn publisher_for_did_surfaces_identity_errors() {
    // Empty resolver; DID not found.
    let resolver = InMemoryIdentityResolver::new();
    let did = Did::parse("did:plc:alice").unwrap();
    let err = publisher_for_did(&resolver, &did).await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("did:plc:alice"), "got: {msg}");
}
