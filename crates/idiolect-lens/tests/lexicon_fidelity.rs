//! Lexicon-fidelity integration tests.
//!
//! Exercises the full `apply_lens` pipeline against a schema that
//! was produced by `panproto_protocols::atproto::parse_lexicon`
//! rather than hand-rolled in a `SchemaBuilder`. This closes the
//! loop from a vendored `dev.idiolect.*` lexicon document all the
//! way through to a round-tripped record.
//!
//! Two complementary checks:
//!
//! - `parse_lexicon_handles_every_idiolect_lexicon`: walks every
//!   `lexicons/dev/idiolect/*.json`, runs it through the parser,
//!   and confirms every document yields a valid [`Schema`] keyed
//!   by its nsid.
//! - `apply_lens_identity_round_trips_observation_fixture`: takes
//!   the observation lexicon's parsed schema, wraps it behind an
//!   identity protolens (`rename_sort("record", "record")`), and
//!   runs the example fixture through forward `get` and backward
//!   `put`. Required top-level fields on the record body survive
//!   the round-trip intact.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use idiolect_lens::{
    ApplyLensInput, ApplyLensPutInput, InMemoryResolver, InMemorySchemaLoader, apply_lens,
    apply_lens_put,
};
use idiolect_records::PanprotoLens;
use panproto_lens::protolens::elementary;
use panproto_protocols::web_document::atproto;
use panproto_schema::Schema;

/// Absolute path to the vendored `dev.idiolect.*` lexicon tree.
///
/// Computed from `CARGO_MANIFEST_DIR` so the test stays correct
/// regardless of where cargo is invoked.
fn idiolect_lexicon_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("lexicons/dev/idiolect")
}

/// Read and parse a lexicon json file.
fn read_json(path: &Path) -> serde_json::Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// Walk every top-level `.json` document under the idiolect lexicon
/// tree (skipping `examples/`) and run it through the atproto
/// lexicon parser. Returns `(nsid, schema)` pairs.
fn parse_all_lexicons() -> Vec<(String, Schema)> {
    let dir = idiolect_lexicon_dir();
    let mut out = Vec::new();

    for entry in fs::read_dir(&dir).expect("read lexicon dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();

        if path.is_dir() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        let value = read_json(&path);
        let nsid = value
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("lexicon {} missing id", path.display()))
            .to_owned();

        let schema = atproto::parse_lexicon(&value)
            .unwrap_or_else(|e| panic!("parse lexicon {}: {e}", path.display()));

        out.push((nsid, schema));
    }

    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn parse_lexicon_handles_every_idiolect_lexicon() {
    let parsed = parse_all_lexicons();
    assert!(
        !parsed.is_empty(),
        "expected at least one idiolect lexicon under lexicons/dev/idiolect",
    );

    // every parsed schema must be keyed by the same nsid the json
    // document declared.
    let ids: HashSet<&str> = parsed.iter().map(|(id, _)| id.as_str()).collect();

    // spot-check the record nsids we expect to always be present. if
    // any of these disappear the appview's indexing story breaks, so
    // catching it here is cheaper than discovering it at firehose
    // ingestion time.
    for expected in [
        "dev.idiolect.adapter",
        "dev.idiolect.bounty",
        "dev.idiolect.community",
        "dev.idiolect.correction",
        "dev.idiolect.dialect",
        "dev.idiolect.encounter",
        "dev.idiolect.observation",
        "dev.idiolect.recommendation",
        "dev.idiolect.retrospection",
        "dev.idiolect.verification",
    ] {
        assert!(
            ids.contains(expected),
            "missing expected idiolect lexicon: {expected}",
        );
    }
}

#[tokio::test]
async fn apply_lens_identity_round_trips_observation_fixture() {
    let parsed = parse_all_lexicons();
    let protocol = atproto::protocol();

    // identity protolens: `HasSort("record")` is satisfied by every
    // record-typed lexicon, and the sort-rename from "record" to
    // "record" is a no-op, so the derived target schema equals the
    // source schema.
    let protolens = elementary::rename_sort("record", "record");

    let src_schema = parsed
        .iter()
        .find(|(nsid, _)| nsid == "dev.idiolect.observation")
        .map(|(_, s)| s.clone())
        .expect("observation lexicon present");

    let tgt_schema = protolens
        .target_schema(&src_schema, &protocol)
        .expect("derive target schema");

    // opaque hashes — the lens record references them by string and
    // the loader hands back whatever schema we registered.
    let src_hash = "sha256:src".to_owned();
    let tgt_hash = "sha256:tgt".to_owned();

    let mut loader = InMemorySchemaLoader::new();
    loader.insert(src_hash.clone(), src_schema);
    loader.insert(tgt_hash.clone(), tgt_schema);

    let blob = serde_json::to_value(&protolens).expect("serialize protolens");
    let lens_uri = idiolect_lens::AtUri::parse("at://did:plc:xyz/dev.panproto.schema.lens/obs")
        .expect("parse lens uri");
    let lens_record = PanprotoLens {
        blob: Some(blob),
        created_at: "2026-04-19T00:00:00.000Z".to_owned(),
        laws_verified: Some(true),
        object_hash: "sha256:deadbeef".to_owned(),
        round_trip_class: Some("isomorphism".to_owned()),
        source_schema: src_hash,
        target_schema: tgt_hash,
    };

    let mut resolver = InMemoryResolver::new();
    resolver.insert(&lens_uri, lens_record);

    // the example is the record's body (flat top-level fields), so
    // we route past the `record-schema` intermediate by naming the
    // body vertex explicitly instead of letting `primary_entry`
    // return the outer record vertex.
    let body_root = "dev.idiolect.observation:body".to_owned();

    let example_path = idiolect_lexicon_dir().join("examples/observation.json");
    let source_record = read_json(&example_path);

    // forward (get): parse, apply identity lens, serialize.
    let forward = apply_lens(
        &resolver,
        &loader,
        &protocol,
        ApplyLensInput {
            lens_uri: lens_uri.to_string(),
            source_record: source_record.clone(),
            source_root_vertex: Some(body_root.clone()),
        },
    )
    .await
    .expect("forward apply_lens");

    // backward (put): feed the view back through and recover the
    // source.
    let backward = apply_lens_put(
        &resolver,
        &loader,
        &protocol,
        ApplyLensPutInput {
            lens_uri: lens_uri.to_string(),
            target_record: forward.target_record,
            complement: forward.complement,
            target_root_vertex: Some(body_root),
        },
    )
    .await
    .expect("backward apply_lens_put");

    // the reconstructed record must be a json object.
    let reconstructed = backward
        .source_record
        .as_object()
        .expect("reconstructed source is an object");

    // top-level primitive fields survive the round-trip intact.
    assert_eq!(
        reconstructed.get("observer").and_then(|v| v.as_str()),
        Some("did:plc:observer"),
        "observer did should survive round-trip",
    );
    assert_eq!(
        reconstructed.get("version").and_then(|v| v.as_str()),
        Some("1.0.0"),
        "version string should survive round-trip",
    );
    assert_eq!(
        reconstructed.get("visibility").and_then(|v| v.as_str()),
        Some("public-detailed"),
        "visibility ref should survive as-is",
    );
    assert_eq!(
        reconstructed.get("occurredAt").and_then(|v| v.as_str()),
        Some("2026-04-19T00:00:00.000Z"),
        "occurredAt timestamp should survive round-trip",
    );
}
