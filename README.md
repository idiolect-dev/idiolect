<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/assets/wordmark-dark.svg">
  <img src=".github/assets/wordmark.svg" alt="idiolect" height="140"/>
</picture>

<p><strong>Mutual intelligibility for schema idiolects.</strong></p>

<p>
  <a href="https://github.com/idiolect-dev/idiolect/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/idiolect-dev/idiolect/actions/workflows/ci.yml/badge.svg"/></a>
  <a href="https://github.com/idiolect-dev/idiolect/actions/workflows/release.yml"><img alt="Release" src="https://img.shields.io/github/v/release/idiolect-dev/idiolect?sort=semver&label=release"/></a>
  <a href="https://crates.io/crates/idiolect-records"><img alt="idiolect-records on crates.io" src="https://img.shields.io/crates/v/idiolect-records?label=idiolect-records&color=orange"/></a>
  <a href="https://www.npmjs.com/package/@idiolect/schema"><img alt="@idiolect/schema on npm" src="https://img.shields.io/npm/v/@idiolect/schema?color=red"/></a>
  <a href="https://github.com/idiolect-dev/idiolect/blob/main/LICENSE"><img alt="License" src="https://img.shields.io/github/license/idiolect-dev/idiolect?color=blue"/></a>
  <img alt="rustc 1.95+" src="https://img.shields.io/badge/rustc-1.95%2B-blue"/>
</p>

</div>

---

Idiolect turns the linguistic distinction between *idiolects*, *dialects*, and *languages* into an
operating model. An idiolect is one party's choice of schemas, lenses, and
conventions. A dialect is the bundle of idiolects a community treats as
canonical; and a language is the federated substrate over which idiolects and
dialects meet, disagree, and slowly converge without a central arbiter.

Architectural primitives are signed, content-addressed records on
[ATProto](https://atproto.com); and schemas and translations between
schemas are [panproto](https://github.com/panproto/panproto) artifacts. The
project ships reference runtimes, including a CLI, an orchestrator daemon, an observer
daemon, a verification runtime, and a migration library, on top of a small,
set of ten `dev.idiolect.*` lexicons.

## Quickstart

```sh
# CLI: resolve a DID and fetch a record.
cargo install --path crates/idiolect-cli
idiolect resolve did:plc:example
idiolect fetch at://did:plc:example/dev.idiolect.bounty/3l5

# Talk to a local orchestrator.
idiolect orchestrator stats
idiolect orchestrator adapters --framework hasura

# TypeScript: validate incoming records at an appview boundary.
bun add @idiolect/schema
```

```ts
import { NSIDS, isRecord, type Encounter } from "@idiolect/schema";

if (isRecord(NSIDS.encounter, payload)) {
  const e: Encounter = payload;
  console.log(e.kind);
}
```

## Crates

| Crate                          | What it is                                                                |
| ------------------------------ | ------------------------------------------------------------------------- |
| [`idiolect-records`][recs]     | Serde record types mirroring the `dev.idiolect.*` lexicons. Generated.    |
| [`idiolect-codegen`][cg]       | Lexicon-driven Rust + TypeScript emitter. Drives the drift gate.          |
| [`idiolect-lens`][lens]        | Resolve `PanprotoLens` records; run `apply_lens` / `apply_lens_put`.      |
| [`idiolect-identity`][id]      | DID resolution (`did:plc` via plc.directory, `did:web` via well-known).   |
| [`idiolect-indexer`][idx]      | Firehose consumer: `EventStream` + `RecordHandler` + `CursorStore`.       |
| [`idiolect-oauth`][oauth]      | Panproto schema + store trait for atproto OAuth session state.            |
| [`idiolect-observer`][obs]     | Fold encounter-family records into `dev.idiolect.observation` records.    |
| [`idiolect-orchestrator`][orc] | Catalog + read-only HTTP query API over cataloged records.                |
| [`idiolect-verify`][ver]       | Verification runners (`roundtrip-test`, `property-test`, `static-check`). |
| [`idiolect-migrate`][mig]      | Schema diff (panproto-check) + lens-based record migration.               |
| [`idiolect-cli`][cli]          | Command-line tool wrapping the library crates.                            |
| [`@idiolect/schema`][npm]      | TypeScript validators, types, and NSID constants (same lexicons).         |

[recs]: crates/idiolect-records
[cg]: crates/idiolect-codegen
[lens]: crates/idiolect-lens
[id]: crates/idiolect-identity
[idx]: crates/idiolect-indexer
[oauth]: crates/idiolect-oauth
[obs]: crates/idiolect-observer
[orc]: crates/idiolect-orchestrator
[ver]: crates/idiolect-verify
[mig]: crates/idiolect-migrate
[cli]: crates/idiolect-cli
[npm]: packages/schema

## Architecture in one paragraph

The lexicons under `lexicons/dev/` are the single source of truth. Rust types
(`idiolect-records`) and TypeScript validators (`@idiolect/schema`) are
derived via `idiolect-codegen` — CI rejects any PR whose generated output
disagrees with the lexicons. Four crates with a taxonomy of similarly-shaped
items — the orchestrator's queries, the observer's methods, the verifier's
runners, and the CLI's subcommands — each live behind a declarative JSON
spec (`<crate>-spec/`) validated against its own atproto-shaped lexicon;
codegen emits the wire-up while hand-written predicates and semantics supply
the business logic. Runtime state that must not federate — firehose cursors,
OAuth tokens — rides the same panproto schema apparatus as everything else,
flagged under `dev.idiolect.internal.*` so conformant firehose consumers
skip it.

## Install

Binaries for every release are published to the
[releases page](https://github.com/idiolect-dev/idiolect/releases), signed
with sigstore keyless. Container images for the daemons ship to
`ghcr.io/idiolect-dev/orchestrator` and `ghcr.io/idiolect-dev/observer`. See
[`docs/deployment.md`](docs/deployment.md) for operator-facing setup,
[`docs/ci-cd.md`](docs/ci-cd.md) for artifact verification, and
[`RELEASE.md`](RELEASE.md) for the release cadence.

## Stack

- **Rust:** edition 2024, toolchain 1.95, resolver 3, cargo-nextest.
- **TypeScript:** bun 1.2, biome 2.3, tsc 5.7.
- **Monorepo:** moon for polyglot task orchestration and toolchain pinning.

## Getting started as a contributor

```sh
# One-off setup.
moon :setup

# The common loops.
moon :build         # build everything
moon :test          # run every test
moon :lint          # fmt + clippy + biome + tsc

# Regenerate after editing a lexicon or a spec.
cargo run -p idiolect-codegen -- generate
```

Before opening a PR, confirm `cargo fmt --all`, `cargo clippy --workspace
--all-targets -- -D warnings`, `cargo test --workspace`, and `bun run lint
&& bun run typecheck && bun run test` all pass. CI runs the same commands
plus a lexicon breaking-change gate against the PR's merge base.

## Contributing

Issue templates and the PR template under `.github/` are the canonical route
for reports and proposals. The project holds opinionated architectural
commitments; consult the [feature-request template](.github/ISSUE_TEMPLATE/feature.yml)
if you are unsure whether a capability fits before opening.

## License

[MIT](LICENSE).
