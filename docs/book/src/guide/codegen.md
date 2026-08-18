# Run codegen

Codegen treats the [lexicons](../glossary.md#lexicon "A schema document in the AT Protocol Lexicon language")
under `lexicons/dev/idiolect/` as its source of truth. Regenerate
derived Rust types, TypeScript validators, family modules, HTTP
routes, and CLI dispatch after changing that source.

The crate is
[`idiolect-codegen`](../reference/crates/idiolect-codegen.md).
It runs as a workspace binary (`cargo run -p idiolect-codegen`).

## Invocation

```bash
cargo run -p idiolect-codegen           # write the generated tree
cargo run -p idiolect-codegen -- --check # verify no drift
```

The default mode writes the generated tree. The `--check` flag emits
in memory, compares bytes against the working tree, and exits nonzero
on drift. Both modes parse emitted Rust and TypeScript through
panproto 0.70.1's tree-sitter gate before accepting the output.

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

The three spec files are single JSON documents (not directories).
Each declares an array of entries that codegen reads.

## Check for generated drift

`--check` compares generated output with the checked-in tree. A failure usually
means that a lexicon or
spec changed without regeneration, or that a generated file was
edited by hand. Run the default mode, inspect the resulting diff, and
then run `--check` again.

## Adding a new lexicon

1. Drop a JSON document under `lexicons/dev/idiolect/`.
2. Run `cargo run -p idiolect-codegen`.
3. Optional: add a fixture under
   `lexicons/dev/idiolect/examples/<name>.json` so the
   `examples` module emits a typed accessor.
4. Re-run the workspace tests to confirm the family round-trips.

Codegen rejects malformed lexicons, including invalid
[NSIDs](../glossary.md#nsid "A reverse-DNS identifier for an AT Protocol schema or method"),
before it writes output. The subsequent emit gate rejects malformed
generated source.

## Adding a new spec entry

The orchestrator query, observer method, and verifier runner
specs each carry a different shape. The pattern is the same:

1. Add a JSON entry to the spec file.
2. Run `cargo run -p idiolect-codegen`.
3. Implement the hand-written half (the panproto-expr predicate
   for an orchestrator query, the `ObservationMethod` impl for
   an observer method, the `VerificationRunner` impl for a
   verifier runner).

The dispatcher routes by the declared kind; the generated tree owns
the parser and dispatch table.

## Library API

For consumers outside the workspace, the emitter is callable as
a library:

```text
use idiolect_codegen::emit::{emit_rust, emit_typescript};
use idiolect_codegen::emit::family::{FamilyConfig, idiolect_family};
```

`emit_rust(docs, examples, family)` and
`emit_typescript(docs, examples, family)` take preloaded
`LexiconDoc` and `Example` slices plus `&FamilyConfig`. Each returns
`anyhow::Result<Vec<EmittedFile>>`; loading lexicons from disk remains
the caller's responsibility.

`FamilyConfig::new(marker_name, id, nsid_prefix)` constructs a
config from any string-like inputs. The shipped default for
`dev.idiolect.*` is `idiolect_family()`.

The crate is `publish = false` and not on crates.io. Downstream
consumers depend on it via a path or git reference rather than
a registry version.
