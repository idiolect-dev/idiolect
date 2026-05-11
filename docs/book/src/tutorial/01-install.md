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

```bash
idiolect resolve did:plc:idiolect.dev
```

```json
{
  "did": "did:plc:idiolect.dev",
  "method": "Plc",
  "handle": "idiolect.dev",
  "pds_url": "https://shimeji.us-east.host.bsky.network",
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
(the `value` field of the xrpc response, not the response envelope):

```bash
idiolect fetch \
  at://did:plc:idiolect.dev/dev.idiolect.dialect/canonical
```

```json
{
  "$type": "dev.idiolect.dialect",
  "name": "idiolect canonical",
  "issuingCommunity": "at://did:plc:idiolect.dev/dev.idiolect.community/canonical",
  "...": "..."
}
```

The fetch goes through the same `PdsClient` impl `apply_lens` uses,
so anything you can fetch this way you can also feed into the lens
runtime in [chapter 3](./03-apply-lens.md).

## Where things are stored

| Artifact | Location |
| --- | --- |
| The `idiolect` binary | `~/.cargo/bin/idiolect` |
| Cached crate sources | `~/.cargo/registry/src/` |
| Cached PLC responses | `~/.cache/idiolect/plc/` (only if you set `IDIOLECT_CACHE_DIR`) |

You do not need to wire any of this up by hand. The next chapter
takes the raw json `idiolect fetch` returned and validates it
against the shipped lexicon.
