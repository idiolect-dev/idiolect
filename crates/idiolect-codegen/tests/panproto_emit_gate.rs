//! Emit-gate contract tests against panproto-parse.
//!
//! Three pins:
//!
//! 1. panproto reports `emit_pretty` for the rust and typescript
//!    grammars as `Verified` (the corpus-audited tier). The pilot
//!    target refuses to render when this regresses, so the pin makes
//!    an upstream downgrade loud at upgrade time.
//! 2. The gate accepts the checked-in generated trees: every file the
//!    emitters produced re-parses cleanly through the matching
//!    grammar.
//! 3. The gate rejects source that only parses via tree-sitter's
//!    error recovery, in both grammars.

#![allow(clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use idiolect_codegen::emit::gate;
use panproto_parse::EmitVerificationStatus;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate lives two levels under the repo root")
        .to_path_buf()
}

/// Recursively collect `(relative path, contents)` for every file
/// under `root` with the given extension.
fn collect_generated(root: &Path, ext: &str) -> Vec<(String, String)> {
    fn walk(dir: &Path, root: &Path, ext: &str, out: &mut Vec<(String, String)>) {
        for entry in fs::read_dir(dir).expect("read generated dir") {
            let path = entry.expect("read generated entry").path();
            if path.is_dir() {
                walk(&path, root, ext, out);
            } else if path.extension().is_some_and(|e| e == ext) {
                let rel = path
                    .strip_prefix(root)
                    .expect("path under root")
                    .to_string_lossy()
                    .replace('\\', "/");
                let contents = fs::read_to_string(&path).expect("read generated file");
                out.push((rel, contents));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, ext, &mut out);
    out.sort();
    out
}

#[test]
fn emit_verification_status_is_pinned_verified() {
    let registry = gate::shared_registry();
    for protocol in ["rust", "typescript"] {
        assert_eq!(
            registry.emit_verification_status(protocol),
            EmitVerificationStatus::Verified,
            "panproto no longer reports emit_pretty for {protocol} as \
             corpus-verified; the pilot target's trust assumption broke"
        );
    }
}

#[test]
fn gate_accepts_generated_rust_tree() {
    let root = repo_root().join("crates/idiolect-records/src/generated");
    let files = collect_generated(&root, "rs");
    assert!(!files.is_empty(), "no generated rust files under {root:?}");
    gate::verify_parses("rust", files.iter().map(|(p, c)| (p.as_str(), c.as_str())))
        .expect("checked-in generated rust must pass the emit gate");
}

#[test]
fn gate_accepts_generated_typescript_tree() {
    let root = repo_root().join("packages/schema/src/generated");
    let files = collect_generated(&root, "ts");
    assert!(
        !files.is_empty(),
        "no generated typescript files under {root:?}"
    );
    gate::verify_parses(
        "typescript",
        files.iter().map(|(p, c)| (p.as_str(), c.as_str())),
    )
    .expect("checked-in generated typescript must pass the emit gate");
}

#[test]
fn gate_rejects_unparseable_rust() {
    let err = gate::verify_parses("rust", [("broken.rs", "pub strct Broken { x: }")])
        .expect_err("token soup must not pass the gate");
    assert!(
        matches!(err, gate::GateError::ErrorVertices { .. }),
        "expected ErrorVertices, got {err:?}"
    );
}

#[test]
fn gate_rejects_unparseable_typescript() {
    let err = gate::verify_parses("typescript", [("broken.ts", "export const = {;")])
        .expect_err("token soup must not pass the gate");
    assert!(
        matches!(err, gate::GateError::ErrorVertices { .. }),
        "expected ErrorVertices, got {err:?}"
    );
}
