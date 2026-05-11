# Tutorial

This tutorial walks one example end to end. By the time you finish
you will have:

1. installed the `idiolect` CLI and the `idiolect-records` /
   `idiolect-lens` crates,
2. fetched a real ATProto record and validated it against the
   shipped lexicon,
3. resolved a `dev.panproto.schema.lens` record and applied it to
   translate a source record into a target record,
4. run a verification runner against a lens and recorded the result,
5. published a `dev.idiolect.recommendation` record from your own
   PDS endorsing a lens path.

The chapters are linear. Each one starts from the state the previous
one left behind. If you want a task-shaped reference instead, jump
to the [Guides](../guide/index.md). If you want the underlying
theory, the [Concepts](../concepts/index.md) section is the place.

## Prerequisites

You need:

- Rust 1.95 or newer (`rustup default stable`).
- Cargo, with network access to crates.io.
- A working ATProto identity (a `did:plc:*` or `did:web:*`) for the
  publishing chapter. If you do not have one, the bsky.app sign-up
  flow gives you one. Most chapters work without it.
- About 30 minutes of focused time.

The rest of this tutorial assumes you are running commands from a
fresh terminal at the root of a Cargo workspace you control.
