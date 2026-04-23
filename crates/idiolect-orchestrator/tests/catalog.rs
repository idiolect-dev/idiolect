//! End-to-end tests for catalog ingest + query surface.
//!
//! Builds a small in-memory firehose populated with bounties,
//! adapters, communities, dialects, recommendations, and
//! verifications; drives it through the orchestrator's
//! [`CatalogHandler`]; and exercises every query in [`query`].

use std::sync::Arc;

use idiolect_indexer::{
    InMemoryCursorStore, InMemoryEventStream, IndexerAction, IndexerConfig, IndexerError,
    IndexerEvent, RawEvent, RecordHandler, drive_indexer,
};
use idiolect_orchestrator::{CatalogHandler, query};
use idiolect_records::generated::bounty::{BountyStatus, BountyWants, WantAdapter, WantLens};
use idiolect_records::generated::defs::{LensRef, SchemaRef};
use idiolect_records::generated::recommendation::RecommendationRequiredVerifications;
use idiolect_records::generated::verification::{VerificationKind, VerificationResult};
use idiolect_records::{
    Adapter, AnyRecord, Bounty, Community, Dialect, Recommendation, Verification,
};

fn s_ref(uri: &str) -> SchemaRef {
    SchemaRef {
        cid: None,
        language: None,
        uri: Some(uri.to_owned()),
    }
}

fn l_ref(uri: &str) -> LensRef {
    LensRef {
        cid: None,
        direction: None,
        uri: Some(uri.to_owned()),
    }
}

#[allow(clippy::needless_pass_by_value)]
fn ev(
    seq: u64,
    did: &str,
    collection: &str,
    rkey: &str,
    record: AnyRecord,
    action: IndexerAction,
) -> RawEvent {
    // Build via AnyRecord re-serialize so the body mirrors what the
    // firehose would actually carry.
    let body = match action {
        IndexerAction::Delete => None,
        _ => Some(any_to_json(&record)),
    };
    RawEvent {
        seq,
        live: true,
        did: did.to_owned(),
        rev: format!("rev{seq}"),
        collection: collection.to_owned(),
        rkey: rkey.to_owned(),
        action,
        cid: Some(format!("bafyrei{seq}")),
        body,
    }
}

fn any_to_json(record: &AnyRecord) -> serde_json::Value {
    match record {
        AnyRecord::Adapter(a) => serde_json::to_value(a).unwrap(),
        AnyRecord::Bounty(b) => serde_json::to_value(b).unwrap(),
        AnyRecord::Community(c) => serde_json::to_value(c).unwrap(),
        AnyRecord::Dialect(d) => serde_json::to_value(d).unwrap(),
        AnyRecord::Recommendation(r) => serde_json::to_value(r).unwrap(),
        AnyRecord::Verification(v) => serde_json::to_value(v).unwrap(),
        other => panic!("unexpected record kind in test fixture: {other:?}"),
    }
}

fn bounty_want_lens(status: Option<BountyStatus>, src: &str, tgt: &str) -> Bounty {
    Bounty {
        constraints: None,
        eligibility: None,
        fulfillment: None,
        occurred_at: "2026-04-20T00:00:00Z".to_owned(),
        requester: "did:plc:alice".to_owned(),
        reward: None,
        status,
        wants: BountyWants::WantLens(WantLens {
            bidirectional: None,
            source: s_ref(src),
            target: s_ref(tgt),
        }),
    }
}

fn bounty_want_adapter(fw: &str) -> Bounty {
    Bounty {
        constraints: None,
        eligibility: None,
        fulfillment: None,
        occurred_at: "2026-04-20T00:00:00Z".to_owned(),
        requester: "did:plc:alice".to_owned(),
        reward: None,
        status: Some(BountyStatus::Open),
        wants: BountyWants::WantAdapter(WantAdapter {
            framework: fw.to_owned(),
            version_range: None,
        }),
    }
}

fn adapter(framework: &str) -> Adapter {
    use idiolect_records::generated::adapter::{
        AdapterInvocationProtocol, AdapterInvocationProtocolKind, AdapterIsolation,
        AdapterIsolationKind,
    };
    Adapter {
        author: "did:plc:author".to_owned(),
        framework: framework.to_owned(),
        invocation_protocol: AdapterInvocationProtocol {
            entry_point: Some("bin/adapter".to_owned()),
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
        occurred_at: "2026-04-20T00:00:00Z".to_owned(),
        verification: None,
        version_range: ">=1.0 <2.0".to_owned(),
    }
}

fn community(members: &[&str]) -> Community {
    Community {
        conventions: None,
        conventions_text: None,
        core_lenses: None,
        core_schemas: None,
        created_at: "2026-04-20T00:00:00Z".to_owned(),
        description: "c".to_owned(),
        endorsed_communities: None,
        members: Some(members.iter().map(|m| (*m).to_owned()).collect()),
        membership_roll: None,
        name: "Test".to_owned(),
    }
}

fn dialect(preferred: &[&str]) -> Dialect {
    Dialect {
        created_at: "2026-04-20T00:00:00Z".to_owned(),
        deprecations: None,
        description: None,
        idiolects: None,
        name: "d".to_owned(),
        owning_community: "at://did:plc:c/dev.idiolect.community/main".to_owned(),
        preferred_lenses: Some(preferred.iter().map(|u| l_ref(u)).collect()),
        previous_version: None,
        version: None,
    }
}

fn recommendation(path: &[&str]) -> Recommendation {
    use idiolect_records::generated::defs::{LpRoundtrip, LpTheorem};
    Recommendation {
        annotations: None,
        caveats: None,
        caveats_text: None,
        conditions: Vec::new(),
        issuing_community: "at://did:plc:c/dev.idiolect.community/main".to_owned(),
        lens_path: path.iter().map(|u| l_ref(u)).collect(),
        occurred_at: "2026-04-20T00:00:00Z".to_owned(),
        preconditions: None,
        required_verifications: Some(vec![
            RecommendationRequiredVerifications::LpRoundtrip(LpRoundtrip {
                domain: String::new(),
                generator: None,
            }),
            RecommendationRequiredVerifications::LpTheorem(LpTheorem {
                statement: String::new(),
                system: None,
                free_variables: None,
            }),
        ]),
        supersedes: None,
    }
}

fn verification(
    lens_uri: &str,
    kind: VerificationKind,
    result: VerificationResult,
) -> Verification {
    use idiolect_records::generated::defs::{LpTheorem, Tool};
    use idiolect_records::generated::verification::VerificationProperty;
    Verification {
        counterexample: None,
        dependencies: None,
        property: VerificationProperty::LpTheorem(LpTheorem {
            statement: "identity".to_owned(),
            system: Some("coq".to_owned()),
            free_variables: None,
        }),
        kind,
        lens: l_ref(lens_uri),
        occurred_at: "2026-04-20T00:00:00Z".to_owned(),
        proof_artifact: None,
        result,
        tool: Tool {
            commit: None,
            name: "coq".to_owned(),
            version: "8.18".to_owned(),
        },
        verifier: "did:plc:v".to_owned(),
    }
}

#[tokio::test]
async fn catalog_ingests_every_supported_kind() {
    let mut stream = InMemoryEventStream::new();
    stream.push(ev(
        1,
        "did:plc:author",
        "dev.idiolect.adapter",
        "a1",
        AnyRecord::Adapter(adapter("hasura")),
        IndexerAction::Create,
    ));
    stream.push(ev(
        2,
        "did:plc:alice",
        "dev.idiolect.bounty",
        "b1",
        AnyRecord::Bounty(bounty_want_lens(
            Some(BountyStatus::Open),
            "at://did:plc:x/dev.panproto.schema.schema/a",
            "at://did:plc:x/dev.panproto.schema.schema/b",
        )),
        IndexerAction::Create,
    ));
    stream.push(ev(
        3,
        "did:plc:c",
        "dev.idiolect.community",
        "main",
        AnyRecord::Community(community(&["did:plc:alice", "did:plc:bob"])),
        IndexerAction::Create,
    ));
    stream.push(ev(
        4,
        "did:plc:c",
        "dev.idiolect.dialect",
        "v1",
        AnyRecord::Dialect(dialect(&["at://did:plc:x/dev.panproto.schema.lens/l1"])),
        IndexerAction::Create,
    ));
    stream.push(ev(
        5,
        "did:plc:c",
        "dev.idiolect.recommendation",
        "r1",
        AnyRecord::Recommendation(recommendation(&[
            "at://did:plc:x/dev.panproto.schema.lens/l1",
        ])),
        IndexerAction::Create,
    ));
    stream.push(ev(
        6,
        "did:plc:v",
        "dev.idiolect.verification",
        "v1",
        AnyRecord::Verification(verification(
            "at://did:plc:x/dev.panproto.schema.lens/l1",
            VerificationKind::RoundtripTest,
            VerificationResult::Holds,
        )),
        IndexerAction::Create,
    ));

    let handler = CatalogHandler::new();
    let cat = handler.catalog();
    let cursors = InMemoryCursorStore::new();
    drive_indexer(&mut stream, &handler, &cursors, &IndexerConfig::default())
        .await
        .unwrap();

    let catalog = cat.lock().unwrap();
    assert_eq!(catalog.len(), 6);
    assert_eq!(catalog.adapters().count(), 1);
    assert_eq!(catalog.bounties().count(), 1);
    assert_eq!(catalog.communities().count(), 1);
    assert_eq!(catalog.dialects().count(), 1);
    assert_eq!(catalog.recommendations().count(), 1);
    assert_eq!(catalog.verifications().count(), 1);
}

#[tokio::test]
async fn delete_removes_record() {
    let handler = CatalogHandler::new();
    let cat = handler.catalog();

    // create
    let create = IndexerEvent {
        seq: 1,
        live: true,
        did: "did:plc:author".to_owned(),
        rev: "r1".to_owned(),
        collection: "dev.idiolect.adapter".to_owned(),
        rkey: "a1".to_owned(),
        action: IndexerAction::Create,
        cid: None,
        record: Some(AnyRecord::Adapter(adapter("prisma"))),
    };
    RecordHandler::handle(&handler, &create).await.unwrap();
    assert_eq!(cat.lock().unwrap().adapters().count(), 1);

    // delete
    let del = IndexerEvent {
        action: IndexerAction::Delete,
        record: None,
        ..create.clone()
    };
    RecordHandler::handle(&handler, &del).await.unwrap();
    assert_eq!(cat.lock().unwrap().adapters().count(), 0);
}

#[tokio::test]
async fn catalog_ignores_non_orchestrator_records() {
    // An encounter shouldn't land in the catalog.
    use idiolect_records::generated::defs::Purpose;
    use idiolect_records::generated::defs::Visibility;
    use idiolect_records::generated::encounter::{Encounter, EncounterKind};
    let handler = CatalogHandler::new();
    let cat = handler.catalog();
    let encounter = Encounter {
        annotations: None,
        downstream_result: None,
        kind: EncounterKind::InvocationLog,
        lens: l_ref("at://did:plc:x/dev.panproto.schema.lens/l1"),
        occurred_at: "2026-04-20T00:00:00Z".to_owned(),
        produced_output: None,
        purpose: Purpose {
            action: "t".to_owned(),
            material: None,
            actor: None,
            vocabulary: None,
        },
        source_instance: None,
        source_schema: s_ref("at://did:plc:x/dev.panproto.schema.schema/a"),
        target_schema: None,
        visibility: Visibility::PublicDetailed,
    };
    let event = IndexerEvent {
        seq: 1,
        live: true,
        did: "did:plc:a".to_owned(),
        rev: "r1".to_owned(),
        collection: "dev.idiolect.encounter".to_owned(),
        rkey: "e1".to_owned(),
        action: IndexerAction::Create,
        cid: None,
        record: Some(AnyRecord::Encounter(encounter)),
    };
    RecordHandler::handle(&handler, &event).await.unwrap();
    assert_eq!(cat.lock().unwrap().len(), 0);
}

#[tokio::test]
async fn update_replaces_existing_slot() {
    let handler = CatalogHandler::new();
    let cat = handler.catalog();
    let mut event = IndexerEvent {
        seq: 1,
        live: true,
        did: "did:plc:author".to_owned(),
        rev: "r1".to_owned(),
        collection: "dev.idiolect.adapter".to_owned(),
        rkey: "a1".to_owned(),
        action: IndexerAction::Create,
        cid: None,
        record: Some(AnyRecord::Adapter(adapter("hasura"))),
    };
    RecordHandler::handle(&handler, &event).await.unwrap();
    event.action = IndexerAction::Update;
    event.record = Some(AnyRecord::Adapter(adapter("prisma")));
    event.rev = "r2".to_owned();
    RecordHandler::handle(&handler, &event).await.unwrap();

    let catalog = cat.lock().unwrap();
    assert_eq!(catalog.adapters().count(), 1);
    let a = catalog.adapters().next().unwrap();
    assert_eq!(a.record.framework, "prisma");
    assert_eq!(a.rev, "r2");
}

fn seed_catalog() -> Arc<std::sync::Mutex<idiolect_orchestrator::Catalog>> {
    let handler = CatalogHandler::new();
    let cat = handler.catalog();
    {
        let mut c = cat.lock().unwrap();
        c.upsert(
            "at://did:plc:x/dev.idiolect.bounty/open1".to_owned(),
            "did:plc:a".to_owned(),
            "r".to_owned(),
            AnyRecord::Bounty(bounty_want_lens(
                Some(BountyStatus::Open),
                "at://s/a",
                "at://s/b",
            )),
        );
        c.upsert(
            "at://did:plc:x/dev.idiolect.bounty/fulfilled1".to_owned(),
            "did:plc:a".to_owned(),
            "r".to_owned(),
            AnyRecord::Bounty(Bounty {
                status: Some(BountyStatus::Fulfilled),
                ..bounty_want_lens(None, "at://s/a", "at://s/b")
            }),
        );
        c.upsert(
            "at://did:plc:x/dev.idiolect.bounty/adapter1".to_owned(),
            "did:plc:a".to_owned(),
            "r".to_owned(),
            AnyRecord::Bounty(bounty_want_adapter("prisma")),
        );
        c.upsert(
            "at://did:plc:x/dev.idiolect.adapter/a-hasura".to_owned(),
            "did:plc:a".to_owned(),
            "r".to_owned(),
            AnyRecord::Adapter(adapter("hasura")),
        );
        c.upsert(
            "at://did:plc:x/dev.idiolect.adapter/a-hasura-lc".to_owned(),
            "did:plc:a".to_owned(),
            "r".to_owned(),
            AnyRecord::Adapter(adapter("HASURA")),
        );
        c.upsert(
            "at://did:plc:x/dev.idiolect.community/c1".to_owned(),
            "did:plc:c".to_owned(),
            "r".to_owned(),
            AnyRecord::Community(community(&["did:plc:alice", "did:plc:bob"])),
        );
        c.upsert(
            "at://did:plc:x/dev.idiolect.dialect/d1".to_owned(),
            "did:plc:c".to_owned(),
            "r".to_owned(),
            AnyRecord::Dialect(dialect(&["at://did:plc:x/dev.panproto.schema.lens/l1"])),
        );
        c.upsert(
            "at://did:plc:x/dev.idiolect.recommendation/r1".to_owned(),
            "did:plc:c".to_owned(),
            "r".to_owned(),
            AnyRecord::Recommendation(recommendation(&[
                "at://did:plc:x/dev.panproto.schema.lens/l1",
            ])),
        );
        c.upsert(
            "at://did:plc:x/dev.idiolect.verification/v-rt".to_owned(),
            "did:plc:v".to_owned(),
            "r".to_owned(),
            AnyRecord::Verification(verification(
                "at://did:plc:x/dev.panproto.schema.lens/l1",
                VerificationKind::RoundtripTest,
                VerificationResult::Holds,
            )),
        );
        c.upsert(
            "at://did:plc:x/dev.idiolect.verification/v-fp".to_owned(),
            "did:plc:v".to_owned(),
            "r".to_owned(),
            AnyRecord::Verification(verification(
                "at://did:plc:x/dev.panproto.schema.lens/l1",
                VerificationKind::FormalProof,
                VerificationResult::Holds,
            )),
        );
        c.upsert(
            "at://did:plc:x/dev.idiolect.verification/v-falsified".to_owned(),
            "did:plc:v".to_owned(),
            "r".to_owned(),
            AnyRecord::Verification(verification(
                "at://did:plc:x/dev.panproto.schema.lens/l-bad",
                VerificationKind::RoundtripTest,
                VerificationResult::Falsified,
            )),
        );
    }
    cat
}

#[test]
fn query_open_bounties_filters_out_fulfilled() {
    let cat = seed_catalog();
    let catalog = cat.lock().unwrap();
    let open = query::open_bounties(&catalog);
    // 2 opens (one WantLens, one WantAdapter) + 0 fulfilled.
    assert_eq!(open.len(), 2);
}

#[test]
fn query_bounties_for_want_lens_matches_schema_pair() {
    let cat = seed_catalog();
    let catalog = cat.lock().unwrap();
    let got = query::bounties_for_want_lens(&catalog, &s_ref("at://s/a"), &s_ref("at://s/b"));
    assert_eq!(got.len(), 1);
    // Non-matching pair returns empty.
    let got = query::bounties_for_want_lens(&catalog, &s_ref("at://s/x"), &s_ref("at://s/y"));
    assert!(got.is_empty());
}

#[test]
fn query_adapters_for_framework_case_insensitive() {
    let cat = seed_catalog();
    let catalog = cat.lock().unwrap();
    let got = query::adapters_for_framework(&catalog, "hasura");
    assert_eq!(got.len(), 2);
    let got = query::adapters_for_framework(&catalog, "Prisma");
    assert!(got.is_empty());
}

#[test]
fn query_preferred_lenses_for_dialect() {
    let d = dialect(&["at://x/l1", "at://x/l2"]);
    let lenses = query::preferred_lenses_for_dialect(&d);
    assert_eq!(lenses.len(), 2);
}

#[test]
fn query_sufficient_verifications_returns_true_when_all_kinds_hold() {
    use idiolect_records::generated::defs::{LpGenerator, LpRoundtrip, LpTheorem};
    let cat = seed_catalog();
    let catalog = cat.lock().unwrap();
    let lens = l_ref("at://did:plc:x/dev.panproto.schema.lens/l1");
    let rt = || {
        RecommendationRequiredVerifications::LpRoundtrip(LpRoundtrip {
            domain: String::new(),
            generator: None,
        })
    };
    let fp = || {
        RecommendationRequiredVerifications::LpTheorem(LpTheorem {
            statement: String::new(),
            system: None,
            free_variables: None,
        })
    };
    let pt = || {
        RecommendationRequiredVerifications::LpGenerator(LpGenerator {
            spec: String::new(),
            runner: None,
            seed: None,
        })
    };
    let required = vec![rt(), fp()];
    assert!(query::sufficient_verifications_for(
        &catalog, &lens, &required, true
    ));
    let req_plus = vec![rt(), fp(), pt()];
    assert!(!query::sufficient_verifications_for(
        &catalog, &lens, &req_plus, true
    ));
}

#[test]
fn query_sufficient_verifications_excludes_falsified_when_holding_required() {
    use idiolect_records::generated::defs::LpRoundtrip;
    let cat = seed_catalog();
    let catalog = cat.lock().unwrap();
    let bad_lens = l_ref("at://did:plc:x/dev.panproto.schema.lens/l-bad");
    let required = vec![RecommendationRequiredVerifications::LpRoundtrip(
        LpRoundtrip {
            domain: String::new(),
            generator: None,
        },
    )];
    assert!(!query::sufficient_verifications_for(
        &catalog, &bad_lens, &required, true
    ));
    assert!(query::sufficient_verifications_for(
        &catalog, &bad_lens, &required, false
    ));
}

#[test]
fn query_sufficient_verifications_empty_required_is_vacuously_true() {
    let cat = seed_catalog();
    let catalog = cat.lock().unwrap();
    let lens = l_ref("at://any/lens");
    assert!(query::sufficient_verifications_for(
        &catalog,
        &lens,
        &[],
        true
    ));
}

#[test]
fn query_communities_for_member() {
    let cat = seed_catalog();
    let catalog = cat.lock().unwrap();
    let got = query::communities_for_member(&catalog, "did:plc:alice");
    assert_eq!(got.len(), 1);
    let got = query::communities_for_member(&catalog, "did:plc:not-a-member");
    assert!(got.is_empty());
}

#[test]
fn query_recommendations_starting_from_returns_non_empty_paths() {
    let cat = seed_catalog();
    let catalog = cat.lock().unwrap();
    let got = query::recommendations_starting_from(&catalog);
    assert_eq!(got.len(), 1);
}

#[test]
fn catalog_find_hits_every_variant() {
    let cat = seed_catalog();
    let catalog = cat.lock().unwrap();
    // Each inserted URI must find its corresponding variant.
    for uri in [
        "at://did:plc:x/dev.idiolect.bounty/open1",
        "at://did:plc:x/dev.idiolect.adapter/a-hasura",
        "at://did:plc:x/dev.idiolect.community/c1",
        "at://did:plc:x/dev.idiolect.dialect/d1",
        "at://did:plc:x/dev.idiolect.recommendation/r1",
        "at://did:plc:x/dev.idiolect.verification/v-rt",
    ] {
        let got = catalog.find(uri);
        assert!(got.is_some(), "expected hit for {uri}");
    }
}

#[test]
fn catalog_len_matches_per_kind_counts() {
    let cat = seed_catalog();
    let catalog = cat.lock().unwrap();
    let expected = catalog.adapters().count()
        + catalog.bounties().count()
        + catalog.communities().count()
        + catalog.dialects().count()
        + catalog.recommendations().count()
        + catalog.verifications().count();
    assert_eq!(catalog.len(), expected);
    assert!(!catalog.is_empty());
}

#[test]
#[allow(clippy::many_single_char_names)]
fn schema_refs_match_prefers_cid_over_uri() {
    use idiolect_records::generated::defs::SchemaRef;
    let a = SchemaRef {
        cid: Some("cid-a".into()),
        language: None,
        uri: Some("at://a".into()),
    };
    let b = SchemaRef {
        cid: Some("cid-a".into()),
        language: None,
        uri: Some("at://b".into()),
    };
    assert!(query::schema_refs_match(&a, &b));
    let c = SchemaRef {
        cid: Some("cid-other".into()),
        language: None,
        uri: Some("at://a".into()),
    };
    assert!(!query::schema_refs_match(&a, &c));
    // Falls back to uri match when neither has a cid.
    let d = SchemaRef {
        cid: None,
        language: None,
        uri: Some("at://shared".into()),
    };
    let e = SchemaRef {
        cid: None,
        language: None,
        uri: Some("at://shared".into()),
    };
    assert!(query::schema_refs_match(&d, &e));
    // Two refs with nothing in common don't match.
    let f = SchemaRef {
        cid: None,
        language: None,
        uri: None,
    };
    let g = SchemaRef {
        cid: None,
        language: None,
        uri: None,
    };
    assert!(!query::schema_refs_match(&f, &g));
}

#[test]
fn catalog_find_returns_polymorphic_ref() {
    let cat = seed_catalog();
    let catalog = cat.lock().unwrap();
    match catalog.find("at://did:plc:x/dev.idiolect.adapter/a-hasura") {
        Some(idiolect_orchestrator::CatalogRef::Adapter(e)) => {
            assert_eq!(e.record.framework, "hasura");
        }
        other => panic!("unexpected: {other:?}"),
    }
    assert!(catalog.find("at://no/such/uri").is_none());
}

#[test]
fn upsert_across_kinds_does_not_leak_old_slot() {
    // Formally impossible per atproto URI conventions, but defensive:
    // if a URI is inserted under one kind and then again under a
    // different kind, only the latter should survive. Regression
    // guard for a real-world buggy upstream we have no control over.
    let handler = CatalogHandler::new();
    let cat = handler.catalog();
    let uri = "at://did:plc:x/dev.idiolect.adapter/same".to_owned();
    {
        let mut c = cat.lock().unwrap();
        c.upsert(
            uri.clone(),
            "did:plc:a".into(),
            "r1".into(),
            AnyRecord::Adapter(adapter("hasura")),
        );
        // Re-upsert the same URI with a different kind.
        c.upsert(
            uri,
            "did:plc:a".into(),
            "r2".into(),
            AnyRecord::Bounty(bounty_want_lens(
                Some(BountyStatus::Open),
                "at://s/a",
                "at://s/b",
            )),
        );
        assert_eq!(c.adapters().count(), 0, "old adapter slot must be vacated");
        assert_eq!(
            c.bounties().count(),
            1,
            "bounty slot should hold the new value"
        );
        assert_eq!(c.len(), 1, "total catalog size reflects one record");
    }
}

#[test]
fn query_bounties_by_requester_ignores_status() {
    let cat = seed_catalog();
    let catalog = cat.lock().unwrap();
    // All three seed bounties have requester = "did:plc:alice".
    let got = query::bounties_by_requester(&catalog, "did:plc:alice");
    assert_eq!(got.len(), 3);
    // Absent DID returns empty.
    assert!(query::bounties_by_requester(&catalog, "did:plc:no-one").is_empty());
}

#[test]
fn query_adapters_by_invocation_protocol() {
    use idiolect_records::generated::adapter::AdapterInvocationProtocolKind;
    let cat = seed_catalog();
    let catalog = cat.lock().unwrap();
    let subprocess = query::adapters_by_invocation_protocol(
        &catalog,
        &AdapterInvocationProtocolKind::Subprocess,
    );
    assert!(subprocess.len() >= 2);
    let http =
        query::adapters_by_invocation_protocol(&catalog, &AdapterInvocationProtocolKind::Http);
    assert!(http.is_empty());
}

#[test]
fn query_communities_by_name() {
    let cat = seed_catalog();
    let catalog = cat.lock().unwrap();
    // Seeded community is named "Test"; case-insensitive match.
    assert_eq!(query::communities_by_name(&catalog, "test").len(), 1);
    assert_eq!(query::communities_by_name(&catalog, "TEST").len(), 1);
    assert_eq!(query::communities_by_name(&catalog, "other").len(), 0);
}

#[test]
fn query_verifications_by_kind() {
    use idiolect_records::generated::verification::VerificationKind;
    let cat = seed_catalog();
    let catalog = cat.lock().unwrap();
    // Seeded verifications: one roundtrip-test (holds) + one formal-proof (holds) + one roundtrip-test (falsified).
    let rt = query::verifications_by_kind(&catalog, &VerificationKind::RoundtripTest);
    assert_eq!(rt.len(), 2);
    let fp = query::verifications_by_kind(&catalog, &VerificationKind::FormalProof);
    assert_eq!(fp.len(), 1);
    let pt = query::verifications_by_kind(&catalog, &VerificationKind::PropertyTest);
    assert!(pt.is_empty());
}

#[test]
fn query_dialects_for_community_filters_on_owning_community() {
    let cat = seed_catalog();
    let catalog = cat.lock().unwrap();
    // Seed catalog has one dialect owned by "at://did:plc:c/dev.idiolect.community/main"
    let got = query::dialects_for_community(&catalog, "at://did:plc:c/dev.idiolect.community/main");
    assert_eq!(got.len(), 1);
    let miss = query::dialects_for_community(&catalog, "at://absent");
    assert!(miss.is_empty());
}

#[test]
fn query_catalog_stats_sums_correctly() {
    let cat = seed_catalog();
    let catalog = cat.lock().unwrap();
    let stats = query::catalog_stats(&catalog);
    assert_eq!(stats.total(), catalog.len());
    // seeded: 3 bounties + 2 adapters + 1 community + 1 dialect + 1 recommendation + 3 verifications
    assert_eq!(stats.bounties, 3);
    assert_eq!(stats.adapters, 2);
    assert_eq!(stats.communities, 1);
    assert_eq!(stats.dialects, 1);
    assert_eq!(stats.recommendations, 1);
    assert_eq!(stats.verifications, 3);
}

#[test]
fn lens_refs_match_prefers_cid() {
    let a = LensRef {
        cid: Some("cid-a".to_owned()),
        direction: None,
        uri: Some("at://a".to_owned()),
    };
    let b = LensRef {
        cid: Some("cid-a".to_owned()),
        direction: None,
        uri: Some("at://b".to_owned()),
    };
    assert!(query::lens_refs_match(&a, &b));
    let c = LensRef {
        cid: Some("cid-other".to_owned()),
        direction: None,
        uri: Some("at://a".to_owned()),
    };
    assert!(!query::lens_refs_match(&a, &c));
}

#[tokio::test]
async fn handler_error_surfaces_if_mutex_poisoned() {
    // Simulate a poisoned mutex by panicking inside a lock held on
    // another thread.
    use std::sync::Mutex;
    let cat = Arc::new(Mutex::new(idiolect_orchestrator::Catalog::new()));
    let poisoner = Arc::clone(&cat);
    let _ = std::thread::spawn(move || {
        let _g = poisoner.lock().unwrap();
        panic!("poison");
    })
    .join();

    let handler = CatalogHandler::with_catalog(cat);
    let event = IndexerEvent {
        seq: 1,
        live: true,
        did: "did:plc:x".to_owned(),
        rev: "r".to_owned(),
        collection: "dev.idiolect.adapter".to_owned(),
        rkey: "a".to_owned(),
        action: IndexerAction::Create,
        cid: None,
        record: Some(AnyRecord::Adapter(adapter("z"))),
    };
    let err = RecordHandler::handle(&handler, &event).await.unwrap_err();
    assert!(matches!(err, IndexerError::Handler(_)));
}
