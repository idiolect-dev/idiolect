# Tutorial

This tutorial follows one published Panproto lens from discovery to
recommendation. The route is linear, and the first chapter produces a live,
read-only result in about five minutes:

1. [Install the CLI and fetch a lens](./01-install.md) from the idiolect project
   account.
2. [Validate a typed record](./02-validate.md) through the generated NSID
   dispatcher.
3. [Apply the lens](./03-apply-lens.md) to a small JSON record with Panproto
   0.71.0.
4. [Verify the round trip](./04-verify.md) over a three-record corpus.
5. [Publish a recommendation](./05-publish.md) and its community record to your
   [personal data server (PDS)](../glossary.md#pds "Personal data server").

Each chapter starts from the repository state left by the previous one. The
first four chapters do not require an ATProto account; Chapter 5 does, because
it writes public records. Use the [Guides](../guide/index.md) for task-specific
procedures and the [Concepts](../concepts/index.md) for the underlying model.

## Prerequisites

You need Git, Rust 1.95 or later, Cargo, and network access. Clone the idiolect
repository in Chapter 1 and run the remaining commands from its root. If you
plan to complete Chapter 5, you also need an ATProto account and an app
password for that account.
