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
