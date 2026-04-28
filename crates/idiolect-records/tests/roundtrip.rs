//! Round-trip tests for the generated record types.
//!
//! These pin the serde contract: every fixture must deserialize from
//! wire-shape json, re-serialize, and match the original json. We
//! compare on `serde_json::Value` equality rather than byte-for-byte
//! because `serde_json` may reorder object keys.

use idiolect_records::generated::dev::idiolect::{
    bounty, defs::Visibility, encounter::EncounterKind,
};
use idiolect_records::{
    Adapter, Bounty, Correction, Encounter, Observation, Retrospection, Verification,
};
use serde_json::json;

fn roundtrip<T: serde::Serialize + serde::de::DeserializeOwned>(value: &serde_json::Value) -> T {
    let parsed: T = serde_json::from_value(value.clone()).expect("deserialize");
    let reserialized = serde_json::to_value(&parsed).expect("serialize");
    // every field in the fixture must round-trip. extra fields from
    // skip-serializing-if-none defaults are fine, but anything in the
    // fixture must appear in the re-serialized output with the same
    // value.
    assert_subset(value, &reserialized);
    parsed
}

fn assert_subset(expected: &serde_json::Value, actual: &serde_json::Value) {
    match (expected, actual) {
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            for (k, v) in a {
                let found = b.get(k).unwrap_or_else(|| panic!("missing key {k}"));
                assert_subset(v, found);
            }
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "array length");
            for (x, y) in a.iter().zip(b.iter()) {
                assert_subset(x, y);
            }
        }
        _ => assert_eq!(expected, actual),
    }
}

#[test]
fn encounter_minimal() {
    let parsed: Encounter = roundtrip(&json!({
        "lens":         { "uri": "at://did:plc:example/dev.idiolect.lens/abc" },
        "sourceSchema": { "uri": "at://did:plc:example/dev.idiolect.schema/src" },
        "use":          { "action": "translate_source_to_target" },
        "kind":         "invocation-log",
        "visibility":   "public-detailed",
        "occurredAt":   "2026-04-19T00:00:00.000Z",
    }));
    assert_eq!(parsed.kind, EncounterKind::InvocationLog);
    assert_eq!(parsed.visibility, Visibility::PublicDetailed);
}

#[test]
fn correction_minimal() {
    let _parsed: Correction = roundtrip(&json!({
        "encounter":   { "uri": "at://did:plc:example/dev.idiolect.encounter/abc" },
        "path":        "/foo/bar",
        "reason":      "lens-error",
        "visibility":  "public-detailed",
        "occurredAt":  "2026-04-19T00:00:00.000Z",
    }));
}

#[test]
fn verification_minimal() {
    let _parsed: Verification = roundtrip(&json!({
        "lens":       { "uri": "at://did:plc:example/dev.idiolect.lens/abc" },
        "kind":       "roundtrip-test",
        "verifier":   "did:plc:verifier",
        "tool":       { "name": "nextest", "version": "0.9.87" },
        "property": {
            "$type":  "dev.idiolect.defs#lpRoundtrip",
            "domain": "all valid records",
        },
        "result":     "holds",
        "occurredAt": "2026-04-19T00:00:00.000Z",
    }));
}

#[test]
fn adapter_minimal() {
    let parsed: Adapter = roundtrip(&json!({
        "framework":          "coq",
        "versionRange":       "^8.20",
        "invocationProtocol": { "kind": "subprocess" },
        "isolation":          { "kind": "process" },
        "author":             "did:plc:adapter-author",
        "occurredAt":         "2026-04-19T00:00:00.000Z",
    }));
    assert_eq!(parsed.framework, "coq");
}

#[test]
fn bounty_with_want_lens_variant() {
    let parsed: Bounty = roundtrip(&json!({
        "requester": "did:plc:requester",
        "wants": {
            "$type":  "dev.idiolect.bounty#wantLens",
            "source": { "language": "postgres-sql" },
            "target": { "language": "atproto-lexicon" },
        },
        "constraints": [],
        "occurredAt":  "2026-04-19T00:00:00.000Z",
    }));
    match parsed.wants {
        bounty::BountyWants::WantLens(_) => {}
        other => panic!("expected WantLens variant, got {other:?}"),
    }
}

#[test]
fn observation_minimal() {
    let _parsed: Observation = roundtrip(&json!({
        "observer":   "did:plc:observer",
        "method":     { "name": "weighted-correction-rate" },
        "scope":      { "encounterKinds": ["production"] },
        "output":     { "lenses": [] },
        "version":    "1.0.0",
        "visibility": "public-detailed",
        "occurredAt": "2026-04-19T00:00:00.000Z",
    }));
}

#[test]
fn retrospection_minimal() {
    let _parsed: Retrospection = roundtrip(&json!({
        "encounter":      { "uri": "at://did:plc:example/dev.idiolect.encounter/abc" },
        "finding":        { "kind": "merge-divergence", "detail": "left branch lost a record" },
        "detectingParty": "did:plc:detector",
        "detectedAt":     "2026-04-19T06:00:00.000Z",
        "occurredAt":     "2026-04-19T06:30:00.000Z",
    }));
}

#[test]
fn unknown_closed_enum_value_rejected() {
    // closed enums (here: visibility) reject unknown values at
    // deserialization. open enums (knownValues + Other(String)) do
    // not — that asymmetry is the point of the open-enum convention.
    let bad = json!({
        "lens":         { "uri": "at://did:plc:x/dev.panproto.schema.lens/z" },
        "sourceSchema": { "uri": "at://did:plc:x/dev.panproto.schema.schema/a" },
        "use":          { "action": "p" },
        "kind":         "invocation-log",
        "visibility":   "not-a-real-visibility",
        "occurredAt":   "2026-04-19T00:00:00.000Z",
    });
    assert!(serde_json::from_value::<Encounter>(bad).is_err());
}

#[test]
fn unknown_open_enum_value_accepted_as_other() {
    // open enums round-trip community-extended slugs through the
    // Other(String) variant. This is wire-compatible with future
    // additions to knownValues and lets downstream vocabs extend
    // without forking the lexicon.
    use idiolect_records::generated::dev::idiolect::encounter::EncounterKind;
    let parsed: Encounter = roundtrip(&json!({
        "lens":         { "uri": "at://did:plc:x/dev.panproto.schema.lens/z" },
        "sourceSchema": { "uri": "at://did:plc:x/dev.panproto.schema.schema/a" },
        "use":          { "action": "p" },
        "kind":         "community-extended-kind",
        "visibility":   "public-detailed",
        "occurredAt":   "2026-04-19T00:00:00.000Z",
    }));
    assert_eq!(
        parsed.kind,
        EncounterKind::Other("community-extended-kind".to_owned())
    );
}

// --- New lexicon round-trips (deliberation, community shape additions, vocab graph) ---

#[test]
fn deliberation_minimal_roundtrip() {
    use idiolect_records::Deliberation;
    let _parsed: Deliberation = roundtrip(&json!({
        "owningCommunity": "at://did:plc:c/dev.idiolect.community/main",
        "topic":           "Should we adopt verification policy v2?",
        "createdAt":       "2026-04-28T00:00:00.000Z",
    }));
}

#[test]
fn deliberation_with_open_enums_roundtrips() {
    use idiolect_records::Deliberation;
    use idiolect_records::generated::dev::idiolect::deliberation::{
        DeliberationClassification, DeliberationStatus,
    };
    let parsed: Deliberation = roundtrip(&json!({
        "owningCommunity": "at://did:plc:c/dev.idiolect.community/main",
        "topic":           "Adopt v2?",
        "description":     "Long-form context",
        "authRequired":    true,
        "classification":  "proposal",
        "status":          "open",
        "createdAt":       "2026-04-28T00:00:00.000Z",
    }));
    assert_eq!(
        parsed.classification,
        Some(DeliberationClassification::Proposal)
    );
    assert_eq!(parsed.status, Some(DeliberationStatus::Open));
}

#[test]
fn deliberation_open_enum_extension_lands_in_other() {
    use idiolect_records::Deliberation;
    use idiolect_records::generated::dev::idiolect::deliberation::DeliberationClassification;
    let parsed: Deliberation = roundtrip(&json!({
        "owningCommunity": "at://did:plc:c/dev.idiolect.community/main",
        "topic":           "x",
        "classification":  "community-extended-kind",
        "createdAt":       "2026-04-28T00:00:00.000Z",
    }));
    assert_eq!(
        parsed.classification,
        Some(DeliberationClassification::Other(
            "community-extended-kind".to_owned()
        ))
    );
}

#[test]
fn deliberation_statement_roundtrip() {
    use idiolect_records::DeliberationStatement;
    let _parsed: DeliberationStatement = roundtrip(&json!({
        "deliberation": {
            "uri": "at://did:plc:c/dev.idiolect.deliberation/d1",
            "cid": "bafyreidfcm4u3vnuph5ltwdpssiz3a4xfbm2otjrdisftwnbfmnxd6lsxm",
        },
        "text":           "We should require formal proofs for v2",
        "classification": "proposal",
        "createdAt":      "2026-04-28T00:00:00.000Z",
    }));
}

#[test]
fn deliberation_vote_three_way_roundtrip() {
    use idiolect_records::DeliberationVote;
    use idiolect_records::generated::dev::idiolect::deliberation_vote::DeliberationVoteStance;
    for slug in ["agree", "pass", "disagree"] {
        let parsed: DeliberationVote = roundtrip(&json!({
            "subject": {
                "uri": "at://did:plc:c/dev.idiolect.deliberationStatement/s1",
                "cid": "bafyreidfcm4u3vnuph5ltwdpssiz3a4xfbm2otjrdisftwnbfmnxd6lsxm",
            },
            "stance":    slug,
            "createdAt": "2026-04-28T00:00:00.000Z",
        }));
        let expected = match slug {
            "agree" => DeliberationVoteStance::Agree,
            "pass" => DeliberationVoteStance::Pass,
            "disagree" => DeliberationVoteStance::Disagree,
            _ => unreachable!(),
        };
        assert_eq!(parsed.stance, expected);
    }
}

#[test]
fn deliberation_vote_with_weight_and_rationale_roundtrips() {
    use idiolect_records::DeliberationVote;
    let parsed: DeliberationVote = roundtrip(&json!({
        "subject": {
            "uri": "at://did:plc:c/dev.idiolect.deliberationStatement/s1",
            "cid": "bafyreidfcm4u3vnuph5ltwdpssiz3a4xfbm2otjrdisftwnbfmnxd6lsxm",
        },
        "stance":    "agree",
        "weight":    750,
        "rationale": "matches our prior policy",
        "createdAt": "2026-04-28T00:00:00.000Z",
    }));
    assert_eq!(parsed.weight, Some(750));
    assert_eq!(parsed.rationale.as_deref(), Some("matches our prior policy"));
}

#[test]
fn deliberation_outcome_roundtrip() {
    use idiolect_records::DeliberationOutcome;
    let _parsed: DeliberationOutcome = roundtrip(&json!({
        "deliberation": {
            "uri": "at://did:plc:c/dev.idiolect.deliberation/d1",
            "cid": "bafyreidfcm4u3vnuph5ltwdpssiz3a4xfbm2otjrdisftwnbfmnxd6lsxm",
        },
        "statementTallies": [
            {
                "statement": {
                    "uri": "at://did:plc:c/dev.idiolect.deliberationStatement/s1",
                    "cid": "bafyreidfcm4u3vnuph5ltwdpssiz3a4xfbm2otjrdisftwnbfmnxd6lsxm",
                },
                "counts": [
                    { "stance": "agree", "count": 12 },
                    { "stance": "disagree", "count": 3 },
                ],
            },
        ],
        "computedAt": "2026-04-28T01:00:00.000Z",
        "occurredAt": "2026-04-28T01:00:00.000Z",
    }));
}

#[test]
fn community_with_role_assignments_roundtrips() {
    use idiolect_records::Community;
    use idiolect_records::generated::dev::idiolect::community::RoleAssignmentRole;
    let parsed: Community = roundtrip(&json!({
        "name":        "Test community",
        "description": "x",
        "members":     ["did:plc:alice", "did:plc:bob"],
        "roleAssignments": [
            { "did": "did:plc:alice", "role": "moderator" },
            { "did": "did:plc:bob",   "role": "delegate" },
        ],
        "recordHosting":   "community-hosted",
        "appviewEndpoint": "https://example.community",
        "createdAt":       "2026-04-28T00:00:00.000Z",
    }));
    let assignments = parsed
        .role_assignments
        .expect("roleAssignments deserializes");
    assert_eq!(assignments.len(), 2);
    assert_eq!(
        assignments[0].role,
        RoleAssignmentRole::Moderator
    );
    assert_eq!(parsed.appview_endpoint.as_deref(), Some("https://example.community"));
}

#[test]
fn vocab_legacy_tree_roundtrip() {
    use idiolect_records::Vocab;
    let _parsed: Vocab = roundtrip(&json!({
        "name":  "tree-vocab",
        "world": "closed-with-default",
        "top":   "any",
        "actions": [
            { "id": "any",   "parents": [] },
            { "id": "child", "parents": ["any"] },
        ],
        "occurredAt": "2026-04-28T00:00:00.000Z",
    }));
}

#[test]
fn vocab_graph_shape_roundtrip() {
    use idiolect_records::Vocab;
    use idiolect_records::vocab::VocabGraph;
    let parsed: Vocab = roundtrip(&json!({
        "name":  "graph-vocab",
        "world": "closed-with-default",
        "nodes": [
            { "id": "any",   "kind": "concept", "label": "Any" },
            { "id": "child", "kind": "concept" },
            {
                "id":   "subsumed_by",
                "kind": "relation",
                "relationMetadata": { "transitive": true, "reflexive": true },
            },
        ],
        "edges": [
            { "source": "child", "target": "any", "relationSlug": "subsumed_by" },
        ],
        "occurredAt": "2026-04-28T00:00:00.000Z",
    }));
    let g = VocabGraph::from_vocab(&parsed);
    assert!(g.is_subsumed_by("child", "any"));
    assert_eq!(g.top(), Some("any".to_owned()));
}

#[test]
fn adapter_open_enum_extension_lands_in_other() {
    use idiolect_records::Adapter;
    use idiolect_records::generated::dev::idiolect::adapter::AdapterInvocationProtocolKind;
    let parsed: Adapter = roundtrip(&json!({
        "framework":          "future-runtime",
        "versionRange":       "*",
        "invocationProtocol": { "kind": "grpc" },
        "isolation":          { "kind": "process" },
        "author":             "did:plc:author",
        "occurredAt":         "2026-04-28T00:00:00.000Z",
    }));
    assert_eq!(
        parsed.invocation_protocol.kind,
        AdapterInvocationProtocolKind::Other("grpc".to_owned())
    );
}

#[test]
fn open_enum_with_underscored_slug_roundtrips() {
    // The codegen preserves the original slug form when emitting
    // `as_str` arms; underscored slugs like `subsumed_by` must
    // serialize back as `subsumed_by`, not as `subsumed-by`.
    use idiolect_records::Vocab;
    use idiolect_records::generated::dev::idiolect::vocab::VocabEdgeRelationSlug;
    let parsed: Vocab = roundtrip(&json!({
        "name":  "g",
        "world": "open",
        "nodes": [
            { "id": "a" },
            { "id": "b" },
        ],
        "edges": [
            { "source": "a", "target": "b", "relationSlug": "subsumed_by" },
            { "source": "b", "target": "a", "relationSlug": "equivalent_to" },
        ],
        "occurredAt": "2026-04-28T00:00:00.000Z",
    }));
    let edges = parsed.edges.as_ref().expect("edges");
    assert_eq!(edges[0].relation_slug, VocabEdgeRelationSlug::SubsumedBy);
    assert_eq!(edges[1].relation_slug, VocabEdgeRelationSlug::EquivalentTo);
    // Re-serialize and ensure the wire form is verbatim.
    let serialized = serde_json::to_value(&parsed).expect("serialize");
    let edges_out = serialized
        .get("edges")
        .and_then(|v| v.as_array())
        .expect("edges array");
    assert_eq!(edges_out[0]["relationSlug"], "subsumed_by");
    assert_eq!(edges_out[1]["relationSlug"], "equivalent_to");
}
