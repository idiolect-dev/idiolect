# idiolect-cli

The shell interface for community workspaces, schema translation, ATProto
records, and Idiolect services.

## What it does

The `idiolect` binary lets people use the repository's libraries without
writing Rust. It creates and diagnoses community workspaces, analyzes and
reviews schema changes, signs releases, tracks migrations, and exports a
portable copy. It also resolves DIDs, fetches and publishes records, manages
PDS sessions, queries an orchestrator, composes encounter records, and runs
lens verifications. Its orchestrator subcommands are generated from
[`orchestrator-spec/queries.json`](../../orchestrator-spec/queries.json)
so the CLI and HTTP API share one command inventory.

| Input | What the command does | Output |
| --- | --- | --- |
| Workspace paths and schema files | Runs the community change and release lifecycle | Human summary plus JSON artifacts on disk |
| DID or AT-URI | Resolves an identity or fetches a record from its PDS | Formatted JSON |
| Stored session plus record JSON | Validates and publishes an authenticated record | Created-record response |
| Orchestrator query and filters | Calls the read-only catalog API | Formatted JSON query result |
| Lens, schemas, and test data | Runs a verification method | Verification record JSON |

Use the CLI for work that belongs in local files, shell scripts, CI jobs, or
Git review. Use the underlying crates when an application needs the same
operations in-process.

## Architecture

```mermaid
flowchart LR
    USER["user shell"]
    subgraph cli["idiolect"]
        PARSER["subcommand parser"]
        RES["resolve"]
        FET["fetch"]
        ORCSUB["orchestrator (generated)"]
        ENC["encounter"]
        AUTH["oauth"]
        PUB["publish"]
        VER["verify"]
        COM["community lifecycle<br/>init · propose · review · release · migrate · export"]
    end
    OSPEC["orchestrator-spec/queries.json"]
    CG["idiolect-codegen"]

    ID["idiolect-identity"]
    LENSP["idiolect-lens<br/>(ReqwestPdsClient · fetcher_for_did)"]
    STORE[("session store")]
    WORK[("community workspace")]
    PDS[("ATProto PDS")]
    VERIFY["idiolect-verify"]
    ORCHTTP["orchestrator HTTP API"]

    USER --> PARSER
    PARSER --> RES --> ID
    PARSER --> FET --> LENSP --> PDS
    PARSER --> ORCSUB --> ORCHTTP
    PARSER --> ENC
    PARSER --> AUTH --> STORE
    AUTH --> PDS
    PARSER --> PUB --> STORE
    PUB --> PDS
    PARSER --> VER --> VERIFY
    PARSER --> COM --> WORK
    OSPEC --> CG -.emits subcommands.-> ORCSUB
```

Commands that return records or query results print formatted JSON to stdout.
Pipe those results through `jq` for further filtering.

## Install

```sh
cargo install --path crates/idiolect-cli
```

Binary archives for every release ship on the
[releases page](https://github.com/idiolect-dev/idiolect/releases) for
Linux (x86_64, aarch64) and macOS (x86_64, aarch64).

## Usage

```sh
# Create and inspect a community workspace.
idiolect init --workspace neighborhood-archive \
  --name "Neighborhood Archive" --did did:plc:replace-me
idiolect doctor --workspace neighborhood-archive

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

# Compose an encounter record interactively. The output is JSON on
# stdout; pipe into a record creator to publish.
idiolect encounter record \
  --lens at://did:plc:x/dev.panproto.schema.lens/l1 \
  --source-schema at://did:plc:x/dev.panproto.schema.schema/s1

# Store, inspect, and remove authenticated PDS sessions. Omit
# --app-password to read ATPROTO_APP_PASSWORD from the environment.
idiolect oauth login --handle alice.example.com --pds-url https://bsky.social
idiolect oauth list
idiolect oauth logout --did did:plc:alice

# Validate a local record body and publish it with a stored session.
idiolect publish encounter --record encounter.json
idiolect publish verification --record verification.json \
  --rkey 3l5example --did did:plc:alice

# Run a verification and print the resulting verification record.
idiolect verify roundtrip-test --lens at://did:plc:x/dev.panproto.schema.lens/l1
idiolect verify property-test \
  --lens at://did:plc:x/dev.panproto.schema.lens/l1 \
  --corpus fixtures/corpus.json --budget 200
idiolect verify static-check --lens at://did:plc:x/dev.panproto.schema.lens/l1
idiolect verify coercion-law \
  --lens at://did:plc:x/dev.panproto.schema.lens/l1 \
  --vcs-url https://vcs.example.com --standard json
```

## Boundaries and design choices

- The orchestrator query spec drives the `orchestrator` subcommand
  dispatcher. The next codegen run adds a CLI subcommand for each new query.
- `resolve` and `fetch` use public endpoints. `oauth login` stores a PDS
  bearer session under `~/.config/idiolect/sessions/`, and `publish` uses
  that session for authenticated writes. Set `IDIOLECT_SESSION_DIR` to use
  a different store.
- The orchestrator API is read-only and public by design.
- Community lifecycle commands write local workspace artifacts. They do not
  publish those artifacts to a PDS automatically.

## Related

- [`idiolect-identity`](../idiolect-identity): `resolve` uses
  this crate.
- [`idiolect-lens`](../idiolect-lens): `fetch` uses `ReqwestPdsClient`
  via `fetcher_for_did`.
- [`idiolect-orchestrator`](../idiolect-orchestrator): defines the HTTP API
  the `orchestrator` subcommands query.
