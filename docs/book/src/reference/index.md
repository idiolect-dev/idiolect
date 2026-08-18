# Reference

Use this section to look up exported APIs, record fields, commands,
and HTTP contracts. Task procedures remain in the
[guides](../guide/index.md).

| Section | Contents |
| --- | --- |
| [Crates](./crates/index.md) | One page per workspace crate, with public types, traits, error variants, and feature flags. |
| [Lexicons](./lexicons/index.md) | One page per `dev.idiolect.*` lexicon, with field-by-field shape. |
| [CLI](./cli.md) | Every shipped `idiolect` subcommand, its flags, and its output. |
| [HTTP query API](./http-api.md) | Every endpoint exposed by the orchestrator, request and response shape. |
| [Stability and versioning](./stability.md) | The pre-1.0 stability policy. |

The reference covers idiolect 0.11.1 with panproto 0.70.1. For older
releases, use the
[release archive](https://github.com/idiolect-dev/idiolect/releases).

## Extension and API path

For an advanced integration, follow the lookup path that matches the
extension boundary:

| Extension boundary | Start here | Then inspect |
| --- | --- | --- |
| Add a record family or emitter target | [`idiolect-codegen`](./crates/idiolect-codegen.md) | [`RecordFamily`](./crates/idiolect-records.md#family) and the emit functions |
| Add a stream, handler, or cursor backend | [`idiolect-indexer`](./crates/idiolect-indexer.md) | Trait signatures, feature flags, and error variants |
| Add a lens resolver or schema loader | [`idiolect-lens`](./crates/idiolect-lens.md) | Resolver, loader, apply-input, and apply-output types |
| Add an observation or verification method | [`idiolect-observer`](./crates/idiolect-observer.md) or [`idiolect-verify`](./crates/idiolect-verify.md) | Generated taxonomies and implementation traits |
| Integrate over process boundaries | [CLI](./cli.md) or [HTTP API](./http-api.md) | Exact flags, query parameters, response envelopes, and errors |

For wire-level extensions, begin with the [lexicon index](./lexicons/index.md)
and follow each page's source link to the authoritative JSON.

## Authority policy

For published Rust crates, rustdoc on docs.rs is authoritative. For
workspace-only crates, build rustdoc from the current checkout. The
JSON under `lexicons/dev/idiolect/` defines record shape. If this book
disagrees with either source, use the source and file an issue.
