//! Parity pin for the panproto-native rust pilot.
//!
//! The pilot (`emit::panproto_rust::directory_mod_files`) renders the
//! directory `mod.rs` index files as by-construction abstract schemas
//! through panproto's verified tree-sitter-rust emitter. The syn
//! target stays canonical; this test pins the pilot byte-equal to it
//! (both sides pass through the same rustfmt normalisation) over the
//! repo's real lexicon tree, so the swap path is proved on actual
//! output rather than a synthetic fixture.

#![allow(clippy::expect_used)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use idiolect_codegen::emit;
use idiolect_codegen::lexicon;

fn lexicons_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate lives two levels under the repo root")
        .join("lexicons/dev")
}

/// Mirror the binary's lexicon discovery: every `.json` under
/// `lexicons/dev/`, skipping the `examples/` fixture subtrees and the
/// generated `query/` XRPC lexicons.
fn load_docs() -> Vec<lexicon::LexiconDoc> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("read lexicons dir") {
            let path = entry.expect("read lexicons entry").path();
            if path.is_dir() {
                let name = path.file_name().and_then(|s| s.to_str());
                if name == Some("examples") || name == Some("query") {
                    continue;
                }
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "json") {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(&lexicons_root(), &mut files);
    files.sort();

    let mut docs: Vec<lexicon::LexiconDoc> = files
        .iter()
        .map(|path| {
            let raw = fs::read_to_string(path).expect("read lexicon");
            let json: serde_json::Value = serde_json::from_str(&raw).expect("lexicon json");
            lexicon::parse(&json).expect("lexicon parses")
        })
        .collect();
    docs.sort_by(|a, b| a.nsid.cmp(&b.nsid));
    docs
}

#[test]
fn pilot_matches_syn_target_byte_for_byte() {
    let docs = load_docs();
    assert!(!docs.is_empty(), "no lexicons found");

    let family = emit::family::idiolect_family();
    let canonical: BTreeMap<String, String> = emit::emit_rust(&docs, &[], &family)
        .expect("syn target emits")
        .into_iter()
        // Directory index files are the nested `<dir>/mod.rs` entries;
        // the root `mod.rs` (re-exports) is out of the pilot's scope.
        .filter(|f| f.path.ends_with("/mod.rs"))
        .map(|f| (f.path, f.contents))
        .collect();

    let pilot: BTreeMap<String, String> = emit::panproto_rust::directory_mod_files(&docs)
        .expect("panproto pilot renders")
        .into_iter()
        .collect();

    assert!(
        !canonical.is_empty(),
        "expected at least one directory mod.rs from the syn target"
    );
    assert_eq!(
        canonical.keys().collect::<Vec<_>>(),
        pilot.keys().collect::<Vec<_>>(),
        "pilot and syn target must emit the same index-file set"
    );
    for (path, want) in &canonical {
        let got = &pilot[path];
        assert_eq!(
            got, want,
            "panproto-rendered {path} drifted from the syn target's bytes"
        );
    }
}
