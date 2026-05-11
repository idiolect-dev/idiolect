# idiolect-orchestrator

> **Source:** [`crates/idiolect-orchestrator/`](https://github.com/idiolect-dev/idiolect/tree/main/crates/idiolect-orchestrator)
>
> This crate is `publish = false` and is not on docs.rs. The
> authoritative reference is the source above plus the rustdoc
> built locally with `cargo doc -p idiolect-orchestrator --features daemon --open`.

Read-only HTTP query API over a record catalog. Driven by
`orchestrator-spec/queries.json`; codegen emits the routes plus
the matching CLI dispatcher.

Because the crate is `publish = false`, depend via git or path:

```toml
[dependencies]
idiolect-orchestrator = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.8.0", features = ["daemon", "catalog-sqlite", "query-http"] }
```

## Public surface

The crate exposes:

- `Catalog` — an in-memory struct holding `Entry<R>` slots per
  record kind, with typed iterators (`encounters()`, `bounties()`,
  `verifications()`, ...).
- `CatalogRef` — a shareable handle around the catalog that the
  HTTP handlers and the indexer's record handler both hold.
- `SqliteCatalogStore` (under `catalog-sqlite`) — persistent
  catalog backing.
- `CatalogHandler` — the indexer's `RecordHandler<IdiolectFamily>`
  impl that upserts every accepted record into the catalog.
- `AppState` plus `http_router()` — axum router wiring under
  `query-http`.
- Theory-resolver and predicate-evaluator helpers used by
  generated query handlers.

## HTTP endpoints

Every handler under the `v1` prefix is generated from
`orchestrator-spec/queries.json`. The current shipped routes:

| Path | Returns |
| --- | --- |
| `GET /healthz`, `GET /readyz` | Liveness + readiness. |
| `GET /metrics` | Prometheus exposition. |
| `GET /v1/stats` | Per-kind record counts. |
| `GET /v1/bounties/open` | Cataloged bounties whose status is open / claimed / unset. |
| `GET /v1/bounties/want-lens?...` | Bounties whose `wants` is a specific lens. |
| `GET /v1/bounties/by-requester?requester_did=...` | Bounties by requester. |
| `GET /v1/adapters?framework=...` | Adapters by framework. |
| `GET /v1/adapters/by-invocation-protocol?...` | Adapters by invocation-protocol kind. |
| `GET /v1/adapters/with-verification?...` | Adapters with at least one verification record. |
| `GET /v1/recommendations` | Recommendations starting from a given source schema. |
| `GET /v1/verifications?lens_uri=...` | Verifications for a specific lens. |
| `GET /v1/verifications/by-kind?...` | Verifications by kind. |
| `GET /v1/communities?...` | Communities for a member DID. |
| `GET /v1/communities/by-name?...` | Communities by name. |
| `GET /v1/dialects/for-community?...` | Dialects owned by a community. |
| `GET /v1/beliefs/about?...` | Beliefs whose subject is a given record. |
| `GET /v1/beliefs/by-holder?...` | Beliefs by holder DID. |
| `GET /v1/vocabularies/by-world?...` | Vocabularies declared with a given `world`. |
| `GET /v1/vocabularies/by-name?...` | Vocabularies by name. |

The full path-and-flag table for each endpoint is generated; see
[`orchestrator-spec/queries.json`](https://github.com/idiolect-dev/idiolect/blob/main/orchestrator-spec/queries.json)
for the authoritative list.

## Errors

`OrchestratorError` flattens catalog and HTTP errors;
`OrchestratorResult<T>` is its alias.

## Feature flags

| Feature | Adds |
| --- | --- |
| `catalog-sqlite` | SQLite-backed catalog store. |
| `query-http` | HTTP server (axum-based). |
| `daemon` | The `idiolect-orchestrator` binary, wiring the indexer plus catalog plus HTTP API with a tapped-backed firehose. |

## Observability

`/metrics` exposes Prometheus counters and histograms for the
catalog and per-endpoint latency. Structured `tracing` logs at
`info` level for accepted requests; `debug` for query
internals. The exact metric names are defined in
`crates/idiolect-orchestrator/src/http.rs`.
