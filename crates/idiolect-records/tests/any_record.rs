//! Exercises the non-generated helpers on [`AnyRecord`]:
//! `nsid`, `Display`, `Serialize` (typed-json wire form with `$type`),
//! and `from_typed_json`.

use idiolect_records::generated::dev::idiolect::adapter::{
    AdapterInvocationProtocol, AdapterInvocationProtocolKind, AdapterIsolation,
    AdapterIsolationKind,
};
use idiolect_records::{Adapter, AnyRecord};

fn sample_adapter() -> Adapter {
    Adapter {
        author: "did:plc:author".into(),
        framework: "hasura".into(),
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
        occurred_at: "2026-04-21T00:00:00Z".into(),
        verification: None,
        version_range: ">=1 <2".into(),
    }
}

#[test]
fn nsid_matches_record_trait() {
    use idiolect_records::Record;
    let r = AnyRecord::Adapter(sample_adapter());
    assert_eq!(r.nsid_str(), Adapter::NSID);
}

#[test]
fn display_includes_nsid() {
    let r = AnyRecord::Adapter(sample_adapter());
    assert_eq!(r.to_string(), "AnyRecord(dev.idiolect.adapter)");
}

#[test]
fn to_typed_json_splices_type_field() {
    let r = AnyRecord::Adapter(sample_adapter());
    let body = r.to_typed_json().unwrap();
    assert_eq!(body["$type"], "dev.idiolect.adapter");
    assert_eq!(body["framework"], "hasura");
}

#[test]
fn serialize_impl_emits_typed_json() {
    let r = AnyRecord::Adapter(sample_adapter());
    let s = serde_json::to_string(&r).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(parsed["$type"], "dev.idiolect.adapter");
    assert_eq!(parsed["framework"], "hasura");
}

#[test]
fn from_typed_json_round_trips() {
    let original = AnyRecord::Adapter(sample_adapter());
    let body = original.to_typed_json().unwrap();
    let decoded = AnyRecord::from_typed_json(body).unwrap();
    assert_eq!(decoded.nsid_str(), "dev.idiolect.adapter");
    match decoded {
        AnyRecord::Adapter(a) => {
            assert_eq!(a.framework, "hasura");
            assert_eq!(a.version_range, ">=1 <2");
        }
        other => panic!("unexpected variant: {other}"),
    }
}

#[test]
fn from_typed_json_errors_on_missing_type() {
    let body = serde_json::json!({ "framework": "hasura" });
    let err = AnyRecord::from_typed_json(body).unwrap_err();
    assert!(err.to_string().contains("missing $type field"));
}

#[test]
fn from_typed_json_errors_on_unknown_type() {
    let body = serde_json::json!({ "$type": "app.bsky.feed.post" });
    let err = AnyRecord::from_typed_json(body).unwrap_err();
    assert!(err.to_string().contains("app.bsky.feed.post"));
}

#[test]
fn from_typed_json_errors_on_body_mismatch() {
    // nsid says adapter, body is missing required fields.
    let body = serde_json::json!({
        "$type": "dev.idiolect.adapter",
        "framework": "hasura",
        // missing author, invocation_protocol, isolation, etc.
    });
    let err = AnyRecord::from_typed_json(body).unwrap_err();
    // Serde error message mentions a missing field.
    assert!(err.to_string().to_ascii_lowercase().contains("missing"));
}
