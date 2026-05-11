# idiolect-codegen

> **Source:** [`crates/idiolect-codegen/`](https://github.com/idiolect-dev/idiolect/tree/main/crates/idiolect-codegen)
>
> This crate is `publish = false`: it is workspace-internal
> machinery, not a library you depend on. There is no docs.rs
> page. The authoritative reference is the source above.

Lexicon-driven Rust + TypeScript emitter. Reads
`lexicons/dev/idiolect/*.json` and the three spec files
(`orchestrator-spec/queries.json`, `observer-spec/methods.json`,
`verify-spec/runners.json`); writes the generated modules under
each downstream crate.

The crate is shipped both as a library (callable from a
downstream emitter) and as a binary (`cargo run -p idiolect-codegen`).

## Binary subcommands

`cargo run -p idiolect-codegen` invokes the binary. The two
operations:

| Mode | Purpose |
| --- | --- |
| Default | Emit every generated tree. |
| `--check` | Verify the working tree matches what the default mode would produce. Exits non-zero on drift. |

The check mode is the drift gate. CI runs it on every PR.

## Library API

The callable surface is in `idiolect_codegen::emit`:

```rust
use idiolect_codegen::emit::{emit_rust, emit_typescript};
use idiolect_codegen::emit::family::{FamilyConfig, idiolect_family};
use idiolect_codegen::lexicon::LexiconDoc;
use idiolect_codegen::Example;
```

`emit_rust(docs, examples, family)` and
`emit_typescript(docs, examples, family)` take pre-loaded
`LexiconDoc` and `Example` slices plus a `FamilyConfig`, and
return `Vec<EmittedFile>`. Loading the lexicons from disk is the
caller's job; the workspace binary does this through the
`idiolect_codegen::lexicon` parser.

`FamilyConfig` carries three `Cow<'static, str>` fields: the
marker name, the family ID, and the NSID prefix. The shipped
default for `dev.idiolect.*` is the `idiolect_family()`
constructor.

## What it emits

Per shipped lexicon (`lexicons/dev/idiolect/<name>.json`):

- A Rust module under
  `crates/idiolect-records/src/generated/dev/idiolect/<name>.rs`
  with the typed record struct, every nested `defs` type, the
  `Record` impl, and the open-enum types with their helpers.
- A TypeScript module under
  `packages/schema/src/generated/` with the validator, the
  discriminator predicates, and the
  `'a' | 'b' | (string & {})` open-enum types.

Per spec file:

- `orchestrator-spec/queries.json` produces the orchestrator's
  HTTP routes (`crates/idiolect-orchestrator/src/generated/`)
  and the matching CLI dispatcher
  (`crates/idiolect-cli/src/generated.rs`).
- `observer-spec/methods.json` produces the observer's method
  taxonomy (`crates/idiolect-observer/src/generated.rs`).
- `verify-spec/runners.json` produces the verifier's runner
  taxonomy (`crates/idiolect-verify/src/generated.rs`).

Each spec file is a single JSON document with a top-level
`queries` / `methods` / `runners` array; codegen produces the
dispatch tables and typed enums. The hand-written predicates
live alongside the generated tree.

## Drift gate semantics

`cargo run -p idiolect-codegen -- --check` runs the same emitter
as the default mode, then byte-compares each emitted file
against the working-tree counterpart. Any diff is a drift error,
with a per-file diff. Run `cargo run -p idiolect-codegen` to
fix.

## Identifier policy

Three rules:

1. NSIDs are ASCII, lowercase, dot-separated. The emitter
   rejects non-conforming input.
2. PascalCase names are derived deterministically from a slug.
   On collision (`foo-bar` and `foo_bar`), the second occurrence
   gets a numeric suffix (`FooBar2`).
3. The emitter walks each record's path until each member's
   prefix is unique within the colliding group; the alias is
   the unique-prefix concatenation (e.g. `ChangelogEntry`,
   `ResourceEntry`).

The collision report is printed at codegen time so authors can
rename a slug when the generated name is awkward.
