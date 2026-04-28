//! Integration test: drive the public `emit_rust` and `emit_typescript`
//! pipeline against the vendored `pub.layers.*` lexicon snapshot under
//! `tests/fixtures/layers-pub/`. Locks the family-config plumbing
//! against the actual downstream consumer's schema (layers-pub) rather
//! than synthetic data, so a regression that quietly anchors on
//! idiolect strings shows up here.
//!
//! The fixture is the live `lexicons/pub/layers/` tree from
//! `~/Projects/layers-pub/layers/lexicons/pub/`, copied at the time
//! this test was written. Refresh by re-running `cp -R` if the layers
//! schema evolves.
//!
//! `lexicon::parse` only supports `record`, `object`, `string`, and
//! `union` top-level defs — the parser bails on the
//! `permission-set` / `query` / `procedure` lexicons that share the
//! tree. The loader here skips files whose top-level def kind isn't
//! supported, mirroring how a real downstream codegen would feed only
//! the structural lexicons through the emit pipeline.

use std::fs;
use std::path::{Path, PathBuf};

use idiolect_codegen::Example;
use idiolect_codegen::emit;
use idiolect_codegen::emit::family::FamilyConfig;
use idiolect_codegen::lexicon;

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("layers-pub")
        .join("lexicons")
}

fn discover_layers_lexicons(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read fixture dir") {
            let entry = entry.expect("read fixture entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "json") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn load_layers_docs() -> Vec<lexicon::LexiconDoc> {
    let mut docs = Vec::new();
    for path in discover_layers_lexicons(&fixtures_root()) {
        let raw = fs::read_to_string(&path).expect("read fixture lexicon");
        let json: serde_json::Value =
            serde_json::from_str(&raw).expect("fixture json parses");
        // Permission-set, query, and procedure lexicons aren't in the
        // structural codegen surface — skip them, same as a downstream
        // codegen would.
        if let Ok(doc) = lexicon::parse(&json) {
            docs.push(doc);
        }
    }
    docs.sort_by(|a, b| a.nsid.cmp(&b.nsid));
    docs
}

fn layers_family() -> FamilyConfig {
    FamilyConfig::new("LayersFamily", "pub.layers", "pub.layers.")
}

#[test]
fn emit_rust_picks_up_non_idiolect_family() {
    let docs = load_layers_docs();
    let examples: Vec<Example> = Vec::new();

    let files = emit::emit_rust(&docs, &examples, &layers_family())
        .expect("emit_rust succeeds against layers fixture");

    let family_rs = files
        .iter()
        .find(|f| f.path == "family.rs")
        .expect("family.rs emitted");

    // Family identity threads through unchanged.
    assert!(
        family_rs.contents.contains("pub struct LayersFamily;"),
        "family.rs should declare LayersFamily marker, got:\n{}",
        family_rs.contents
    );
    assert!(
        family_rs
            .contents
            .contains("const ID: &'static str = \"pub.layers\""),
        "family.rs should set ID = \"pub.layers\", got:\n{}",
        family_rs.contents
    );
    assert!(
        family_rs.contents.contains("for LayersFamily"),
        "family.rs should impl RecordFamily for LayersFamily, got:\n{}",
        family_rs.contents
    );

    // Every pub.layers.* record-type doc surfaces as an AnyRecord
    // variant. Cross-check with the input doc set so the assertion
    // tracks reality if layers adds a record type.
    let record_docs: Vec<&lexicon::LexiconDoc> = docs
        .iter()
        .filter(|d| matches!(d.defs.get("main"), Some(lexicon::Def::Record(_))))
        .filter(|d| d.nsid.starts_with("pub.layers."))
        .collect();
    assert!(
        !record_docs.is_empty(),
        "fixture should contain at least one pub.layers.* record"
    );
    for doc in &record_docs {
        // The variant ident is PascalCase of the leaf module name.
        // We just check that the NSID literal lands in `nsid_str_arms`
        // — robust to ident pascal-casing rules without duplicating
        // them here.
        assert!(
            family_rs.contents.contains(&doc.nsid),
            "family.rs should reference NSID {}, got:\n{}",
            doc.nsid,
            family_rs.contents
        );
    }

    // Idiolect names must not bleed into the layers output — guards
    // against a regression where a hardcoded `IDIOLECT_FAMILY`
    // accidentally re-anchors on its constants.
    assert!(
        !family_rs.contents.contains("IdiolectFamily"),
        "family.rs leaked IdiolectFamily into the layers emit:\n{}",
        family_rs.contents
    );
    assert!(
        !family_rs.contents.contains("\"dev.idiolect\""),
        "family.rs leaked dev.idiolect family ID into the layers emit:\n{}",
        family_rs.contents
    );

    // Sanity: prettyplease still parses what we emitted as valid Rust.
    let _ = syn::parse_file(&family_rs.contents).unwrap_or_else(|e| {
        panic!(
            "rendered family.rs should parse as Rust:\n{}\n\nerror: {e}",
            family_rs.contents
        )
    });
}

#[test]
fn emit_typescript_picks_up_non_idiolect_family() {
    let docs = load_layers_docs();
    let examples: Vec<Example> = Vec::new();

    let files = emit::emit_typescript(&docs, &examples, &layers_family())
        .expect("emit_typescript succeeds against layers fixture");

    let family_ts = files
        .iter()
        .find(|f| f.path == "family.ts")
        .expect("family.ts emitted");

    assert!(
        family_ts.contents.contains("FAMILY_ID = \"pub.layers\""),
        "family.ts should set FAMILY_ID, got:\n{}",
        family_ts.contents
    );
    assert!(
        family_ts
            .contents
            .contains("FAMILY_NSID_PREFIX = \"pub.layers.\""),
        "family.ts should set FAMILY_NSID_PREFIX, got:\n{}",
        family_ts.contents
    );
    assert!(
        family_ts
            .contents
            .contains("type FamilyMarker = \"LayersFamily\""),
        "family.ts should declare FamilyMarker = \"LayersFamily\", got:\n{}",
        family_ts.contents
    );
    assert!(
        family_ts.contents.contains("export function familyContains"),
        "family.ts should export familyContains predicate, got:\n{}",
        family_ts.contents
    );
    assert!(
        family_ts.contents.contains("export function decodeRecord"),
        "family.ts should export decodeRecord, got:\n{}",
        family_ts.contents
    );
    assert!(
        family_ts.contents.contains("export function toTypedJson"),
        "family.ts should export toTypedJson, got:\n{}",
        family_ts.contents
    );

    // No idiolect-specific strings should leak into a non-idiolect
    // emit.
    assert!(
        !family_ts.contents.contains("IdiolectFamily"),
        "family.ts leaked IdiolectFamily into the layers emit:\n{}",
        family_ts.contents
    );
    assert!(
        !family_ts.contents.contains("\"dev.idiolect"),
        "family.ts leaked dev.idiolect references into the layers emit:\n{}",
        family_ts.contents
    );

    // Every pub.layers.* record-type doc gets an NSID entry.
    let record_docs: Vec<&lexicon::LexiconDoc> = docs
        .iter()
        .filter(|d| matches!(d.defs.get("main"), Some(lexicon::Def::Record(_))))
        .filter(|d| d.nsid.starts_with("pub.layers."))
        .collect();
    for doc in &record_docs {
        assert!(
            family_ts.contents.contains(&format!("\"{}\"", doc.nsid)),
            "family.ts should reference NSID literal \"{}\", got:\n{}",
            doc.nsid,
            family_ts.contents
        );
    }
}

#[test]
fn family_filter_excludes_out_of_prefix_records() {
    // Construct a config whose prefix matches no record in the layers
    // fixture; render_family_rs should refuse to emit. Locks the
    // "no records → bail" path so a future refactor can't emit an
    // empty AnyRecord.
    let docs = load_layers_docs();
    let examples: Vec<Example> = Vec::new();
    let bogus = FamilyConfig::new("BogusFamily", "pub.bogus", "pub.bogus.");

    let result = emit::emit_rust(&docs, &examples, &bogus);
    assert!(
        result.is_err(),
        "emit_rust should bail when no records match the family prefix"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("BogusFamily") || err.contains("pub.bogus"),
        "error should name the empty family, got: {err}"
    );
}
