# idiolect-lens

> **Source:** [`crates/idiolect-lens/`](https://github.com/idiolect-dev/idiolect/tree/main/crates/idiolect-lens)
>
> This crate is `publish = false` and is not on docs.rs. The
> authoritative reference is the source above plus the rustdoc
> built locally with `cargo doc -p idiolect-lens --open`.

Resolve `dev.panproto.schema.lens` records and run `apply_lens`.
Bridges idiolect's record runtime to panproto's lens runtime.

Because the crate is `publish = false`, downstream consumers
depend on it via a git or path reference rather than a
registry version:

```toml
[dependencies]
idiolect-lens = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.8.0", features = ["pds-reqwest"] }
```

## Public surface

### Resolvers

`Resolver` is the trait every resolver implements. It is
object-safe (`Arc<dyn Resolver>`) since v0.8.0; the resolve
future is `Send`.

Shipped implementations:

| Type | Backing store |
| --- | --- |
| `InMemoryResolver` | `HashMap<AtUri, PanprotoLens>`. For tests and fixtures. |
| `PdsResolver<C>` | `com.atproto.repo.getRecord` via a pluggable `PdsClient`. |
| `PanprotoVcsResolver<C>` | A panproto vcs store via a pluggable `PanprotoVcsClient`. |
| `CachingResolver<R>` | TTL'd cache wrapping any `R: Resolver`. |
| `VerifyingResolver<R, H>` | Re-hashes the bytes, refuses on mismatch. |

### Schema loaders

`SchemaLoader` is also object-safe. Shipped implementations:
`InMemorySchemaLoader`, `FilesystemSchemaLoader`.

### Apply functions

The runtime shipped under `idiolect_lens::runtime`:

- `apply_lens` / `apply_lens_put` — state-based forward / backward.
- `apply_lens_get_edit` / `apply_lens_put_edit` — edit-based
  variants for incremental translation.
- `apply_lens_symmetric` — symmetric pairing of two state-based
  lenses sharing a middle schema.

Each takes a resolver, a schema loader, a `Protocol`, and a
typed input struct; each returns a typed output struct. The
composed future is `Send` so callers can spawn it under
`tokio::spawn` or hold it inside an `#[async_trait]` impl.

### PDS clients

`PdsClient` (read) and `PdsWriter` (write) are the boundary
traits over xrpc. Behind feature flags:

| Feature | Adds |
| --- | --- |
| `pds-reqwest` | `ReqwestPdsClient` (read-only). The reqwest-backed write surface uses `SigningPdsWriter` plus a `DpopProver` (one of `StaticDpopProver`, `NoOpDpopProver`, or `P256DpopProver` with the `dpop-p256` feature). |
| `pds-atrium` | `AtriumPdsClient`. |
| `pds-resolve` | `fetcher_for_did`, `publisher_for_did` — DID-to-PDS resolution helpers. Pulls in `idiolect-identity`. |
| `dpop-p256` | The `P256DpopProver` for OAuth-bound DPoP requests. |

### Generic publisher

`RecordPublisher<W: PdsWriter>` is the typed publisher. Wrap any
`PdsWriter` with `RecordPublisher::new(writer, repo_did)` and
publish typed records via `publisher.create::<R: Record>(&record)`,
`publisher.put`, and `publisher.delete`. The publisher serializes
the record, splices the `$type` field, and forwards to the
`PdsWriter` boundary.

### Errors

`LensError` collapses backend-specific errors into a small set
of variants (`NotFound`, `Transport`, decode failures, translate
failures). Backend-specific errors collapse to one of these at
the resolver layer; callers do not pattern-match on transport
types.

## Composition pattern

The recommended runtime stack:

```rust
use std::sync::Arc;
use std::time::Duration;
use idiolect_lens::*;

let client = ReqwestPdsClient::with_service_url("https://bsky.social");
let inner: Arc<dyn Resolver> = Arc::new(PdsResolver::new(client));
let verifying = Arc::new(VerifyingResolver::sha256(inner));
let resolver = CachingResolver::new(verifying, Duration::from_secs(300));

let loader = FilesystemSchemaLoader::new("./schema-cache")?;

let out = apply_lens(&resolver, &loader, &Protocol::default(), input).await?;
```

The `Arc<dyn Resolver>` indirection lets a downstream
orchestrator inject a different resolver (e.g. a record-of-record
mock for tests) without changing the surface.
