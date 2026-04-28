//! End-to-end tests for the orchestrator's HTTP query API.
//!
//! Spins up the axum router on an ephemeral port, seeds the catalog
//! with fixture records, and exercises each endpoint with a live
//! reqwest client.

#![cfg(feature = "query-http")]

use std::sync::{Arc, Mutex};

use idiolect_orchestrator::{AppState, Catalog, http_router};
use idiolect_records::generated::dev::idiolect::adapter::{
    AdapterInvocationProtocol, AdapterInvocationProtocolKind, AdapterIsolation,
    AdapterIsolationKind,
};
use idiolect_records::generated::dev::idiolect::bounty::{BountyStatus, BountyWants, WantLens};
use idiolect_records::generated::dev::idiolect::defs::{LensRef, SchemaRef};
use idiolect_records::generated::dev::idiolect::verification::{
    VerificationKind, VerificationResult,
};
use idiolect_records::{Adapter, AnyRecord, Bounty, Community, Recommendation, Verification};

fn s_ref(uri: &str) -> SchemaRef {
    SchemaRef {
        cid: None,
        language: None,
        uri: Some(idiolect_records::AtUri::parse(uri).expect("valid at-uri")),
    }
}

fn l_ref(uri: &str) -> LensRef {
    LensRef {
        cid: None,
        direction: None,
        uri: Some(idiolect_records::AtUri::parse(uri).expect("valid at-uri")),
    }
}

fn adapter_for(framework: &str) -> Adapter {
    Adapter {
        author: idiolect_records::Did::parse("did:plc:author").expect("valid DID"),
        framework: framework.into(),
        invocation_protocol: AdapterInvocationProtocol {
            entry_point: Some("bin/x".into()),
            input_schema: None,
            kind: AdapterInvocationProtocolKind::Subprocess,
            output_schema: None,
        },
        isolation: AdapterIsolation {
            kind: AdapterIsolationKind::Process,
            filesystem_policy: None,
            network_policy: None,
            resource_limits: None,
        },
        occurred_at: idiolect_records::Datetime::parse("2026-04-21T00:00:00Z")
            .expect("valid datetime"),
        verification: None,
        version_range: ">=1 <2".into(),
    }
}

fn bounty_want_lens(src: &str, tgt: &str) -> Bounty {
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
        wants: BountyWants::WantLens(WantLens {
            bidirectional: None,
            source: s_ref(src),
            target: s_ref(tgt),
        }),
    }
}

fn community(members: &[&str]) -> Community {
    Community {
        conventions: None,
        conventions_text: None,
        core_lenses: None,
        core_schemas: None,
        created_at: idiolect_records::Datetime::parse("2026-04-21T00:00:00Z")
            .expect("valid datetime"),
        description: "c".into(),
        endorsed_communities: None,
        members: Some(
            members
                .iter()
                .map(|m| idiolect_records::Did::parse(m).expect("valid did"))
                .collect(),
        ),
        membership_roll: None,
        name: "Test".into(),
        appview_endpoint: None,
        member_role_vocab: None,
        record_hosting: None,
        role_assignments: None,
    }
}

fn recommendation_for(lens: &str) -> Recommendation {
    Recommendation {
        annotations: None,
        basis: None,
        caveats: None,
        caveats_text: None,
        conditions: vec![],
        issuing_community: idiolect_records::AtUri::parse(
            "at://did:plc:c/dev.idiolect.community/main",
        )
        .expect("valid at-uri"),
        lens_path: vec![l_ref(lens)],
        occurred_at: idiolect_records::Datetime::parse("2026-04-21T00:00:00Z")
            .expect("valid datetime"),
        preconditions: None,
        required_verifications: None,
        supersedes: None,
    }
}

fn verification_holds(lens_uri: &str, kind: VerificationKind) -> Verification {
    use idiolect_records::generated::dev::idiolect::defs::{
        LpChecker, LpCoercionLaw, LpConformance, LpConvergence, LpGenerator, LpRoundtrip,
        LpTheorem, Tool,
    };
    use idiolect_records::generated::dev::idiolect::verification::VerificationProperty;
    let property = match kind {
        VerificationKind::RoundtripTest => VerificationProperty::LpRoundtrip(LpRoundtrip {
            domain: "any".into(),
            generator: None,
        }),
        VerificationKind::PropertyTest => VerificationProperty::LpGenerator(LpGenerator {
            spec: "any".into(),
            runner: None,
            seed: None,
        }),
        VerificationKind::FormalProof => VerificationProperty::LpTheorem(LpTheorem {
            statement: "identity".into(),
            system: Some("coq".into()),
            free_variables: None,
        }),
        VerificationKind::ConformanceTest => VerificationProperty::LpConformance(LpConformance {
            standard: "any".into(),
            version: "1".into(),
            clauses: None,
        }),
        VerificationKind::StaticCheck => VerificationProperty::LpChecker(LpChecker {
            checker: "any".into(),
            ruleset: None,
            version: None,
        }),
        VerificationKind::ConvergencePreserving => {
            VerificationProperty::LpConvergence(LpConvergence {
                property: "any".into(),
                bound_steps: None,
            })
        }
        VerificationKind::CoercionLaw => VerificationProperty::LpCoercionLaw(LpCoercionLaw {
            standard: "any".into(),
            version: None,
            violation_threshold: None,
        }),
    };
    Verification {
        basis: None,
        counterexample: None,
        dependencies: None,
        kind,
        lens: l_ref(lens_uri),
        occurred_at: idiolect_records::Datetime::parse("2026-04-21T00:00:00Z")
            .expect("valid datetime"),
        proof_artifact: None,
        property,
        result: VerificationResult::Holds,
        tool: Tool {
            commit: None,
            name: "coq".into(),
            version: "8.18".into(),
        },
        verifier: idiolect_records::Did::parse("did:plc:v").expect("valid DID"),
    }
}

async fn serve_app(catalog: Arc<Mutex<Catalog>>) -> String {
    let app = http_router(AppState::ready(catalog));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn seed_catalog() -> Arc<Mutex<Catalog>> {
    let cat = Arc::new(Mutex::new(Catalog::new()));
    {
        let mut c = cat.lock().unwrap();
        c.upsert(
            "at://did:plc:x/dev.idiolect.bounty/b1".into(),
            "did:plc:alice".into(),
            "r".into(),
            AnyRecord::Bounty(bounty_want_lens(
                "at://did:plc:s/dev.panproto.schema.schema/a",
                "at://did:plc:s/dev.panproto.schema.schema/b",
            )),
        );
        c.upsert(
            "at://did:plc:x/dev.idiolect.adapter/a1".into(),
            "did:plc:author".into(),
            "r".into(),
            AnyRecord::Adapter(adapter_for("hasura")),
        );
        c.upsert(
            "at://did:plc:x/dev.idiolect.community/c1".into(),
            "did:plc:c".into(),
            "r".into(),
            AnyRecord::Community(community(&["did:plc:alice"])),
        );
        c.upsert(
            "at://did:plc:x/dev.idiolect.recommendation/r1".into(),
            "did:plc:c".into(),
            "r".into(),
            AnyRecord::Recommendation(recommendation_for(
                "at://did:plc:x/dev.panproto.schema.lens/l1",
            )),
        );
        c.upsert(
            "at://did:plc:x/dev.idiolect.verification/v1".into(),
            "did:plc:v".into(),
            "r".into(),
            AnyRecord::Verification(verification_holds(
                "at://did:plc:x/dev.panproto.schema.lens/l1",
                VerificationKind::FormalProof,
            )),
        );
        c.upsert(
            "at://did:plc:x/dev.idiolect.verification/v2".into(),
            "did:plc:v".into(),
            "r".into(),
            AnyRecord::Verification(verification_holds(
                "at://did:plc:x/dev.panproto.schema.lens/l1",
                VerificationKind::RoundtripTest,
            )),
        );
    }
    cat
}

#[tokio::test]
async fn healthz_returns_ok() {
    let cat = Arc::new(Mutex::new(Catalog::new()));
    let base = serve_app(cat).await;
    let body = reqwest::get(format!("{base}/healthz"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(body, "ok");
}

#[tokio::test]
async fn open_bounties_returns_seeded_bounty() {
    let base = serve_app(seed_catalog()).await;
    let body: serde_json::Value = reqwest::get(format!("{base}/v1/bounties/open"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        body["items"][0]["uri"],
        "at://did:plc:x/dev.idiolect.bounty/b1"
    );
}

#[tokio::test]
async fn xrpc_route_matches_rest_route() {
    let base = serve_app(seed_catalog()).await;
    let rest: serde_json::Value = reqwest::get(format!("{base}/v1/bounties/open"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let xrpc: serde_json::Value =
        reqwest::get(format!("{base}/xrpc/dev.idiolect.query.openBounties"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    assert_eq!(rest, xrpc);
}

#[tokio::test]
async fn bounties_want_lens_filters_by_schema_pair() {
    let base = serve_app(seed_catalog()).await;
    let matching: serde_json::Value = reqwest::get(format!(
        "{base}/v1/bounties/want-lens?source_uri=at://did:plc:s/dev.panproto.schema.schema/a&target_uri=at://did:plc:s/dev.panproto.schema.schema/b"
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(matching["items"].as_array().unwrap().len(), 1);

    let miss: serde_json::Value = reqwest::get(format!(
        "{base}/v1/bounties/want-lens?source_uri=at://did:plc:s/dev.panproto.schema.schema/x&target_uri=at://did:plc:s/dev.panproto.schema.schema/y"
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert!(miss["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn adapters_endpoint_matches_framework() {
    let base = serve_app(seed_catalog()).await;
    let body: serde_json::Value = reqwest::get(format!("{base}/v1/adapters?framework=hasura"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn recommendations_endpoint_returns_paths() {
    let base = serve_app(seed_catalog()).await;
    let body: serde_json::Value = reqwest::get(format!("{base}/v1/recommendations"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn verifications_endpoint_filters_by_lens() {
    let base = serve_app(seed_catalog()).await;
    let body: serde_json::Value = reqwest::get(format!(
        "{base}/v1/verifications?lens_uri=at://did:plc:x/dev.panproto.schema.lens/l1"
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn sufficient_returns_true_when_all_kinds_present() {
    let base = serve_app(seed_catalog()).await;
    let body: serde_json::Value = reqwest::get(format!(
        "{base}/v1/verifications/sufficient?lens_uri=at://did:plc:x/dev.panproto.schema.lens/l1&kinds=roundtrip-test,formal-proof&hold=true"
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(body["sufficient"], true);
    assert_eq!(body["require_holds"], true);
}

#[tokio::test]
async fn sufficient_returns_false_when_kind_missing() {
    let base = serve_app(seed_catalog()).await;
    let body: serde_json::Value = reqwest::get(format!(
        "{base}/v1/verifications/sufficient?lens_uri=at://did:plc:x/dev.panproto.schema.lens/l1&kinds=property-test"
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(body["sufficient"], false);
}

#[tokio::test]
async fn sufficient_returns_400_on_bad_kind() {
    let base = serve_app(seed_catalog()).await;
    let resp = reqwest::get(format!(
        "{base}/v1/verifications/sufficient?lens_uri=at://did:plc:x/dev.panproto.schema.lens/l1&kinds=teleport"
    ))
    .await
    .unwrap();
    // Structured error: 400 + {error, message} json body.
    assert_eq!(resp.status().as_u16(), 400);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "invalid_request");
    assert!(body["message"].as_str().unwrap().contains("teleport"));
}

#[tokio::test]
async fn pagination_limit_and_offset_slice_results() {
    // Seed a catalog with five open bounties so pagination math is
    // easy to verify.
    let catalog = Arc::new(Mutex::new(Catalog::new()));
    {
        let mut c = catalog.lock().unwrap();
        for i in 0..5 {
            c.upsert(
                format!("at://did:plc:x/dev.idiolect.bounty/b{i}"),
                "did:plc:alice".into(),
                "r".into(),
                AnyRecord::Bounty(bounty_want_lens(
                    "at://did:plc:s/dev.panproto.schema.schema/a",
                    &format!("at://did:plc:s/dev.panproto.schema.schema/t{i}"),
                )),
            );
        }
    }
    let base = serve_app(catalog).await;

    // limit=2, offset=1 -> 2 items, total=5.
    let body: serde_json::Value = reqwest::get(format!("{base}/v1/bounties/open?limit=2&offset=1"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_eq!(body["total"], 5);
    assert_eq!(body["limit"], 2);
    assert_eq!(body["offset"], 1);
}

#[tokio::test]
async fn pagination_offset_past_end_returns_empty_items() {
    let base = serve_app(seed_catalog()).await;
    let body: serde_json::Value = reqwest::get(format!("{base}/v1/bounties/open?offset=999"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(body["items"].as_array().unwrap().is_empty());
    // The seeded catalog has one open bounty; total reflects that
    // count regardless of offset.
    assert_eq!(body["total"], 1);
}

#[tokio::test]
async fn pagination_rejects_limit_zero_and_too_large() {
    let base = serve_app(seed_catalog()).await;
    let zero = reqwest::get(format!("{base}/v1/bounties/open?limit=0"))
        .await
        .unwrap();
    assert_eq!(zero.status().as_u16(), 400);
    let body: serde_json::Value = zero.json().await.unwrap();
    assert_eq!(body["error"], "invalid_request");

    let too_big = reqwest::get(format!("{base}/v1/bounties/open?limit=10001"))
        .await
        .unwrap();
    assert_eq!(too_big.status().as_u16(), 400);
}

#[tokio::test]
async fn readyz_returns_200_when_ready_and_503_when_not() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let catalog = Arc::new(Mutex::new(Catalog::new()));
    let ready = Arc::new(AtomicBool::new(false));
    let state = AppState::with_readiness(Arc::clone(&catalog), Arc::clone(&ready));
    let app = http_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let base = format!("http://{addr}");

    // Not ready yet: 503.
    let resp = reqwest::get(format!("{base}/readyz")).await.unwrap();
    assert_eq!(resp.status().as_u16(), 503);

    // Flip the flag and retry.
    ready.store(true, Ordering::SeqCst);
    let resp = reqwest::get(format!("{base}/readyz")).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
}

#[tokio::test]
async fn metrics_endpoint_exposes_prometheus_text() {
    let base = serve_app(seed_catalog()).await;
    let resp = reqwest::get(format!("{base}/metrics")).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    assert!(ct.starts_with("text/plain"), "got content-type: {ct}");
    let body = resp.text().await.unwrap();
    assert!(body.contains("idiolect_orchestrator_ready 1"));
    assert!(body.contains("kind=\"adapter\""));
    assert!(body.contains("kind=\"bounty\""));
    assert!(body.contains("idiolect_orchestrator_catalog_total"));
}

#[tokio::test]
async fn stats_endpoint_returns_catalog_counts() {
    let base = serve_app(seed_catalog()).await;
    let body: serde_json::Value = reqwest::get(format!("{base}/v1/stats"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["adapters"], 1);
    assert_eq!(body["bounties"], 1);
    assert_eq!(body["communities"], 1);
    assert_eq!(body["recommendations"], 1);
    assert_eq!(body["verifications"], 2);
}

#[tokio::test]
async fn bounties_by_requester_filters() {
    let base = serve_app(seed_catalog()).await;
    let hit: serde_json::Value = reqwest::get(format!(
        "{base}/v1/bounties/by-requester?requester_did=did:plc:alice"
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(hit["items"].as_array().unwrap().len(), 1);

    let miss: serde_json::Value = reqwest::get(format!(
        "{base}/v1/bounties/by-requester?requester_did=did:plc:unknown"
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert!(miss["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn adapters_by_invocation_protocol_filters() {
    let base = serve_app(seed_catalog()).await;
    let subprocess: serde_json::Value = reqwest::get(format!(
        "{base}/v1/adapters/by-invocation-protocol?kind=subprocess"
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(subprocess["items"].as_array().unwrap().len(), 1);

    let http: serde_json::Value = reqwest::get(format!(
        "{base}/v1/adapters/by-invocation-protocol?kind=http"
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert!(http["items"].as_array().unwrap().is_empty());

    // unknown kind -> 400 with structured error body
    let bad = reqwest::get(format!(
        "{base}/v1/adapters/by-invocation-protocol?kind=teleport"
    ))
    .await
    .unwrap();
    assert_eq!(bad.status().as_u16(), 400);
    let body: serde_json::Value = bad.json().await.unwrap();
    assert_eq!(body["error"], "invalid_request");
}

#[tokio::test]
async fn communities_by_name_filters_case_insensitively() {
    let base = serve_app(seed_catalog()).await;
    let hit: serde_json::Value = reqwest::get(format!("{base}/v1/communities/by-name?name=test"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(hit["items"].as_array().unwrap().len(), 1);

    let miss: serde_json::Value = reqwest::get(format!("{base}/v1/communities/by-name?name=nope"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(miss["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn verifications_by_kind_returns_matches() {
    let base = serve_app(seed_catalog()).await;
    let formal: serde_json::Value =
        reqwest::get(format!("{base}/v1/verifications/by-kind?kind=formal-proof"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    assert_eq!(formal["items"].as_array().unwrap().len(), 1);

    let none: serde_json::Value = reqwest::get(format!(
        "{base}/v1/verifications/by-kind?kind=property-test"
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert!(none["items"].as_array().unwrap().is_empty());

    let bad = reqwest::get(format!("{base}/v1/verifications/by-kind?kind=unknown"))
        .await
        .unwrap();
    assert_eq!(bad.status().as_u16(), 400);
    let body: serde_json::Value = bad.json().await.unwrap();
    assert_eq!(body["error"], "invalid_request");
}

#[tokio::test]
async fn dialects_for_community_filters() {
    // Add a dialect to the seeded catalog for testing.
    let catalog = seed_catalog();
    {
        let mut c = catalog.lock().unwrap();
        c.upsert(
            "at://did:plc:x/dev.idiolect.dialect/d1".into(),
            "did:plc:c".into(),
            "r".into(),
            idiolect_records::AnyRecord::Dialect(idiolect_records::Dialect {
                created_at: idiolect_records::Datetime::parse("2026-04-21T00:00:00Z")
                    .expect("valid datetime"),
                deprecations: None,
                description: None,
                idiolects: None,
                name: "d".into(),
                owning_community: idiolect_records::AtUri::parse(
                    "at://did:plc:x/dev.idiolect.community/c1",
                )
                .expect("valid at-uri"),
                preferred_lenses: None,
                previous_version: None,
                version: None,
            }),
        );
    }
    let base = serve_app(catalog).await;
    let hit: serde_json::Value = reqwest::get(format!(
        "{base}/v1/dialects/for-community?community_uri=at://did:plc:x/dev.idiolect.community/c1"
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(hit["items"].as_array().unwrap().len(), 1);

    let miss: serde_json::Value = reqwest::get(format!(
        "{base}/v1/dialects/for-community?community_uri=at://did:plc:other/dev.idiolect.community/main"
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert!(miss["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn communities_endpoint_filters_by_member() {
    let base = serve_app(seed_catalog()).await;
    let hit: serde_json::Value =
        reqwest::get(format!("{base}/v1/communities?member_did=did:plc:alice"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    assert_eq!(hit["items"].as_array().unwrap().len(), 1);

    let miss: serde_json::Value =
        reqwest::get(format!("{base}/v1/communities?member_did=did:plc:bob"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    assert!(miss["items"].as_array().unwrap().is_empty());
}
