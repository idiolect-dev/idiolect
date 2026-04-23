# idiolect-lens

Resolve `PanprotoLens` records and run them through the panproto lens
runtime.

## Overview

Lenses in the `dev.panproto.schema.lens` record type carry a protolens
chain plus source and target schema hashes. This crate turns that record
into runnable translation: resolve the lens record from its at-uri, load
both schemas, compile the chain against the source schema, and apply
`get` / `put` / edit-lens / symmetric-lens pipelines against record
bodies.

## Architecture

```mermaid
flowchart LR
    subgraph resolvers["Resolver"]
        R1["InMemoryResolver"]
        R2["PdsResolver&lt;C&gt;"]
        R3["PanprotoVcsResolver&lt;C&gt;"]
        VR["VerifyingResolver<br/>(re-hash check)"]
        CR["CachingResolver (TTL)"]
    end

    subgraph clients["PdsClient"]
        C1["AtriumPdsClient"]
        C2["ReqwestPdsClient"]
        SW["SigningPdsWriter<br/>(DPoP · Bearer)"]
    end

    subgraph runtime["Lens runtime"]
        LOAD["SchemaLoader"]
        APPLY["apply_lens · apply_lens_put<br/>(panproto)"]
        OUT["target_record + complement"]
    end

    subgraph typed["Typed I/O"]
        RF["RecordFetcher::fetch&lt;R&gt;"]
        RP["RecordPublisher::create&lt;R&gt;"]
        COMP["fetcher_for_did ·<br/>publisher_for_did"]
    end

    PDS[("ATProto PDS")]

    R2 --> C1
    R2 --> C2
    SW -.wraps.-> C2
    VR -.wraps.-> R1
    VR -.wraps.-> R2
    VR -.wraps.-> R3
    CR -.wraps.-> R1
    CR -.wraps.-> R2
    CR -.wraps.-> R3
    R1 --> APPLY
    R2 --> APPLY
    R3 --> APPLY
    LOAD --> APPLY
    APPLY --> OUT
    C1 --> PDS
    C2 --> PDS
    RF --> C1
    RP --> C1
    COMP --> RF
    COMP --> RP
```

Three resolver shapes ship, all behind the narrow `Resolver` trait:

- **`InMemoryResolver`** — `HashMap<AtUri, PanprotoLens>` for tests.
- **`PdsResolver<C>`** — generic over a `PdsClient`; turns an at-uri
  into `(did, collection, rkey)` and delegates to the injected client.
- **`PanprotoVcsResolver<C>`** — generic over a `PanprotoVcsClient`;
  treats an at-uri as a ref, resolves to an object-hash, fetches the
  object body.

Two shipped PDS clients: `AtriumPdsClient` (typed XRPC via atrium) and
`ReqwestPdsClient` (raw reqwest). `VerifyingResolver<R, H>` wraps any
resolver and re-hashes the returned body against the lens record's
`object_hash` to reject content-hash mismatches. `CachingResolver<R>`
adds a TTL cache. `SigningPdsWriter<P>` layers DPoP / Bearer auth over
`ReqwestPdsClient` for authenticated writes.

## Usage

```rust
use idiolect_lens::{
    ApplyLensInput, InMemoryResolver, InMemorySchemaLoader, apply_lens,
};
use panproto_schema::Protocol;

let out = apply_lens(
    &resolver,                  // any Resolver impl
    &schema_loader,             // any SchemaLoader impl
    &Protocol::default(),
    ApplyLensInput {
        lens_uri: "at://did:plc:x/dev.panproto.schema.lens/l1".into(),
        source_record: source_json,
        source_root_vertex: None,
    },
).await?;
// out.target_record: the translated body.
// out.complement:    the data `get` discarded, needed by `put` to
//                    reconstruct the source.
```

Typed read + write helpers built over `PdsClient` / `PdsWriter`:
`RecordFetcher::fetch<R: Record>`, `RecordFetcher::list_records<R>`,
`RecordPublisher::create<R>`. `fetcher_for_did` and `publisher_for_did`
compose an `IdentityResolver` with a `ReqwestPdsClient` so callers go
from DID to typed writes in one call.

## Feature flags

| Flag | Default | Effect |
| ---- | ------- | ------ |
| `pds-atrium` | off | `AtriumPdsClient` via atrium-api + atrium-xrpc-client. |
| `pds-reqwest` | off | `ReqwestPdsClient` via raw reqwest. |
| `pds-resolve` | off | `fetcher_for_did` / `publisher_for_did` composition helpers. Implies `pds-reqwest`. |
| `dpop-p256` | off | `P256DpopProver` (ES256 DPoP via the `p256` crate). Implies `pds-reqwest`. |
| `pds-smoke-test` | off | Live-network test against a public PDS. Intentionally off in CI. |

## Related

- [`idiolect-records`](../idiolect-records) — the `PanprotoLens` record
  type lives here.
- [`idiolect-identity`](../idiolect-identity) — DID resolution the
  `pds-resolve` helpers compose against.
- [`idiolect-migrate`](../idiolect-migrate) — migration layer sitting on
  top of `apply_lens`.
