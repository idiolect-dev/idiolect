//! Unit tests for the three additional reference
//! [`ObservationMethod`](idiolect_observer::ObservationMethod) impls:
//! `encounter-throughput`, `verification-coverage`, and `lens-adoption`.
//!
//! Each test builds a small in-memory firehose, folds the events
//! through the method directly (no driver involved), and asserts the
//! snapshot's shape.

use idiolect_indexer::{IndexerAction, IndexerEvent};
use idiolect_observer::{
    EncounterThroughputMethod, LensAdoptionMethod, ObservationMethod, VerificationCoverageMethod,
};
use idiolect_records::AnyRecord;
use idiolect_records::generated::correction::{Correction, CorrectionReason};
use idiolect_records::generated::defs::{LensRef, SchemaRef, Visibility};
use idiolect_records::generated::encounter::{Encounter, EncounterDownstreamResult, EncounterKind};
use idiolect_records::generated::verification::{
    Verification, VerificationKind, VerificationResult,
};

fn ix_event(seq: u64, did: &str, record: AnyRecord) -> IndexerEvent {
    IndexerEvent {
        seq,
        live: true,
        did: did.to_owned(),
        rev: format!("3l{seq}"),
        collection: match &record {
            AnyRecord::Correction(_) => "dev.idiolect.correction",
            AnyRecord::Verification(_) => "dev.idiolect.verification",
            _ => "dev.idiolect.encounter",
        }
        .to_owned(),
        rkey: format!("k{seq}"),
        action: IndexerAction::Create,
        cid: Some(format!("bafyrei{seq}")),
        record: Some(record),
    }
}

fn encounter(
    kind: EncounterKind,
    result: Option<EncounterDownstreamResult>,
    lens: &str,
) -> AnyRecord {
    AnyRecord::Encounter(Encounter {
        annotations: None,
        downstream_result: result,
        kind,
        lens: LensRef {
            uri: Some(lens.to_owned()),
            cid: None,
            direction: None,
        },
        occurred_at: "2026-04-20T10:00:00Z".to_owned(),
        produced_output: None,
        purpose: idiolect_records::generated::defs::Purpose {
            action: "test".to_owned(),
            material: None,
            actor: None,
            vocabulary: None,
        },
        source_instance: None,
        source_schema: SchemaRef {
            uri: Some("at://did:plc:x/dev.panproto.schema.schema/s".to_owned()),
            cid: None,
            language: None,
        },
        target_schema: None,
        visibility: Visibility::PublicDetailed,
    })
}

fn correction(encounter_uri: &str) -> AnyRecord {
    AnyRecord::Correction(Correction {
        encounter: idiolect_records::generated::defs::EncounterRef {
            uri: encounter_uri.to_owned(),
            cid: None,
        },
        corrected_value: None,
        original_value: None,
        occurred_at: "2026-04-20T10:05:00Z".to_owned(),
        path: "/text".to_owned(),
        rationale: None,
        reason: CorrectionReason::LensError,
        visibility: Visibility::PublicDetailed,
    })
}

fn verification(
    lens: &str,
    verifier: &str,
    kind: VerificationKind,
    result: VerificationResult,
) -> AnyRecord {
    AnyRecord::Verification(Verification {
        counterexample: None,
        dependencies: None,
        property: idiolect_records::generated::verification::VerificationProperty::LpTheorem(
            idiolect_records::generated::defs::LpTheorem {
                statement: "test".to_owned(),
                system: None,
                free_variables: None,
            },
        ),
        kind,
        lens: LensRef {
            uri: Some(lens.to_owned()),
            cid: None,
            direction: None,
        },
        occurred_at: "2026-04-20T11:00:00Z".to_owned(),
        proof_artifact: None,
        result,
        tool: idiolect_records::generated::defs::Tool {
            name: "coq".to_owned(),
            version: "8.18".to_owned(),
            commit: None,
        },
        verifier: verifier.to_owned(),
    })
}

#[test]
fn throughput_none_when_empty() {
    let m = EncounterThroughputMethod::new();
    assert!(m.snapshot().unwrap().is_none());
    assert_eq!(m.total(), 0);
}

#[test]
fn throughput_counts_by_kind_and_result() {
    let mut m = EncounterThroughputMethod::new();
    let lens_a = "at://did:plc:x/dev.panproto.schema.lens/a";

    m.observe(&ix_event(
        1,
        "did:plc:a",
        encounter(
            EncounterKind::InvocationLog,
            Some(EncounterDownstreamResult::Success),
            lens_a,
        ),
    ))
    .unwrap();
    m.observe(&ix_event(
        2,
        "did:plc:b",
        encounter(
            EncounterKind::Production,
            Some(EncounterDownstreamResult::Corrected),
            lens_a,
        ),
    ))
    .unwrap();
    m.observe(&ix_event(
        3,
        "did:plc:c",
        encounter(EncounterKind::Production, None, lens_a),
    ))
    .unwrap();
    // Non-encounter record is ignored.
    m.observe(&ix_event(
        4,
        "did:plc:d",
        correction("at://did:plc:a/dev.idiolect.encounter/1"),
    ))
    .unwrap();

    assert_eq!(m.total(), 3);
    let snap = m.snapshot().unwrap().unwrap();
    assert_eq!(snap["total"], 3);
    assert_eq!(snap["byKind"]["invocation-log"], 1);
    assert_eq!(snap["byKind"]["production"], 2);
    assert_eq!(snap["byDownstreamResult"]["success"], 1);
    assert_eq!(snap["byDownstreamResult"]["corrected"], 1);
    assert_eq!(snap["byDownstreamResult"]["none"], 1);
}

#[test]
fn throughput_deletes_have_no_body() {
    // An event with no record body (delete) is a no-op.
    let mut m = EncounterThroughputMethod::new();
    let mut evt = ix_event(
        1,
        "did:plc:a",
        encounter(EncounterKind::InvocationLog, None, "at://x/lens/a"),
    );
    evt.record = None;
    evt.action = IndexerAction::Delete;
    m.observe(&evt).unwrap();
    assert!(m.snapshot().unwrap().is_none());
}

#[test]
fn verification_coverage_groups_by_lens() {
    let mut m = VerificationCoverageMethod::new();
    let lens_a = "at://did:plc:x/dev.panproto.schema.lens/a";
    let lens_b = "at://did:plc:x/dev.panproto.schema.lens/b";

    m.observe(&ix_event(
        1,
        "did:plc:v1",
        verification(
            lens_a,
            "did:plc:v1",
            VerificationKind::FormalProof,
            VerificationResult::Holds,
        ),
    ))
    .unwrap();
    m.observe(&ix_event(
        2,
        "did:plc:v2",
        verification(
            lens_a,
            "did:plc:v2",
            VerificationKind::PropertyTest,
            VerificationResult::Holds,
        ),
    ))
    .unwrap();
    m.observe(&ix_event(
        3,
        "did:plc:v1",
        verification(
            lens_a,
            "did:plc:v1",
            VerificationKind::StaticCheck,
            VerificationResult::Falsified,
        ),
    ))
    .unwrap();
    m.observe(&ix_event(
        4,
        "did:plc:v3",
        verification(
            lens_b,
            "did:plc:v3",
            VerificationKind::RoundtripTest,
            VerificationResult::Inconclusive,
        ),
    ))
    .unwrap();

    let snap = m.snapshot().unwrap().unwrap();
    let lenses = snap["lenses"].as_object().unwrap();
    assert_eq!(lenses[lens_a]["total"], 3);
    assert_eq!(lenses[lens_a]["distinctVerifiers"], 2);
    assert_eq!(lenses[lens_a]["byKind"]["formal-proof"], 1);
    assert_eq!(lenses[lens_a]["byKind"]["property-test"], 1);
    assert_eq!(lenses[lens_a]["byKind"]["static-check"], 1);
    assert_eq!(lenses[lens_a]["byResult"]["holds"], 2);
    assert_eq!(lenses[lens_a]["byResult"]["falsified"], 1);
    assert_eq!(lenses[lens_b]["total"], 1);
    assert_eq!(lenses[lens_b]["distinctVerifiers"], 1);
    assert_eq!(lenses[lens_b]["byResult"]["inconclusive"], 1);
}

#[test]
fn verification_coverage_ignores_non_verifications() {
    let mut m = VerificationCoverageMethod::new();
    m.observe(&ix_event(
        1,
        "did:plc:a",
        encounter(EncounterKind::InvocationLog, None, "at://x/lens/a"),
    ))
    .unwrap();
    assert!(m.snapshot().unwrap().is_none());
}

#[test]
fn adoption_tracks_distinct_invokers() {
    let mut m = LensAdoptionMethod::new();
    let lens_a = "at://did:plc:x/dev.panproto.schema.lens/a";
    let lens_b = "at://did:plc:x/dev.panproto.schema.lens/b";

    m.observe(&ix_event(
        1,
        "did:plc:alice",
        encounter(EncounterKind::InvocationLog, None, lens_a),
    ))
    .unwrap();
    m.observe(&ix_event(
        2,
        "did:plc:alice",
        encounter(EncounterKind::Production, None, lens_a),
    ))
    .unwrap();
    m.observe(&ix_event(
        3,
        "did:plc:bob",
        encounter(EncounterKind::InvocationLog, None, lens_a),
    ))
    .unwrap();
    m.observe(&ix_event(
        4,
        "did:plc:alice",
        encounter(EncounterKind::InvocationLog, None, lens_b),
    ))
    .unwrap();

    let snap = m.snapshot().unwrap().unwrap();
    let lenses = snap["lenses"].as_object().unwrap();
    assert_eq!(lenses[lens_a]["encounters"], 3);
    assert_eq!(lenses[lens_a]["distinctInvokers"], 2);
    assert_eq!(lenses[lens_b]["encounters"], 1);
    assert_eq!(lenses[lens_b]["distinctInvokers"], 1);
}

#[test]
fn adoption_ignores_non_encounters() {
    let mut m = LensAdoptionMethod::new();
    m.observe(&ix_event(
        1,
        "did:plc:a",
        correction("at://did:plc:a/dev.idiolect.encounter/1"),
    ))
    .unwrap();
    assert!(m.snapshot().unwrap().is_none());
}

#[test]
fn all_methods_have_stable_descriptors() {
    // names and versions are public API: callers compare observations
    // by method name + version, so the crate's contract is that the
    // string constants match the exported API.
    let m1 = EncounterThroughputMethod::new();
    assert_eq!(m1.name(), "encounter-throughput");
    assert_eq!(m1.version(), "1.0.0");

    let m2 = VerificationCoverageMethod::new();
    assert_eq!(m2.name(), "verification-coverage");
    assert_eq!(m2.version(), "1.0.0");

    let m3 = LensAdoptionMethod::new();
    assert_eq!(m3.name(), "lens-adoption");
    assert_eq!(m3.version(), "1.0.0");
}
