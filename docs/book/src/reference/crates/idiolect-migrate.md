# idiolect-migrate

> **Source:** [`crates/idiolect-migrate/`](https://github.com/idiolect-dev/idiolect/tree/main/crates/idiolect-migrate)
>
> This crate is `publish = false` and is not on docs.rs. The
> authoritative reference is the source above plus the rustdoc
> built locally with `cargo doc -p idiolect-migrate --open`.

The crate combines schema-diff classification with
[lens](../../glossary.md#lens)-based record migration. It is a typed facade over panproto 0.71.0's
`panproto-check` crate and `idiolect-lens`.

Because the crate is `publish = false`, depend via git or path:

```toml
[dependencies]
idiolect-migrate = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.11.1" }
```

## Public surface

The crate exposes:

- `classify(old, new, protocol)` runs the panproto diff and returns a
  `CompatReport` distinguishing compatible from breaking
  changes.
- `plan_auto(old, new, protocol, source_schema_hash,
  target_schema_hash)` asks panproto's `auto_generate` to derive a
  `MigrationPlan` carrying source / target schema hashes plus a
  protolens chain the caller can publish. For breaking diffs that
  resist automatic derivation, it returns
  `Err(PlannerError::NotAutoDerivable)` listing the offending
  changes.
- `migrate_record(resolver, schema_loader, protocol, lens_uri,
  source_record)` wraps `idiolect_lens::apply_lens` for one record.
- `MigrationPlan` — the typed plan struct.
- `MigrateError`, `MigrateResult`, `PlannerError` — the error
  types.
- Re-exported `CompatReport` and `SchemaDiff` from
  `panproto-check` for convenience.

## Migration shapes

| Diff | Behavior |
| --- | --- |
| Non-breaking (added optional, added vertex, added edge) | `classify` returns `compatible = true`; no plan is needed. |
| Auto-derivable breaking | `plan_auto` returns a `MigrationPlan` with a protolens-chain body and an alignment-quality score. The exact supported shapes follow panproto 0.71.0's `auto_generate`. |
| Non-auto breaking (removed required, changed required type, added required without default) | `plan_auto` returns `NotAutoDerivable`. The caller writes the lens by hand. |

The alignment-quality score ranks alternatives for one source schema. Panproto
0.71.0 does not give it a pair-independent confidence interpretation.

## Dependency boundary

Two reasons:

1. The migration-shaped API (classify-then-plan-then-migrate)
   is a different shape than the runtime API
   (`apply_lens` plus resolvers).
2. `idiolect-migrate` depends on `panproto-check`, which is a
   heavier dependency than the lens runtime itself. Separating it
   avoids adding that dependency to applications that only apply
   existing lenses.

## The `idiolect-migrate` binary

Behind the `cli` feature the crate also ships an
`idiolect-migrate` binary (`src/bin/idiolect_migrate.rs`) that
streams a directory of JSON records through a lens at-uri against
a live PDS. The library is the default; the binary is opt-in:

```bash
cargo install --path crates/idiolect-migrate --features cli
```

See [Migrate records across a revision](../../guide/migrate.md)
for the batch-migration flow.

## Scope

The crate is a thin facade with no runtime state. The
runtime cost of a migration equals the cost of one `apply_lens`
per record.
