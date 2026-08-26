# Install the CLI and fetch a lens

This chapter gets you from a fresh checkout to a live idiolect record in about
five minutes. You need Git, Rust 1.95 or later, and a network connection. You do
not need an ATProto account.

## Install from the checkout

Clone the repository and install the command-line interface (CLI):

```bash
git clone https://github.com/idiolect-dev/idiolect
cd idiolect
cargo install --locked --path crates/idiolect-cli
```

The first build may take a few minutes. Confirm that the installed binary comes
from this checkout:

```bash
idiolect version
```

```text
idiolect 0.12.1
```

## Resolve the project account

A [decentralized identifier (DID)](../glossary.md#did "Decentralized identifier")
names an account. Its DID document identifies the account's
[personal data server (PDS)](../glossary.md#pds "Personal data server"). Resolve
the idiolect project DID:

```bash
idiolect resolve did:plc:wdl4nnvxxdy4mc5vddxlm6f3
```

The JSON response should identify `idiolect.dev` and include a `pds_url`.
Resolution is a separate step because records move with an account when its PDS
changes.

## Fetch the tutorial lens

An [AT-URI](../glossary.md#at-uri "AT Protocol record address") identifies one
record by DID, collection, and record key. Fetch the published tutorial
[lens](../glossary.md#lens "Bidirectional schema translation"):

```bash
idiolect fetch \
  at://did:plc:wdl4nnvxxdy4mc5vddxlm6f3/dev.panproto.schema.lens/tutorial-rename-sort-string-to-text
```

The result is a real `dev.panproto.schema.lens` record. Its `sourceSchema` and
`targetSchema` fields identify the schemas it connects, while `blob` contains
the Panproto lens definition. Keep the checkout: the next chapter validates a
typed record with the same libraries the CLI uses.
