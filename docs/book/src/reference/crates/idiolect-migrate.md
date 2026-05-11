# idiolect-migrate

> **Source:** [`crates/idiolect-migrate/`](https://github.com/idiolect-dev/idiolect/tree/main/crates/idiolect-migrate)
>
> This crate is `publish = false` and is not on docs.rs. The
> authoritative reference is the source above plus the rustdoc
> built locally with `cargo doc -p idiolect-migrate --open`.

Schema-diff classification plus lens-based record migration.
Thin typed façade over `panproto-check` (for diff classification)
and `idiolect-lens` (for record translation).

Because the crate is `publish = false`, depend via git or path:

```toml
[dependencies]
idiolect-migrate = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.8.0" }
```

## Public surface

The crate exposes:

- `classify(src, tgt)` — runs the panproto diff and returns a
  `CompatReport` distinguishing compatible from breaking
  changes.
- `plan_auto(src, tgt, hints)` — for breaking diffs that are
  covered by shipped migration recipes, returns a
  `MigrationPlan` carrying source / target schema hashes plus a
  lens body the caller can publish. For breaking diffs that
  resist automation, returns
  `Err(PlannerError::NotAutoDerivable)` listing the offending
  changes.
- `migrate_record(lens, source_record, schema_loader)` — wraps
  `idiolect_lens::apply_lens` for the one-shot case.
- `MigrationPlan` — the typed plan struct.
- `MigrateError`, `MigrateResult`, `PlannerError` — the error
  types.
- Re-exported `CompatReport` and `SchemaDiff` from
  `panproto-check` for convenience.

## Migration shapes

| Diff | Behavior |
| --- | --- |
| Non-breaking (added optional, added vertex, added edge) | `classify` returns `compatible = true`; no plan is needed. |
| Auto-derivable breaking (removed optional, renamed vertex via hint) | `plan_auto` returns a `MigrationPlan` with a protolens-chain body. |
| Non-auto breaking (removed required, changed required type, added required without default) | `plan_auto` returns `NotAutoDerivable`. The caller writes the lens by hand. |

## Why this is a separate crate from idiolect-lens

Two reasons:

1. The migration-shaped API (classify-then-plan-then-migrate)
   is a different shape than the runtime API
   (`apply_lens` plus resolvers).
2. `idiolect-migrate` depends on `panproto-check`, which is a
   heavier dep than the lens runtime itself. Keeping it
   separate keeps the runtime crate's compile-time small.

## Scope

The crate owns no runtime state. It is a thin façade; the
runtime cost of a migration equals the cost of one `apply_lens`
per record.
