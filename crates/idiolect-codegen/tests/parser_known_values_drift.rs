//! Drift detection: the hardcoded `parser_known_values` lists in
//! `spec_driven/orchestrator.rs` must match the actual `knownValues`
//! / `enum` declared in the source lexicons. Without this gate a
//! lexicon edit can silently desync the XRPC parameter validators.
//!
//! The test loads each relevant lexicon JSON, walks to the
//! enum-bearing field, extracts the value list, and asserts equality
//! with the hardcoded list (sorted, since order is informational).

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|p| p.join("Cargo.lock").exists())
        .map(std::path::Path::to_path_buf)
        .expect("workspace root has Cargo.lock")
}

fn load_lexicon(rel: &str) -> Value {
    let path = workspace_root().join(rel);
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn pluck(v: &Value, path: &[&str]) -> Value {
    let mut cur = v;
    for seg in path {
        cur = cur.get(seg).unwrap_or_else(|| {
            panic!("path {path:?} missing at segment {seg}: {cur}")
        });
    }
    cur.clone()
}

fn extract_values(v: &Value) -> Vec<String> {
    let arr = v
        .get("knownValues")
        .or_else(|| v.get("enum"))
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected knownValues or enum, got {v}"));
    let mut out: Vec<String> = arr
        .iter()
        .filter_map(|x| x.as_str().map(str::to_owned))
        .collect();
    out.sort();
    out
}

fn sorted(slice: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = slice.iter().map(|s| (*s).to_owned()).collect();
    out.sort();
    out
}

#[test]
fn parser_known_values_match_lexicons() {
    // VerificationKind: from dev.idiolect.verification#main.record.properties.kind
    let v = load_lexicon("lexicons/dev/idiolect/verification.json");
    let kind_field = pluck(&v, &["defs", "main", "record", "properties", "kind"]);
    let lex_kinds = extract_values(&kind_field);
    let hard_kinds = sorted(&[
        "roundtrip-test",
        "property-test",
        "formal-proof",
        "conformance-test",
        "static-check",
        "convergence-preserving",
        "coercion-law",
    ]);
    assert_eq!(
        lex_kinds, hard_kinds,
        "verification.kind drift: lexicon vs. parser_known_values"
    );

    // AdapterInvocationProtocolKind: from
    // dev.idiolect.adapter#main.record.properties.invocationProtocol.properties.kind
    let v = load_lexicon("lexicons/dev/idiolect/adapter.json");
    let kind_field = pluck(
        &v,
        &[
            "defs",
            "main",
            "record",
            "properties",
            "invocationProtocol",
            "properties",
            "kind",
        ],
    );
    let lex_protos = extract_values(&kind_field);
    let hard_protos = sorted(&["subprocess", "http", "wasm"]);
    assert_eq!(
        lex_protos, hard_protos,
        "adapter.invocationProtocol.kind drift: lexicon vs. parser_known_values"
    );

    // VocabWorld: from dev.idiolect.vocab#main.record.properties.world
    // (closed enum — keep, since world is meta-policy)
    let v = load_lexicon("lexicons/dev/idiolect/vocab.json");
    let world_field = pluck(&v, &["defs", "main", "record", "properties", "world"]);
    let lex_worlds = extract_values(&world_field);
    let hard_worlds = sorted(&["closed-with-default", "open", "hierarchy-closed"]);
    assert_eq!(
        lex_worlds, hard_worlds,
        "vocab.world drift: lexicon vs. parser_known_values"
    );
}
