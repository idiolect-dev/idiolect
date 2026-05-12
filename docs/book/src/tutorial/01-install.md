# Install and resolve a record

The fastest way to get a working idiolect setup is to install the
CLI from source. The CLI links against the same library crates an
application would, so installing it also gives you the runtime
machinery for later chapters.

## Install the CLI

```bash
git clone https://github.com/idiolect-dev/idiolect
cd idiolect
cargo install --path crates/idiolect-cli
```

This compiles every crate the CLI depends on (`idiolect-records`,
`idiolect-identity`, `idiolect-lens`, plus their atproto transport
features) and drops an `idiolect` binary into `~/.cargo/bin`. The
build takes two to four minutes on a recent laptop. There is no
runtime dependency on the cloned tree after install; you can `cd`
anywhere.

Confirm it works:

```bash
idiolect version
```

```text
idiolect 0.8.0
```

## Resolve a DID

The first thing the runtime does on any record fetch is resolve a
DID to its PDS. `idiolect resolve` exposes that step on its own:

The project's own DID is a good first target:

```bash
idiolect resolve did:plc:wdl4nnvxxdy4mc5vddxlm6f3
```

```json
{
  "did": "did:plc:wdl4nnvxxdy4mc5vddxlm6f3",
  "method": "Plc",
  "handle": "idiolect.dev",
  "pds_url": "https://jellybaby.us-east.host.bsky.network",
  "also_known_as": ["at://idiolect.dev"]
}
```

The resolver uses
[`idiolect-identity`](../reference/crates/idiolect-identity.md)'s
`ReqwestIdentityResolver`. For `did:plc:*` it goes through
`plc.directory`; for `did:web:*` it fetches
`https://<host>/.well-known/did.json`. Both transports are built
on `reqwest`.

If the DID does not resolve, the CLI prints a structured error
on stderr (the message is `IdentityError`-shaped: the variant
plus the underlying transport message).

## Fetch a record

`idiolect fetch` takes an at-uri and returns the raw record body
(the `value` field of the xrpc response, not the response
envelope). The project DID has a tutorial lens record published;
fetching it exercises the runtime path end to end:

```bash
idiolect fetch \
  at://did:plc:wdl4nnvxxdy4mc5vddxlm6f3/dev.panproto.schema.lens/tutorial-rename-sort-string-to-text
```

The response is a `dev.panproto.schema.lens` body carrying the
protolens chain (`blob`), the source and target schema at-uris,
the optic class (`iso`), and the content hash. Chapter 3 takes
this exact record and runs it through `apply_lens`.

The fetch goes through the same `PdsClient` impl `apply_lens`
uses, so any record you can fetch this way you can also feed
into the lens runtime.

## Where things are stored

| Artifact | Location |
| --- | --- |
| The `idiolect` binary | `~/.cargo/bin/idiolect` |
| Cached crate sources | `~/.cargo/registry/src/` |
| Cached PLC responses | `~/.cache/idiolect/plc/` (only if you set `IDIOLECT_CACHE_DIR`) |

You do not need to wire any of this up by hand. The next chapter
takes the raw json `idiolect fetch` returned and validates it
against the shipped lexicon.
