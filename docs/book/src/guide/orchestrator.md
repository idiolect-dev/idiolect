# Run the orchestrator HTTP API

[`idiolect-orchestrator`](../reference/crates/idiolect-orchestrator.md)
serves a read-only HTTP API over a record
[catalog](../glossary.md#catalog "An indexed collection of AT Protocol records").
`CatalogHandler` fills the catalog from the indexer; the HTTP server
queries that same state.

## What it serves

- `GET /healthz`, `GET /readyz` — liveness and readiness.
- `GET /metrics` — Prometheus exposition.
- `GET /v1/stats` — record counts per kind.
- One pair of REST and XRPC endpoints per declarative query in
  `orchestrator-spec/queries.json`. The 0.11.1 surface includes:
  bounties (open, want-lens, by-requester), adapters (by
  framework, by invocation protocol, with verification),
  recommendations (starting from a source schema), verifications
  (by lens, by kind), communities (by member, by name),
  dialects (for community), beliefs (about a record, by holder),
  and vocabularies (with world, by name).

The full surface is in the
[HTTP query API](../reference/http-api.md) reference.

## Run it

The shipped daemon binary lives behind the `daemon` feature:

```bash
cargo install --path crates/idiolect-orchestrator \
    --features daemon
```

The `daemon` feature pulls in `catalog-sqlite`, `query-http`, the
indexer's tapped firehose, and the SQLite cursor store. Configure and
run it with environment variables:

```bash
IDIOLECT_TAP_URL=http://localhost:2480 \
IDIOLECT_ORCHESTRATOR_DB=./catalog.sqlite \
IDIOLECT_ORCHESTRATOR_CURSORS=./cursors.sqlite \
IDIOLECT_HTTP_ADDR=127.0.0.1:8787 \
idiolect-orchestrator
```

Set `IDIOLECT_TAP_ADMIN_PASSWORD` if the tap requires it and
`IDIOLECT_SUBSCRIPTION_ID` when several subscriptions share a cursor
database. The daemon does not parse `--catalog` or `--bind` flags.

## Query it

```bash
curl -s http://localhost:8787/v1/stats | jq
curl -s 'http://localhost:8787/v1/bounties/open' | jq
curl -s 'http://localhost:8787/v1/adapters?framework=hasura' | jq
curl -s 'http://localhost:8787/v1/verifications?lens_uri=at://...' | jq
curl -s 'http://localhost:8787/v1/verifications/sufficient?lens_uri=at://...&kinds=roundtrip-test&hold=true' | jq
```

The generated CLI exposes the spec entries that declare a `cli`
mapping:

```bash
idiolect orchestrator bounties
idiolect orchestrator adapters --framework hasura
idiolect orchestrator verifications --lens_uri at://...
```

Other HTTP queries have no CLI subcommand. The dispatcher in
`crates/idiolect-cli/src/generated.rs` is generated from the
same spec as the HTTP routes.

## Add a query

Queries live in `orchestrator-spec/queries.json` (a single JSON
document with a top-level `queries` array). To add one:

1. Add a new entry to the array. Each entry declares the
   query's name, description, parameters, predicate (a
   panproto-expr expression), and the record kind it iterates
   over.
2. Run `cargo run -p idiolect-codegen`.
3. The generated tree picks up the HTTP route, XRPC alias,
   query-string parser, and response shape. Add a `cli` mapping if the
   query also needs a CLI subcommand.

The hand-written part is the panproto-expr predicate inside the
spec entry. The generated tree handles routing, parameter
parsing, and response encoding.

## Observability

The orchestrator exposes `/metrics` in Prometheus exposition format
and emits structured `tracing` logs. The metric names and label sets
are defined in `crates/idiolect-orchestrator/src/http.rs`.

## Deployment

A pre-built container image ships at
`ghcr.io/idiolect-dev/orchestrator:<version>` per release. The
image is signed with sigstore keyless. Verification policy is
in
[`docs/ci-cd.md`](https://github.com/idiolect-dev/idiolect/blob/main/docs/ci-cd.md).
