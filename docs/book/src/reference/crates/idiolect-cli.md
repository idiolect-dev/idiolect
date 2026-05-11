# idiolect-cli

> **Source:** [`crates/idiolect-cli/`](https://github.com/idiolect-dev/idiolect/tree/main/crates/idiolect-cli)
>
> This crate is `publish = false` and is not on docs.rs. The
> CLI is intended to be installed and run, not depended on as a
> library.

The `idiolect` command-line tool. Wraps the library crates so
operators and end users do not need to write Rust for common
operations.

```bash
cargo install --path crates/idiolect-cli
```

The CLI hardcodes the `idiolect-lens` features it needs
(`pds-reqwest`, `pds-resolve`); there are no CLI-level features
to set.

## Subcommand surface

```text
idiolect resolve <did>
idiolect fetch <at-uri>
idiolect orchestrator <subcommand>
idiolect encounter record [...]
idiolect version
idiolect help
```

The full reference is the [CLI reference](../cli.md).

## Why no clap

The CLI uses a hand-rolled subcommand parser. Two reasons:

1. The orchestrator subcommand surface is partly codegen-emitted
   from `orchestrator-spec/queries.json`. A clap surface would
   put two layers of declarative wiring on top of each other.
2. Compile time and binary size matter for a tool meant to be
   installed broadly.

## Codegen surface

The CLI's `orchestrator …` dispatcher is emitted from
`orchestrator-spec/queries.json` into
`crates/idiolect-cli/src/generated.rs`. Adding a query (per the
[orchestrator guide](../../guide/orchestrator.md)) regenerates
the dispatcher; the new subcommand becomes available
automatically.

The hand-written subcommands (`resolve`, `fetch`,
`encounter record`, `version`, `help`) live in `main.rs` and
`encounter.rs`.

## Output

All commands print pretty-printed JSON to stdout on success.
Errors go to stderr with `error: <message>`.

## Configuration

| Setting | Default | Override |
| --- | --- | --- |
| Orchestrator URL | `http://localhost:8787` | `--url` flag on `orchestrator` subcommands |
| Log level | `info` | `RUST_LOG` env var |

The CLI does not read a config file.
