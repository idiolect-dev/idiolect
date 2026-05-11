# Reference

Per-symbol detail. Use the navigation to jump to a specific crate,
lexicon, CLI subcommand, or HTTP endpoint.

| Section | Contents |
| --- | --- |
| [Crates](./crates/index.md) | One page per workspace crate, with public types, traits, error variants, and feature flags. |
| [Lexicons](./lexicons/index.md) | One page per `dev.idiolect.*` lexicon, with field-by-field shape. |
| [CLI](./cli.md) | Every shipped `idiolect` subcommand, its flags, and its output. |
| [HTTP query API](./http-api.md) | Every endpoint exposed by the orchestrator, request and response shape. |
| [Stability and versioning](./stability.md) | The pre-1.0 stability policy. |

The reference covers the `0.8.0` release. For older releases, see
the
[release archive](https://github.com/idiolect-dev/idiolect/releases).

## Authority policy

This section is editorial. For Rust crates, the authoritative
per-symbol reference is the rendered rustdoc on docs.rs (linked at
the top of every crate page). For lexicons, the authoritative
shape is the JSON document under `lexicons/dev/idiolect/`. When
this book and either source disagree, the source wins; please
file an issue.
