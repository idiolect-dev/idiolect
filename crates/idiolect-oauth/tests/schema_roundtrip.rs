//! Integration test: the bundled session lexicon must parse into a
//! panproto [`Schema`] AND survive an identity lens applied over its
//! w-instances.
//!
//! The round-trip guards two invariants at once:
//!
//! 1. The lexicon file is well-formed enough for
//!    `panproto_protocols::web_document::atproto::parse_lexicon` to
//!    accept it.
//! 2. The `OAuthSession` struct's json wire form is compatible with
//!    the schema's body vertex. If a new field lands in the struct
//!    but not the lexicon (or vice versa), the round-trip either
//!    fails to parse or drops fields.
//!
//! The identity lens is the canonical "panproto-native, but a no-op"
//! protolens: `rename_sort("record", "record")`. Every atproto record
//! lexicon has a `record`-sorted vertex, so the precondition is met
//! for any record-shaped schema.

use idiolect_lens::{
    ApplyLensInput, ApplyLensPutInput, InMemoryResolver, InMemorySchemaLoader, apply_lens,
    apply_lens_put,
};
use idiolect_oauth::{OAuthSession, SESSION_NSID, session_body_vertex, session_schema};
use idiolect_records::PanprotoLens;
use panproto_lens::protolens::elementary;
use panproto_protocols::web_document::atproto;

/// A representative OAuth session used across tests.
fn sample_session() -> OAuthSession {
    let mut s = OAuthSession::new(
        "did:plc:alice",
        "https://pds.example.com",
        "eyJhbGciOi-access",
        "eyJhbGciOi-refresh",
        "{\"kty\":\"EC\",\"crv\":\"P-256\"}",
        "2026-04-19T00:00:00.000Z",
        "2026-04-19T01:00:00.000Z",
    );

    s.dpop_nonce = Some("nonce-abc".to_owned());
    s.token_type = Some("DPoP".to_owned());
    s.scope = Some("atproto transition:generic".to_owned());
    s.handle = Some("alice.example.com".to_owned());
    s.refresh_expires_at = Some("2026-05-19T01:00:00.000Z".to_owned());
    s
}

#[test]
fn bundled_session_lexicon_parses_as_panproto_schema() {
    let schema = session_schema().expect("bundled session lexicon parses");

    // non-trivial vertex count means the body + leaves actually
    // materialised; a degenerate parse would hand back an empty
    // schema.
    assert!(
        schema.vertices.len() >= 2,
        "expected at least the root + body vertices, got {}",
        schema.vertices.len(),
    );

    // the schema must expose the session body vertex under the
    // canonical name we use to route json instances.
    let body_vertex = session_body_vertex();
    assert!(
        schema
            .vertices
            .iter()
            .any(|(name, _)| name.as_str() == body_vertex),
        "session schema missing expected body vertex `{body_vertex}`",
    );
}

#[test]
fn session_nsid_is_outside_dev_idiolect_record_family() {
    // indexer filters on the `dev.idiolect.` prefix but the
    // `internal.` segment flags that this nsid is not a PDS record.
    // a deliberate guard: if someone ever renames SESSION_NSID away
    // from this shape, they need to think through whether the
    // firehose consumer needs a matching exclusion.
    assert!(SESSION_NSID.starts_with("dev.idiolect.internal."));
    assert_eq!(SESSION_NSID, "dev.idiolect.internal.oauthSession");
}

#[tokio::test(flavor = "current_thread")]
async fn identity_lens_round_trips_oauth_session_body() {
    let src_schema = session_schema().expect("bundled session schema parses");
    let protocol = atproto::protocol();

    // identity protolens: every atproto record schema has a
    // `record`-sorted vertex, so the precondition holds, and
    // record→record is a no-op.
    let protolens = elementary::rename_sort("record", "record");
    let tgt_schema = protolens
        .target_schema(&src_schema, &protocol)
        .expect("derive identity target schema");

    let src_hash = "at://did:plc:x/dev.panproto.schema.schema/oauth-session-src".to_owned();
    let tgt_hash = "at://did:plc:x/dev.panproto.schema.schema/oauth-session-tgt".to_owned();

    let mut loader = InMemorySchemaLoader::new();
    loader.insert(src_hash.clone(), src_schema);
    loader.insert(tgt_hash.clone(), tgt_schema);

    let blob = serde_json::to_value(&protolens).expect("serialize protolens");
    let lens_uri =
        idiolect_lens::AtUri::parse("at://did:plc:xyz/dev.panproto.schema.lens/oauth-id")
            .expect("parse lens uri");
    let lens_record = PanprotoLens {
        blob: Some(blob),
        created_at: idiolect_records::Datetime::parse("2026-04-19T00:00:00.000Z")
            .expect("valid datetime"),
        laws_verified: Some(true),
        object_hash: "sha256:deadbeef".to_owned(),
        round_trip_class: Some("isomorphism".into()),
        source_schema: idiolect_records::AtUri::parse(&src_hash).expect("valid at-uri"),
        target_schema: idiolect_records::AtUri::parse(&tgt_hash).expect("valid at-uri"),
    };

    let mut resolver = InMemoryResolver::new();
    resolver.insert(&lens_uri, lens_record);

    let body_root = session_body_vertex();
    let session = sample_session();
    let source_record = serde_json::to_value(&session).expect("serialize session to json");

    // forward (get): parse session json through the lens.
    let forward = apply_lens(
        &resolver,
        &loader,
        &protocol,
        ApplyLensInput {
            lens_uri: lens_uri.clone(),
            source_record: source_record.clone(),
            source_root_vertex: Some(body_root.clone()),
        },
    )
    .await
    .expect("forward apply_lens");

    // backward (put): recover the original session from the view.
    let backward = apply_lens_put(
        &resolver,
        &loader,
        &protocol,
        ApplyLensPutInput {
            lens_uri: lens_uri.clone(),
            target_record: forward.target_record,
            complement: forward.complement,
            target_root_vertex: Some(body_root),
        },
    )
    .await
    .expect("backward apply_lens_put");

    // the reconstructed record must deserialize cleanly back into
    // an OAuthSession and match the original.
    let reconstructed: OAuthSession = serde_json::from_value(backward.source_record)
        .expect("reconstructed json deserializes as OAuthSession");

    // every required field survives.
    assert_eq!(reconstructed.did, session.did);
    assert_eq!(reconstructed.pds_url, session.pds_url);
    assert_eq!(reconstructed.access_jwt, session.access_jwt);
    assert_eq!(reconstructed.refresh_jwt, session.refresh_jwt);
    assert_eq!(
        reconstructed.dpop_private_key_jwk,
        session.dpop_private_key_jwk
    );
    assert_eq!(reconstructed.issued_at, session.issued_at);
    assert_eq!(reconstructed.expires_at, session.expires_at);

    // optional metadata survives too.
    assert_eq!(reconstructed.dpop_nonce, session.dpop_nonce);
    assert_eq!(reconstructed.token_type, session.token_type);
    assert_eq!(reconstructed.scope, session.scope);
    assert_eq!(reconstructed.handle, session.handle);
    assert_eq!(reconstructed.refresh_expires_at, session.refresh_expires_at);
}
