//! Smoke test against a real public PDS.
//!
//! Hits `bsky.social`'s public `com.atproto.repo.getRecord` endpoint
//! and fetches the canonical bsky team profile record
//! `at://did:plc:z72i7hdynmk6r22z27h6tvur/app.bsky.actor.profile/self`.
//! This is a well-known, unauthenticated-readable record that has
//! been stable for years; it is the closest thing to a "ping" target
//! the atproto public network offers.
//!
//! Gated behind `pds-smoke-test` (which in turn enables
//! `pds-atrium`). Run locally with:
//!
//! ```text
//! cargo test -p idiolect-lens --features pds-smoke-test
//! ```
//!
//! CI does not enable this feature, so the network is never touched
//! during the default test matrix. The crate's offline correctness is
//! covered by `tests/pds_atrium.rs` against a wiremock server.

use idiolect_lens::AtriumPdsClient;
use idiolect_lens::resolver::PdsClient;

/// The bsky team profile — a long-lived public record on the flagship
/// PDS. If this ever disappears, pick any other stable
/// `app.bsky.actor.profile` record on a well-known DID.
const PUBLIC_DID: &str = "did:plc:z72i7hdynmk6r22z27h6tvur";
const PUBLIC_COLLECTION: &str = "app.bsky.actor.profile";
const PUBLIC_RKEY: &str = "self";

#[tokio::test(flavor = "current_thread")]
async fn fetches_public_bsky_profile_record() {
    let client = AtriumPdsClient::with_service_url("https://bsky.social");

    let value = client
        .get_record(PUBLIC_DID, PUBLIC_COLLECTION, PUBLIC_RKEY)
        .await
        .expect("fetch public bsky team profile from bsky.social");

    // we don't deserialize into a lens record — this isn't one. the
    // point is to prove the xrpc plumbing talks to a real server and
    // gives us back a well-formed json object with the expected
    // `$type`.
    let obj = value
        .as_object()
        .expect("getRecord response value is a json object");
    let ty = obj
        .get("$type")
        .and_then(|v| v.as_str())
        .expect("record has a $type field");
    assert_eq!(ty, "app.bsky.actor.profile");
}
