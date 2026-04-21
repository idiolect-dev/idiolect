//! Smoke test: running the CLI `help` subcommand via a subprocess
//! exits 0 and prints the usage string.
//!
//! Everything else the CLI does hits network endpoints (identity
//! resolution, orchestrator queries), so it's tested indirectly by
//! the library crates' integration suites. The binary itself only
//! needs a smoke test to ensure parsing and dispatch work.

use std::process::Command;

fn cargo_bin(name: &str) -> std::path::PathBuf {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Walk up until we find `target/debug/`. Cargo places the built
    // binary there during `cargo test`.
    path.pop(); // crates/
    path.pop(); // repo root
    path.push("target");
    path.push("debug");
    path.push(name);
    path
}

#[test]
fn help_exits_0_and_prints_usage() {
    let bin = cargo_bin("idiolect");
    if !bin.exists() {
        eprintln!(
            "idiolect binary not found at {}; run `cargo build -p idiolect-cli` first",
            bin.display()
        );
        return;
    }
    let out = Command::new(&bin).arg("help").output().unwrap();
    assert!(out.status.success(), "help should exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("usage: idiolect"));
    assert!(stdout.contains("orchestrator"));
}

#[test]
fn no_args_prints_usage() {
    let bin = cargo_bin("idiolect");
    if !bin.exists() {
        return;
    }
    let out = Command::new(&bin).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("usage: idiolect"));
}

#[test]
fn unknown_subcommand_exits_1() {
    let bin = cargo_bin("idiolect");
    if !bin.exists() {
        return;
    }
    let out = Command::new(&bin).arg("not-a-subcommand").output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown subcommand"));
}

#[test]
fn version_prints_semver_like_string() {
    let bin = cargo_bin("idiolect");
    if !bin.exists() {
        return;
    }
    let out = Command::new(&bin).arg("version").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("idiolect "));
}

#[test]
fn resolve_without_argument_errors() {
    let bin = cargo_bin("idiolect");
    if !bin.exists() {
        return;
    }
    let out = Command::new(&bin).arg("resolve").output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("usage: idiolect resolve"));
}

#[test]
fn orchestrator_adapters_without_framework_errors() {
    let bin = cargo_bin("idiolect");
    if !bin.exists() {
        return;
    }
    let out = Command::new(&bin)
        .args(["orchestrator", "adapters"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--framework"));
}
