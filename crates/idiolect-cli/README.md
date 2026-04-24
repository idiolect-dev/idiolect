# idiolect-cli

Command-line tool wrapping the library crates.

## Overview

Single binary named `idiolect`. Subcommands cover the three common
operations: resolve a DID, fetch a record from its home PDS, and query
a running local orchestrator. The orchestrator subcommand dispatcher is
**generated** from
[`orchestrator-spec/queries.json`](../../orchestrator-spec/queries.json)
so the CLI never drifts out of sync with the HTTP API it's targeting.

## Architecture

```mermaid
flowchart LR
    USER["user shell"]
    subgraph cli["idiolect"]
        CLAP["clap parser"]
        RES["resolve"]
        FET["fetch"]
        ORCSUB["orchestrator (generated)"]
    end
    OSPEC["orchestrator-spec/queries.json"]
    CG["idiolect-codegen"]

    ID["idiolect-identity"]
    LENSP["idiolect-lens<br/>(ReqwestPdsClient · fetcher_for_did)"]
    ORCHTTP["orchestrator HTTP API"]

    USER --> CLAP
    CLAP --> RES --> ID
    CLAP --> FET --> LENSP
    CLAP --> ORCSUB --> ORCHTTP
    OSPEC --> CG -.emits subcommands.-> ORCSUB
```

Every command prints pretty-printed JSON to stdout — pipe through `jq`
for further filtering.

## Install

```sh
cargo install --path crates/idiolect-cli
# Or, once released:
cargo install idiolect-cli
```

Binary archives for every release ship on the
[releases page](https://github.com/idiolect-dev/idiolect/releases) for
Linux (x86_64, aarch64) and macOS (x86_64, aarch64).

## Usage

```sh
# Identity resolution.
idiolect resolve did:plc:alice

# Fetch a record body (uses the DID's own PDS).
idiolect fetch at://did:plc:alice/dev.idiolect.bounty/3l5

# Orchestrator queries (default base URL http://localhost:8787).
idiolect orchestrator stats
idiolect orchestrator bounties                            # open bounties
idiolect orchestrator bounties --requester did:plc:alice
idiolect orchestrator adapters --framework hasura
idiolect orchestrator recommendations
idiolect orchestrator verifications --lens at://did:plc:x/dev.panproto.schema.lens/l1

# Point at a non-default orchestrator.
idiolect orchestrator stats --url https://orch.example.com
```

## Design notes

- The `orchestrator` subcommand dispatcher is emitted from the
  orchestrator's query spec; adding a query to the spec produces a new
  CLI subcommand automatically on the next codegen run.
- Authentication is not wired: `resolve` and `fetch` hit public
  endpoints; the orchestrator API is read-only and public by design.
  Authenticated writes are
  [`idiolect-lens::SigningPdsWriter`](../idiolect-lens)'s responsibility.

## Related

- [`idiolect-identity`](../idiolect-identity) — `resolve` backs onto
  this crate.
- [`idiolect-lens`](../idiolect-lens) — `fetch` uses `ReqwestPdsClient`
  via `fetcher_for_did`.
- [`idiolect-orchestrator`](../idiolect-orchestrator) — the HTTP API
  the `orchestrator` subcommands query.
