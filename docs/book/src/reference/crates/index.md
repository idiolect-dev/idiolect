# Crates

The workspace ships eleven crates. Each is independently
versioned but bumped together at every release.

| Crate | Purpose |
| --- | --- |
| [idiolect-records](./idiolect-records.md) | Generated record types for the `dev.idiolect.*` lexicons; `Record` trait; family modules. |
| [idiolect-codegen](./idiolect-codegen.md) | Lexicon-driven Rust + TypeScript emitter; drift gate; breaking-change classifier. |
| [idiolect-lens](./idiolect-lens.md) | Resolve `PanprotoLens` records; run `apply_lens`. |
| [idiolect-identity](./idiolect-identity.md) | DID resolution (`did:plc`, `did:web`). |
| [idiolect-indexer](./idiolect-indexer.md) | Firehose consumer with pluggable stream / handler / cursor store. |
| [idiolect-oauth](./idiolect-oauth.md) | `OAuthTokenStore` trait and shipped impls. |
| [idiolect-observer](./idiolect-observer.md) | Fold encounter-family records into observation records. |
| [idiolect-orchestrator](./idiolect-orchestrator.md) | Read-only HTTP query API over a record catalog. |
| [idiolect-verify](./idiolect-verify.md) | Verification runners with declarative dispatch. |
| [idiolect-migrate](./idiolect-migrate.md) | Schema diff plus lens-based record migration. |
| [idiolect-cli](./idiolect-cli.md) | Command-line tool wrapping the library crates. |

Cargo manifests live under `crates/<name>/Cargo.toml`. Every
shipped crate is published to crates.io under the same name and
to docs.rs at `https://docs.rs/<name>/latest/<name_underscored>/`.

## Policy

The pages in this section are editorial overviews: an opinionated
summary of what each crate is for, the public types you reach for
first, and the feature flags. They are not the authoritative
per-symbol reference. The authoritative reference is the rendered
rustdoc on docs.rs, linked at the top of every crate page. When
this book and docs.rs disagree, docs.rs is right.
