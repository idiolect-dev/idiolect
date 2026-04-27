//! Tests for the `RecordFamily` trait and `OrFamily` composer.

use idiolect_records::{
    AnyRecord, IdiolectFamily, Nsid, OrAny, OrFamily, RecordFamily, decode_record,
    detect_or_family_overlap,
};
use serde_json::json;

fn nsid(s: &str) -> Nsid {
    Nsid::parse(s).expect("valid nsid")
}

#[test]
fn idiolect_family_id_is_dev_idiolect() {
    assert_eq!(IdiolectFamily::ID, "dev.idiolect");
}

#[test]
fn idiolect_family_contains_known_record_types() {
    assert!(IdiolectFamily::contains(&nsid("dev.idiolect.encounter")));
    assert!(IdiolectFamily::contains(&nsid("dev.idiolect.bounty")));
    assert!(IdiolectFamily::contains(&nsid("dev.idiolect.community")));
}

#[test]
fn idiolect_family_rejects_outside_nsids() {
    assert!(!IdiolectFamily::contains(&nsid("app.bsky.feed.post")));
    assert!(!IdiolectFamily::contains(&nsid(
        "dev.panproto.schema.lens"
    )));
    // Same prefix, NSID not a known idiolect record type — the
    // codegen-emitted contains predicate enumerates the exact set,
    // so this returns false rather than relying on the prefix.
    assert!(!IdiolectFamily::contains(&nsid(
        "dev.idiolect.notARealRecord"
    )));
}

#[test]
fn idiolect_family_decode_returns_none_for_outside_nsids() {
    let body = json!({});
    let result = IdiolectFamily::decode(&nsid("app.bsky.feed.post"), body)
        .expect("out-of-family decode does not error");
    assert!(result.is_none());
}

#[test]
fn idiolect_family_decode_round_trips_known_record() {
    let bounty_nsid = nsid("dev.idiolect.bounty");
    // Pull the canonical example so the body actually deserializes.
    let body = serde_json::to_value(idiolect_records::generated::examples::bounty()).expect("serialize bounty");
    let decoded = IdiolectFamily::decode(&bounty_nsid, body)
        .expect("in-family decode succeeds")
        .expect("in-family decode returns Some");
    assert!(matches!(decoded, AnyRecord::Bounty(_)));
}

#[test]
fn idiolect_family_decode_propagates_serde_error_for_in_family_garbage() {
    // In-family NSID, body that doesn't satisfy the schema.
    let body = json!({"not": "a valid bounty"});
    let err = IdiolectFamily::decode(&nsid("dev.idiolect.bounty"), body)
        .expect_err("missing-required-fields surfaces as Serde error");
    assert!(matches!(
        err,
        idiolect_records::DecodeError::Serde(_)
    ));
}

#[test]
fn nsid_str_round_trips_through_any_record() {
    let n = nsid("dev.idiolect.community");
    let body = serde_json::to_value(idiolect_records::generated::examples::community())
        .expect("serialize community");
    let any = decode_record(&n, body).expect("decode");
    assert_eq!(any.nsid_str(), "dev.idiolect.community");
    assert_eq!(IdiolectFamily::nsid_str(&any), "dev.idiolect.community");
}

// -----------------------------------------------------------------
// OrFamily
// -----------------------------------------------------------------

/// A throwaway second family that "claims" `app.bsky.feed.post` and
/// nothing else. Lets us test `OrFamily` composition without
/// depending on a real second family being in the workspace.
#[derive(Debug, Clone, Copy)]
struct StubBskyFamily;

#[derive(Debug, Clone)]
struct StubPost {
    text: String,
}

const STUB_POST_NSID: &str = "app.bsky.feed.post";

impl RecordFamily for StubBskyFamily {
    type AnyRecord = StubPost;
    const ID: &'static str = "app.bsky";

    fn contains(nsid: &Nsid) -> bool {
        nsid.as_str() == STUB_POST_NSID
    }

    fn decode(
        nsid: &Nsid,
        body: serde_json::Value,
    ) -> Result<Option<Self::AnyRecord>, idiolect_records::DecodeError> {
        if nsid.as_str() != STUB_POST_NSID {
            return Ok(None);
        }
        let text = body
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        Ok(Some(StubPost { text }))
    }

    fn nsid_str(_: &Self::AnyRecord) -> &'static str {
        STUB_POST_NSID
    }

    fn to_typed_json(record: &Self::AnyRecord) -> Result<serde_json::Value, serde_json::Error> {
        Ok(json!({ "$type": STUB_POST_NSID, "text": record.text }))
    }
}

type Combined = OrFamily<IdiolectFamily, StubBskyFamily>;

#[test]
fn or_family_contains_is_union() {
    assert!(Combined::contains(&nsid("dev.idiolect.encounter")));
    assert!(Combined::contains(&nsid(STUB_POST_NSID)));
    assert!(!Combined::contains(&nsid("dev.panproto.schema.lens")));
}

#[test]
fn or_family_decode_routes_left_then_right() {
    // Left side claims dev.idiolect.bounty.
    let body = serde_json::to_value(idiolect_records::generated::examples::bounty()).unwrap();
    let decoded = Combined::decode(&nsid("dev.idiolect.bounty"), body)
        .unwrap()
        .unwrap();
    assert!(matches!(decoded, OrAny::Left(AnyRecord::Bounty(_))));

    // Right side claims app.bsky.feed.post.
    let decoded = Combined::decode(&nsid(STUB_POST_NSID), json!({"text": "hi"}))
        .unwrap()
        .unwrap();
    let OrAny::Right(post) = decoded else {
        panic!("expected Right");
    };
    assert_eq!(post.text, "hi");
}

#[test]
fn or_family_decode_returns_none_for_outside_nsids() {
    let result = Combined::decode(&nsid("dev.panproto.schema.lens"), json!({})).unwrap();
    assert!(result.is_none());
}

#[test]
fn or_family_overlap_detector_surfaces_intersection() {
    // Pair an idiolect NSID against itself (so both sides "claim" it
    // for the test).
    type Self_ = OrFamily<IdiolectFamily, IdiolectFamily>;
    let probe = vec![
        nsid("dev.idiolect.encounter"),
        nsid("app.bsky.feed.post"),
    ];
    let overlap =
        detect_or_family_overlap::<IdiolectFamily, IdiolectFamily>(&probe);
    assert_eq!(overlap.len(), 1);
    assert_eq!(overlap[0].as_str(), "dev.idiolect.encounter");
    // Self_ is the type — exercise it just so the compiler verifies
    // the `RecordFamily for OrFamily<F, F>` bound resolves.
    let _ = Self_::ID;
}

#[test]
fn or_family_id_is_constant() {
    // OrFamily can't synthesise a per-composition ID at compile
    // time; the constant is "OrFamily" and curated bundles wrap
    // their own marker.
    assert_eq!(Combined::ID, "OrFamily");
}
