# CLI

`idiolect` is the command-line tool. The full surface is below.

## Top-level subcommands

```text
idiolect resolve <did>
idiolect fetch <at-uri>
idiolect orchestrator <subcommand>
idiolect encounter record [...]
idiolect oauth login | list | logout [...]
idiolect publish <kind> --record <path> [...]
idiolect verify <kind> [...]
idiolect version          # also accepts --version, -V
idiolect help [<sub>]     # also accepts --help, -h
```

The hand-written subcommands (`resolve`, `fetch`, `oauth`,
`publish`, `verify`, `encounter`) live in their own modules
under `crates/idiolect-cli/src/`. The `orchestrator` subcommand
is generated from `orchestrator-spec/queries.json` and routes
its calls to the orchestrator's HTTP API.

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

## `oauth`

Authenticated PDS sessions for `idiolect publish` and downstream
record-writing flows.

```text
idiolect oauth login  --handle HANDLE --app-password PASSWORD [--pds-url URL]
idiolect oauth list
idiolect oauth logout --did DID
```

`login` exchanges `(handle, app-password)` for an access JWT via
`com.atproto.server.createSession` and persists `{did, handle,
pds_url, access_jwt, refresh_jwt}` as one JSON file per DID
under `$IDIOLECT_SESSION_DIR` (default
`~/.config/idiolect/sessions/`). `--app-password` may be passed
as a flag or via `ATPROTO_APP_PASSWORD` / `ATPROTO_PASSWORD`
env vars to avoid leaking into shell history.

`list` enumerates every stored session as a JSON array of
`{did, handle, pds_url}` triples.

`logout` deletes the session file for `--did`; a missing file is
not an error.

## `publish <kind>`

```text
idiolect publish <kind> --record <path> [--rkey RKEY] [--did DID]
```

Loads a JSON file, validates it against the typed `Record` impl
for `<kind>` (which can be either the unqualified kind like
`recommendation` or the fully-qualified NSID like
`dev.idiolect.recommendation`), splices in a `$type`
discriminator, and POSTs `com.atproto.repo.createRecord` using
the stored session's bearer auth.

When `--did` is omitted the CLI picks the first stored session.
When `--rkey` is omitted the CLI generates a TID-shaped key.

Prints `{uri, cid}` of the published record on success.

## `verify <kind>`

```text
idiolect verify roundtrip-test  --lens AT_URI [--corpus PATH]   [--pds-url URL] [--verifier-did DID]
idiolect verify property-test   --lens AT_URI  --corpus PATH    [--budget N]    [--pds-url URL] [--verifier-did DID]
idiolect verify static-check    --lens AT_URI                   [--pds-url URL] [--verifier-did DID]
idiolect verify coercion-law    --lens AT_URI  --vcs-url URL    --standard STD  [--version V] [--violation-threshold N] [--verifier-did DID]
```

Runs the shipped `VerificationRunner` for the named kind against
the live PDS, prints the typed `Verification` record as JSON,
and exits non-zero on `Falsified` / `Inconclusive` so CI surfaces
failures.

The corpus file (for `roundtrip-test` and `property-test`) may
be a JSON array or JSON Lines. `property-test`'s generator
cycles through the corpus by index, so `--budget` controls how
many cases run.

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

## Roadmap

The shipped login path uses app passwords in legacy Bearer
mode (`com.atproto.server.createSession` plus
`Authorization: Bearer <token>`). The full OAuth + DPoP flow
via `atrium-oauth` (browser handoff, PKCE, DPoP-bound tokens)
is the next-iteration login UX; the library `OAuthSession`
shape and `OAuthTokenStore` trait are already in place to
receive whatever the dance returns.
