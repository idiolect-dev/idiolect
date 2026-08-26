#!/usr/bin/env python3
"""Compile every Rust fence in the mdBook as an independent binary."""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

from check_book import BOOK, ROOT, display, extract_fences


def expand_include(fence_path: Path, body: str) -> str:
    stripped = body.strip()
    prefix = "{{#include "
    if not (stripped.startswith(prefix) and stripped.endswith("}}")):
        return body
    include = stripped[len(prefix) : -2].strip()
    source_name = include.split(":", 1)[0]
    source = (fence_path.parent / source_name).resolve()
    if not source.is_file():
        raise ValueError(f"missing included Rust source {source}")
    return source.read_text(encoding="utf-8")


def runnable_source(body: str) -> str:
    lines: list[str] = []
    for line in body.splitlines():
        if line.startswith("##"):
            lines.append(line[1:])
        elif line.startswith("# "):
            lines.append(line[2:])
        else:
            lines.append(line)
    source = "\n".join(lines).rstrip() + "\n"
    if "fn main(" not in source:
        source += "\nfn main() {}\n"
    return source


def main() -> int:
    errors: list[str] = []
    rust_fences = []
    for path in sorted(BOOK.rglob("*.md")):
        text = path.read_text(encoding="utf-8")
        rust_fences.extend(
            fence
            for fence in extract_fences(path.resolve(), text, errors)
            if fence.language == "rust"
        )
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    with tempfile.TemporaryDirectory(prefix="idiolect-book-rust-") as temporary:
        project = Path(temporary)
        bins = project / "src" / "bin"
        bins.mkdir(parents=True)
        mappings: list[str] = []
        for number, fence in enumerate(rust_fences, 1):
            name = f"book_{number:03d}"
            try:
                body = expand_include(fence.path, fence.body)
            except ValueError as exc:
                print(f"{display(fence.path)}:{fence.line}: {exc}", file=sys.stderr)
                return 1
            (bins / f"{name}.rs").write_text(runnable_source(body), encoding="utf-8")
            mappings.append(f"{name}={display(fence.path)}:{fence.line}")

        manifest = f'''[package]
name = "idiolect-book-rust-check"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
anyhow = "1"
async-trait = "0.1"
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
tokio = {{ version = "1", features = ["full"] }}
tracing-subscriber = "0.3"
url = "2"
idiolect-codegen = {{ path = "{ROOT / 'crates/idiolect-codegen'}" }}
idiolect-identity = {{ path = "{ROOT / 'crates/idiolect-identity'}", features = ["resolver-reqwest"] }}
idiolect-indexer = {{ path = "{ROOT / 'crates/idiolect-indexer'}", features = ["firehose-tapped", "reconnecting", "resilience", "firehose-jetstream", "cursor-filesystem", "cursor-sqlite"] }}
idiolect-lens = {{ path = "{ROOT / 'crates/idiolect-lens'}", features = ["pds-reqwest", "pds-resolve", "dpop-p256"] }}
idiolect-migrate = {{ path = "{ROOT / 'crates/idiolect-migrate'}", features = ["cli"] }}
idiolect-oauth = {{ path = "{ROOT / 'crates/idiolect-oauth'}", features = ["store-filesystem", "store-sqlite"] }}
idiolect-observer = {{ path = "{ROOT / 'crates/idiolect-observer'}", features = ["daemon"] }}
idiolect-orchestrator = {{ path = "{ROOT / 'crates/idiolect-orchestrator'}", features = ["daemon"] }}
idiolect-records = {{ path = "{ROOT / 'crates/idiolect-records'}" }}
idiolect-verify = {{ path = "{ROOT / 'crates/idiolect-verify'}" }}
panproto-lens = {{ git = "https://github.com/panproto/panproto.git", tag = "v0.72.0" }}
panproto-schema = {{ git = "https://github.com/panproto/panproto.git", tag = "v0.72.0" }}
'''
        (project / "Cargo.toml").write_text(manifest, encoding="utf-8")

        print(f"Rust fences: {len(rust_fences)}")
        for mapping in mappings:
            print(mapping)
        if not rust_fences:
            print("Rust fence compilation passed (no runnable fences)")
            return 0
        completed = subprocess.run(
            ["cargo", "check", "--quiet", "--bins", "--manifest-path", str(project / "Cargo.toml")],
            cwd=ROOT,
            check=False,
        )
        if completed.returncode:
            print("Rust fence compilation failed; use the mapping above to locate each bin", file=sys.stderr)
            return completed.returncode
    print("Rust fence compilation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
