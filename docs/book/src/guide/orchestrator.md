# Run the orchestrator HTTP API

[`idiolect-orchestrator`](../reference/crates/idiolect-orchestrator.md)
is a read-only HTTP query API over a record catalog. It pairs
with the indexer (which writes the catalog through
`CatalogHandler`) and exposes the result over a small set of
typed query endpoints.

## What it serves

- `GET /healthz`, `GET /readyz` — liveness and readiness.
- `GET /metrics` — Prometheus exposition.
- `GET /v1/stats` — record counts per kind.
- One endpoint per declarative query under
  `orchestrator-spec/queries.json`. Snapshot at v0.8.0:
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

The `daemon` feature pulls in `catalog-sqlite`, `query-http`,
the indexer's tapped firehose, and the SQLite cursor store. Run:

```bash
idiolect-orchestrator \
  --catalog ./catalog.sqlite \
  --bind 0.0.0.0:8787
```

The exact CLI surface is documented by the daemon's `--help`.
The catalog is populated by the indexer that ships with the
daemon; the orchestrator reads from the same SQLite file.

## Query it

```bash
curl -s http://localhost:8787/v1/stats | jq
curl -s 'http://localhost:8787/v1/bounties/open' | jq
curl -s 'http://localhost:8787/v1/adapters?framework=hasura' | jq
curl -s 'http://localhost:8787/v1/verifications?lens_uri=at://...' | jq
```

Every shipped query has a CLI subcommand under
`idiolect orchestrator <subcommand>` that calls the same
endpoint:

```bash
idiolect orchestrator bounties
idiolect orchestrator adapters --framework hasura
idiolect orchestrator verifications --lens_uri at://...
```

The CLI dispatcher (in
`crates/idiolect-cli/src/generated.rs`) is generated from the
same spec the HTTP routes are.

## Add a query

Queries live in `orchestrator-spec/queries.json` (a single JSON
document with a top-level `queries` array). To add one:

1. Add a new entry to the array. Each entry declares the
   query's name, description, parameters, predicate (a
   panproto-expr expression), and the record kind it iterates
   over.
2. Run `cargo run -p idiolect-codegen`.
3. The generated tree picks up the new query: HTTP route,
   query-string parser, response shape, and CLI subcommand.

The hand-written part is the panproto-expr predicate inside the
spec entry; the generated tree handles routing, parameter
parsing, and response encoding.

## Observability

The orchestrator exposes `/metrics` in Prometheus exposition
format. Plus structured `tracing` logs. The exact metric names
and label sets are defined in
`crates/idiolect-orchestrator/src/http.rs`; see the source for
the live list.

## Deployment

A pre-built container image ships at
`ghcr.io/idiolect-dev/orchestrator:<version>` per release. The
image is signed with sigstore keyless; verification policy is
in
[`docs/ci-cd.md`](https://github.com/idiolect-dev/idiolect/blob/main/docs/ci-cd.md).
