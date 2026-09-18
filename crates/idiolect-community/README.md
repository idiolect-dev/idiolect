# idiolect-community

File-backed infrastructure for communities that govern shared schemas,
vocabularies, and migrations.

## What it does

`idiolect-community` turns a definition change into a reviewable lifecycle.
A community supplies its identity, governed packages, authorities, decision
rules, and resource limits in `idiolect.toml`. The crate then keeps each
proposal together with its schema-difference report, participant-facing
consequences, reviews, verification evidence, release signatures, and
migration state.

The unit of work is a **change packet**, not an isolated schema file. This
means a reviewer can answer three separate questions: what changed, what the
change does to existing data, and whether the community's rules permit it to
ship.

| Input | Work performed | Output |
| --- | --- | --- |
| Community name, DID, authorities, and policy | Initializes and validates a workspace | `idiolect.toml` plus managed directories |
| Current and proposed schema documents | Runs bounded Panproto comparison and migration analysis | Change packet with compatibility and consequence evidence |
| Reviews and verification results | Evaluates the configured governance and verification gates | Approved, rejected, or pending decision with a reason |
| Approved packets, artifacts, and signing keys | Builds, signs, and verifies an immutable release index | Community release bundle |
| Batch progress, cursors, and failures | Advances a resumable migration record | Durable migration checkpoints |
| Workspace plus optional release | Copies the material allowed by the exit policy and hashes every file | Portable export with an inventory |

## Use it when

- a group needs explicit rules for changing shared definitions;
- reviewers need consequences in addition to a structural diff;
- releases must identify their artifacts, decisions, dependencies, and
  signers;
- a long-running migration must survive interruption and retain failure
  evidence;
- a community needs a practical way to move, back up, or fork its work.

The crate does not publish anything by itself. Workspaces, private keys,
change packets, migrations, and exports remain local until a caller chooses a
publication mechanism.

## Lifecycle operations

| Operation | Library API | CLI surface | What changes on disk |
| --- | --- | --- | --- |
| Create workspace | `init_workspace` | `idiolect init` | Manifest and managed directories |
| Diagnose workspace | `check_workspace`, `doctor_workspace`, `repair_workspace` | `idiolect check`, `idiolect doctor` | Nothing unless repair is requested |
| Analyze change | `analyze_schema_change` | `idiolect propose`, `idiolect preview` | Change packet under `.idiolect/changes/` |
| Apply governance | `record_review`, `evaluate_governance` | `idiolect review` | Review log and decision in the packet |
| Attach evidence | `record_verification`, `verification_gate` | `idiolect verify change` | Verification evidence in the packet |
| Sign release | `create_release`, `sign_release`, `verify_release` | `idiolect keygen`, `idiolect release` | Private key and signed release bundle |
| Track migration | `create_migration_run`, `advance_migration` | `idiolect migrate` | Migration run under `.idiolect/migrations/` |
| Make portable copy | `export_workspace` | `idiolect export` | New directory plus `export-inventory.json` |

## Workspace layout

```text
idiolect.toml                 community identity, policy, packages, and limits
lexicons/                     default location for governed definitions
.idiolect/
  changes/                    proposals, consequences, reviews, and evidence
  migrations/                 progress, cursors, and sampled failures
  releases/                   signed immutable release indexes
  keys/                       local signing keys; keep these private
```

Package and release paths are configurable in the manifest. Export follows
the manifest's exit policy, rejects symbolic links, and refuses to write into
a nonempty destination.

## Governance and verification

The supported decision models are maintainer approval, consent, threshold
vote, steward approval, and a maintainer-plus-steward hybrid. All models read
the same review log but apply different quorum and approval rules. Breaking or
data-loss-risking changes can require a steward even when the primary model
would otherwise approve them.

Verification evidence has three outcomes: `verified`, `refuted`, and
`incomplete`. A release gate accepts only verified evidence for every check
named by policy; an incomplete run is not treated as a weaker success.

## Boundaries and design choices

- Resource limits belong to the workspace so browser, CLI, and service
  clients can analyze untrusted definitions under the same bounds.
- A federation relationship records lineage or policy between communities.
  Separate mapping URIs provide evidence that their data can be translated.
- Release signatures use community authorities from the manifest. A valid
  signature proves who signed the indexed bytes; it does not decide whether a
  consumer should trust that authority.
- Migration runs record coordination state. The caller still streams,
  translates, and writes the actual records.

## Related

- [Newcomer path](../../docs/book/src/start/index.md): complete workflow in
  plain language.
- [Community manifest reference](../../docs/book/src/reference/community-manifest.md):
  every `idiolect.toml` field.
- [`idiolect-cli`](../idiolect-cli): command-line interface over this crate.
- [`idiolect-migrate`](../idiolect-migrate): schema classification and
  per-record translation used by migration workers.
