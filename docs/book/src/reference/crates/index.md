# Crates

The workspace ships eleven crates at version 0.12.0. The workspace
version keeps their release numbers aligned.

| Crate | Purpose |
| --- | --- |
| [idiolect-records](./idiolect-records.md) | Generated record types for the `dev.idiolect.*` lexicons; `Record` trait; family modules. |
| [idiolect-codegen](./idiolect-codegen.md) | Lexicon-driven Rust + TypeScript emitter; generated-source check; breaking-change classifier. |
| [idiolect-lens](./idiolect-lens.md) | Resolve `PanprotoLens` records; run `apply_lens`. |
| [idiolect-identity](./idiolect-identity.md) | DID resolution (`did:plc`, `did:web`). |
| [idiolect-indexer](./idiolect-indexer.md) | Firehose consumer with pluggable stream / handler / cursor store. |
| [idiolect-oauth](./idiolect-oauth.md) | `OAuthTokenStore` trait and shipped impls. |
| [idiolect-observer](./idiolect-observer.md) | Fold encounter-family records into observation records. |
| [idiolect-orchestrator](./idiolect-orchestrator.md) | Read-only HTTP query API over a record catalog. |
| [idiolect-verify](./idiolect-verify.md) | Verification runners with declarative dispatch. |
| [idiolect-migrate](./idiolect-migrate.md) | Schema diff plus lens-based record migration. |
| [idiolect-cli](./idiolect-cli.md) | Command-line tool wrapping the library crates. |

Cargo manifests live under `crates/<name>/Cargo.toml`. Three
crates — `idiolect-records`, `idiolect-identity`, and
`idiolect-indexer` — are published to crates.io under the same
name and to docs.rs at
`https://docs.rs/<name>/latest/<name_underscored>/`. The rest are
`publish = false` and are consumed via a git or path reference;
each crate page states which applies.

## Reference boundary

These pages list the exported boundaries and feature flags. Use docs.rs
for the three published crates. For workspace-only crates, build
rustdoc from the checkout named at the top of the page.
