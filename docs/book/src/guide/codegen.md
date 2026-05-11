# Run codegen

Codegen keeps the lexicons (under `lexicons/dev/idiolect/`) the
single source of truth. Anything that lives downstream of a
lexicon (Rust types, TypeScript validators, family modules,
spec-driven HTTP routes and CLI subcommands) is regenerated, not
hand-edited.

The crate is
[`idiolect-codegen`](../reference/crates/idiolect-codegen.md).
It runs as a workspace binary (`cargo run -p idiolect-codegen`).

## Invocation

```bash
cargo run -p idiolect-codegen           # write the generated tree
cargo run -p idiolect-codegen -- --check # verify no drift
```

The default mode regenerates. The `--check` flag re-derives in
memory and byte-compares against the working tree, exiting
non-zero on any drift. CI runs `--check` on every PR.

## What gets emitted

| Output | Source | Where |
| --- | --- | --- |
| Per-record Rust types | `lexicons/dev/idiolect/*.json` and the vendored `lexicons/dev/panproto/*` | `crates/idiolect-records/src/generated/` |
| `idiolect-records` family module | the shipped lexicons | `crates/idiolect-records/src/generated/family.rs` |
| Per-record fixtures (Rust) | `lexicons/dev/idiolect/examples/*.json` | `crates/idiolect-records/src/generated/examples.rs` |
| TypeScript validators + types | same lexicons | `packages/schema/src/generated/` |
| Orchestrator HTTP routes | `orchestrator-spec/queries.json` | `crates/idiolect-orchestrator/src/generated/` |
| Orchestrator CLI dispatcher | `orchestrator-spec/queries.json` | `crates/idiolect-cli/src/generated.rs` |
| Observer method descriptors | `observer-spec/methods.json` | `crates/idiolect-observer/src/generated.rs` |
| Verifier kind taxonomy | `verify-spec/runners.json` | `crates/idiolect-verify/src/generated.rs` |

The three spec files are single JSON documents (not directories);
each declares an array of entries that codegen reads.

## The drift gate

`--check` is the drift gate. CI runs it on every PR. A red gate
means somebody edited a lexicon (or a spec) without rerunning
the default mode, or hand-edited a generated file. Both are
fixable by running `cargo run -p idiolect-codegen`.

## Adding a new lexicon

1. Drop a JSON document under `lexicons/dev/idiolect/`.
2. Run `cargo run -p idiolect-codegen`.
3. Optional: add a fixture under
   `lexicons/dev/idiolect/examples/<name>.json` so the
   `examples` module emits a typed accessor.
4. Re-run the workspace tests to confirm the family round-trips.

The codegen rejects malformed lexicons (NSID violations,
unresolved references) before emitting, so a broken lexicon
errors at this step rather than at compile time.

## Adding a new spec entry

The orchestrator query, observer method, and verifier runner
specs each carry a different shape. The pattern is the same:

1. Add a JSON entry to the spec file.
2. Run `cargo run -p idiolect-codegen`.
3. Implement the hand-written half (the panproto-expr predicate
   for an orchestrator query, the `ObservationMethod` impl for
   an observer method, the `VerificationRunner` impl for a
   verifier runner).

The dispatcher routes to the new entry by its kind; the
generated tree handles parsing and shape.

## Library API

For consumers outside the workspace, the emitter is callable as
a library:

```rust
use idiolect_codegen::emit::{emit_rust, emit_typescript};
use idiolect_codegen::emit::family::{FamilyConfig, idiolect_family};
```

`emit_rust(docs, examples, family)` and
`emit_typescript(docs, examples, family)` take pre-loaded
`LexiconDoc` and `Example` slices plus a `FamilyConfig`, and
return a `Vec<EmittedFile>`. Loading the lexicons from disk is
the caller's job.

`FamilyConfig::new(marker_name, id, nsid_prefix)` constructs a
config from any string-like inputs; the shipped default for
`dev.idiolect.*` is `idiolect_family()`.

The crate is `publish = false` and not on crates.io; downstream
consumers depend on it via a path or git reference rather than
a registry version.
