# CLI

`idiolect` is the command-line tool. The full surface is below.

## Top-level subcommands

```text
idiolect resolve <did>
idiolect fetch <at-uri>
idiolect orchestrator <subcommand>
idiolect encounter record [...]
idiolect version          # also accepts --version, -V
idiolect help [<sub>]     # also accepts --help, -h
```

The two hand-written subcommands (`resolve`, `fetch`) live in
`main.rs` and call directly into `idiolect-identity` and
`idiolect-lens`. The `orchestrator` subcommand is generated
from `orchestrator-spec/queries.json` and routes its calls to
the orchestrator's HTTP API.

## `resolve`

```text
idiolect resolve <did>
```

Resolve a DID via `idiolect-identity::ReqwestIdentityResolver`.
Prints `{ did, method, handle, pds_url, also_known_as }`.

## `fetch`

```text
idiolect fetch <at-uri>
```

Fetch a record body via `com.atproto.repo.getRecord` (under the
hood: `idiolect-lens::fetcher_for_did`). Prints the record value
as JSON.

## `orchestrator <subcommand>`

The orchestrator dispatcher accepts a flat path-and-flags shape
generated from `orchestrator-spec/queries.json`. Each query
maps onto a subcommand and a flag set; the CLI translates the
invocation into an HTTP path and calls the orchestrator at
`--url` (default `http://localhost:8787`).

The current subcommands (run `idiolect help orchestrator` for
the live list):

| Command | Calls |
| --- | --- |
| `idiolect orchestrator adapters --framework <NAME>` | `GET /v1/adapters?framework=...` |
| `idiolect orchestrator bounties` | `GET /v1/bounties/open` |
| `idiolect orchestrator bounties --requester_did <DID>` | `GET /v1/bounties/by-requester?requester_did=...` |
| `idiolect orchestrator recommendations` | `GET /v1/recommendations` |
| `idiolect orchestrator verifications --lens_uri <AT-URI>` | `GET /v1/verifications?lens_uri=...` |

Adding a query to the spec extends both the HTTP and the CLI
surface; see [Run codegen](../guide/codegen.md). The CLI's
top-level `--url` flag overrides the default orchestrator base.

## `encounter record`

```text
idiolect encounter record \
  --lens <AT-URI> --source-schema <AT-URI> [--target-schema <AT-URI>] \
  [--vocab <AT-URI>] [--kind <KIND>] [--visibility <V>] [--text-only]
```

Publishes a `dev.idiolect.encounter` record. The exact flag set
is in `crates/idiolect-cli/src/encounter.rs`.

## Output format

All commands print pretty-printed JSON to stdout on success.
Errors go to stderr:

```text
error: <message>
```

Pipe stdout to `jq` for further processing.

## Planned subcommands

A few operations the book references on the library side are
planned for the CLI but not shipped at v0.8.0:

- `idiolect oauth login --handle <HANDLE>` — would walk the
  OAuth dance via `atrium-oauth-client` and persist the
  resulting session through `idiolect_oauth::OAuthTokenStore`.
  Today the dance is driven programmatically.
- `idiolect verify <kind> [...]` — would run any shipped
  `VerificationRunner` from the command line and publish the
  result. Today runners are library-only.
- `idiolect publish <kind> --record <path>` — would load a
  typed record from JSON and publish it under the active
  session. Today publishing goes through
  `idiolect_lens::RecordPublisher` from Rust.

These gaps are expected pre-1.0: the library surface is the
stable contract, and CLI subcommands accumulate as the
hyperdeclarative spec grows.
