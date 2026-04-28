//! Integration test: every seed vocabulary under
//! `lexicons/dev/idiolect/examples/vocab/` and the Acorn bridge
//! vocabulary under `examples/idiolect-acorn/data/vocabs/` must
//! parse as a [`Vocab`] record and lift cleanly through
//! [`VocabGraph::from_vocab`]. These files are checked-in
//! authoritative records — drift here would break consumers
//! pointing at the canonical AT-URIs.

use std::fs;
use std::path::{Path, PathBuf};

use idiolect_records::Vocab;
use idiolect_records::vocab::VocabGraph;
use serde_json::Value;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    manifest_dir()
        .ancestors()
        .find(|p| p.join("Cargo.lock").exists())
        .map(Path::to_path_buf)
        .expect("workspace root has Cargo.lock")
}

fn json_files_in(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            out.push(path);
        }
    }
    out.sort();
    out
}

fn parse_vocab(path: &Path) -> Vocab {
    let bytes = fs::read(path).expect("read vocab file");
    // Example records carry an authoring `$nsid` discriminator that
    // is not part of the lexicon schema; strip it before deserializing.
    let mut value: Value = serde_json::from_slice(&bytes).expect("parse json");
    if let Some(map) = value.as_object_mut() {
        map.remove("$nsid");
    }
    serde_json::from_value(value).expect("deserialize as Vocab")
}

#[test]
fn seed_vocabs_parse_and_lift_to_graph() {
    let dir = workspace_root().join("lexicons/dev/idiolect/examples/vocab");
    let files = json_files_in(&dir);
    assert!(!files.is_empty(), "expected at least one seed vocab");
    for path in &files {
        let vocab = parse_vocab(path);
        let graph = VocabGraph::from_vocab(&vocab);
        assert!(
            graph.node_count() > 0,
            "{}: vocab has zero nodes",
            path.display()
        );
        // Every concept node must reach at least itself under
        // subsumed_by reflexive walk; covers the case where a
        // misauthored vocab severs its hierarchy.
        for node in graph.nodes() {
            if node.kind.as_deref() == Some("relation") {
                continue;
            }
            let closure = graph.walk_relation(&node.id, "subsumed_by", true);
            assert!(
                closure.iter().any(|c| c == &node.id),
                "{}: node {} missing reflexive subsumed_by",
                path.display(),
                node.id
            );
        }
    }
}

#[test]
fn vote_stances_seed_has_polar_opposite_edge() {
    let dir = workspace_root().join("lexicons/dev/idiolect/examples/vocab");
    let vocab = parse_vocab(&dir.join("vote-stances.json"));
    let graph = VocabGraph::from_vocab(&vocab);
    let opposites = graph.walk_relation("agree", "polar_opposite_of", false);
    assert!(
        opposites.iter().any(|n| n == "disagree"),
        "agree should have polar_opposite_of edge to disagree, got {opposites:?}"
    );
}

#[test]
fn verification_kinds_seed_subsumes_all_under_top() {
    let dir = workspace_root().join("lexicons/dev/idiolect/examples/vocab");
    let vocab = parse_vocab(&dir.join("verification-kinds.json"));
    let graph = VocabGraph::from_vocab(&vocab);
    let top = graph.top().expect("verification-kinds-v1 has a unique top");
    assert_eq!(top, "any_verification");
    for kind in [
        "roundtrip-test",
        "property-test",
        "formal-proof",
        "conformance-test",
        "static-check",
        "convergence-preserving",
        "coercion-law",
    ] {
        assert!(
            graph.is_subsumed_by(kind, &top),
            "{kind} must be subsumed by top {top}"
        );
    }
}

#[test]
fn community_roles_seed_subsumes_specific_roles_under_member() {
    let dir = workspace_root().join("lexicons/dev/idiolect/examples/vocab");
    let vocab = parse_vocab(&dir.join("community-roles.json"));
    let graph = VocabGraph::from_vocab(&vocab);
    for role in ["moderator", "delegate", "author"] {
        assert!(
            graph.is_subsumed_by(role, "member"),
            "{role} should subsume to member"
        );
    }
}

#[test]
fn acorn_bridge_vote_stances_translate_via_equivalent_to() {
    // Bridge vocab should translate Blacksky-style slugs into the
    // canonical idiolect vote-stances. Direct match wins (slugs are
    // identical) but the bridge also declares equivalent_to edges
    // for forward-compatibility when slugs diverge.
    let bridge_path =
        workspace_root().join("examples/idiolect-acorn/data/vocabs/blacksky-vote-stances.json");
    let canonical_path =
        workspace_root().join("lexicons/dev/idiolect/examples/vocab/vote-stances.json");
    let bridge = parse_vocab(&bridge_path);
    let canonical = parse_vocab(&canonical_path);
    let g_bridge = VocabGraph::from_vocab(&bridge);
    let g_canonical = VocabGraph::from_vocab(&canonical);
    for slug in ["agree", "pass", "disagree"] {
        assert_eq!(
            g_bridge.equivalent_in(slug, &g_canonical),
            Some(slug.to_owned()),
            "bridge slug {slug} must translate into canonical vocab"
        );
    }
}

#[test]
fn acorn_bridge_records_parse() {
    use idiolect_records::{Adapter, Community, Dialect};

    let dir = workspace_root().join("examples/idiolect-acorn/data/bridge-records");

    let mut community_value: Value =
        serde_json::from_slice(&fs::read(dir.join("blacksky-community.json")).expect("community"))
            .expect("community json");
    community_value
        .as_object_mut()
        .expect("object")
        .remove("$nsid");
    let community: Community =
        serde_json::from_value(community_value).expect("Community deserializes");
    assert_eq!(community.name, "Blacksky");
    assert!(community.appview_endpoint.is_some());

    let mut dialect_value: Value =
        serde_json::from_slice(&fs::read(dir.join("blacksky-dialect.json")).expect("dialect"))
            .expect("dialect json");
    dialect_value
        .as_object_mut()
        .expect("object")
        .remove("$nsid");
    let dialect: Dialect = serde_json::from_value(dialect_value).expect("Dialect deserializes");
    assert!(
        dialect
            .preferred_lenses
            .as_ref()
            .map_or(0, std::vec::Vec::len)
            >= 3,
        "dialect should reference at least three bridge lenses"
    );

    let mut adapter_value: Value = serde_json::from_slice(
        &fs::read(dir.join("blacksky-appview-adapter.json")).expect("adapter"),
    )
    .expect("adapter json");
    adapter_value
        .as_object_mut()
        .expect("object")
        .remove("$nsid");
    let adapter: Adapter = serde_json::from_value(adapter_value).expect("Adapter deserializes");
    assert_eq!(adapter.framework, "blacksky-appview");
    assert_eq!(adapter.invocation_protocol.kind.as_str(), "http");
}
