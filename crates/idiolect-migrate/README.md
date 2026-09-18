# idiolect-migrate

Explains a schema change, proposes a translation when possible, and applies a
published translation to existing data.

## What it does

This crate answers the three questions a migration worker needs to separate:
did the schema change break compatibility, can Panproto derive a translation,
and how should one record be translated with an approved lens? Keeping those
questions separate lets a community review the plan before any data is
rewritten.

| Input | Work performed | Output |
| --- | --- | --- |
| Current schema, proposed schema, and protocol | Diffs and classifies structural changes | `CompatReport` with compatible and breaking items |
| Both schemas plus their content hashes | Attempts to synthesize a protolens chain | `MigrationPlan` or an explanation of unsupported changes |
| Published lens URI and source record | Resolves and applies the lens | Migrated record body |

Use this crate inside a migration planner or worker. Use
[`idiolect-community`](../idiolect-community) around it when the migration
also needs approvals, release policy, checkpoints, and failure history.

The crate provides one typed API over `panproto-check` diff classification
and [`idiolect-lens`](../idiolect-lens) record translation. It holds no
runtime state.

## Architecture

```mermaid
flowchart LR
    subgraph api["idiolect-migrate"]
        CLS["classify(old, new, protocol)"]
        PLN["plan_auto(old, new, protocol,<br/>old_hash, new_hash)"]
        MIG["migrate_record(resolver,<br/>loader, protocol, lens_uri, body)"]
    end

    subgraph out["Outputs"]
        REP["CompatReport<br/>{ breaking, compatible }"]
        PLAN["MigrationPlan<br/>{ source_hash, target_hash,<br/>protolens_chain }"]
        NEW["migrated record body"]
    end

    PC["panproto_check::diff<br/>+ classify"]
    PL["panproto_lens::auto_generate"]
    LENS["idiolect-lens::apply_lens"]
    PUB["user code → PdsWriter"]

    CLS --> PC --> REP
    PLN --> PL --> PLAN
    MIG --> LENS --> NEW
    PLAN -.->|serialize into PanprotoLens.blob| PUB
```

The operations answer three questions:

- **Is the change compatible?** [`classify`] runs `panproto_check::diff`
  + `classify` and returns a `CompatReport`.
- **Can we derive a migration?** [`plan_auto`] delegates to
  `panproto_lens::auto_generate` and returns a [`MigrationPlan`] containing
  source and target schema hashes plus a protolens chain ready to publish
  as a `dev.panproto.schema.lens` record. An error lists the
  breaking changes the auto-planner declined to synthesize.
- **How do we migrate one record?** [`migrate_record`] wraps `apply_lens`
  for the one-shot case.

## Usage

```rust
use idiolect_migrate::{classify, plan_auto, migrate_record};

let report = classify(&old_schema, &new_schema, &protocol);
if !report.breaking.is_empty() {
    let plan = plan_auto(
        &old_schema,
        &new_schema,
        &protocol,
        "sha256:old",
        "sha256:new",
    )?;
    // plan.protolens_chain: serialize into PanprotoLens.blob, publish.
}

let migrated = migrate_record(
    &resolver,
    &schema_loader,
    &protocol,
    "at://did:plc:x/dev.panproto.schema.lens/mig",
    old_record_body,
).await?;
```

## Boundaries and design choices

Non-goals:

- Deciding the hashing rule for schemas. Hashes are deployment policy;
  pass yours into [`MigrationPlan`].
- Publishing records. [`MigrationPlan`] is a payload shape ready for a
  `PdsWriter`. This crate does not reach out to a PDS.
- Batch-rewriting records on disk. Compose `migrate_record` with your
  own record stream.

## Related

- [`idiolect-lens`](../idiolect-lens): runtime that `migrate_record`
  calls into.
- [`idiolect-codegen`](../idiolect-codegen): its `check-compat`
  subcommand uses the same `panproto_check` pipeline this crate
  surfaces programmatically.
