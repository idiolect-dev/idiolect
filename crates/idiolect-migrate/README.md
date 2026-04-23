# idiolect-migrate

Schema diff and lens-based record migration.

## Overview

A thin typed façade over `panproto-check` (diff classification) and
[`idiolect-lens`](../idiolect-lens) (record translation). The crate has
no runtime state of its own — it is glue shaped so callers can answer
three questions about two schema versions in one API.

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
    PLAN -.serialize into<br/>PanprotoLens.blob.-> PUB
```

The three questions:

- **Is the change compatible?** [`classify`] runs `panproto_check::diff`
  + `classify` and returns a `CompatReport`.
- **Can we auto-derive a migration?** [`plan_auto`] delegates to
  `panproto_lens::auto_generate` and returns a [`MigrationPlan`] —
  source/target schema hashes plus a protolens chain ready to publish
  as a `dev.panproto.schema.lens` record — or an error listing the
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

## Design notes

Non-goals:

- Deciding the hashing rule for schemas. Hashes are deployment policy;
  pass yours into [`MigrationPlan`].
- Publishing records. [`MigrationPlan`] is a payload shape ready for a
  `PdsWriter`; this crate does not reach out to a PDS.
- Batch-rewriting records on disk. Compose `migrate_record` with your
  own record stream.

## Related

- [`idiolect-lens`](../idiolect-lens) — runtime that `migrate_record`
  calls into.
- [`idiolect-codegen`](../idiolect-codegen) — the `check-compat`
  subcommand uses the same `panproto_check` pipeline this crate
  surfaces programmatically.
