# Stability and versioning

idiolect is pre-1.0. Minor releases in the `0.x` series may break Rust
APIs, [lexicon](../glossary.md#lexicon "A schema document in the AT Protocol Lexicon language")
shapes, wire formats, HTTP routes, and CLI flags.

Pin to an exact version if you depend on this project. Read the
[changelog](https://github.com/idiolect-dev/idiolect/blob/main/CHANGELOG.md)
before bumping.

## What changes between minor versions

Pre-1.0:

- **Trait signatures** can tighten or widen between minor versions.
- **Lexicon shapes** can change. Wire-compatible changes go through
  the [lexicon-evolution policy](../concepts/lexicon-evolution.md).
  Breaking changes ship with a derived migration lens.
- **CLI subcommands** can rename or reshape. The output JSON shape
  is more stable than the flag surface.
- **HTTP routes** can change under the `v1` prefix between minor
  versions. After 1.0 they will not.

## Current commitments

- The `dev.idiolect.*` namespace stays as is. NSID renames are
  possible but extraordinarily unusual. One would ship with a
  deprecation note in `dev.idiolect.dialect#deprecations`.
- A breaking lexicon revision follows the evolution policy and should
  ship with a migration lens. This process does not guarantee that an
  older record validates unchanged against every later minor release.
- Generated Rust and TypeScript remain derived from the checked-in
  lexicons, and `idiolect-codegen --check` verifies that relationship.

## What changes at 1.0

- Breaking changes between minor versions stop. Breaking changes
  ship in major versions only.
- The lexicon-evolution `check-compat` gate flips from advisory to
  a hard fail.
- The HTTP API's `v1` prefix becomes a stability commitment. New
  endpoints are additive.
- Trait signatures in `idiolect-records`, `idiolect-lens`,
  `idiolect-indexer`, and `idiolect-orchestrator` become
  semver-stable.

The project has not committed to a 1.0 release date.

## Reading the changelog

The project follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) plus
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Every
release section has six fixed buckets:

| Bucket | Contents |
| --- | --- |
| **Added** | New features. |
| **Changed** | Behavior changes; trait surface tightenings; lexicon shape changes. |
| **Deprecated** | Features that still work but are scheduled for removal. |
| **Removed** | Features that are gone. |
| **Fixed** | Bug fixes for behavior introduced in earlier versions. |
| **Security** | Security-relevant fixes. |

The Changelog is in
[`CHANGELOG.md`](https://github.com/idiolect-dev/idiolect/blob/main/CHANGELOG.md).

## Compatibility matrix

| Component | Source of truth | Lock at |
| --- | --- | --- |
| `idiolect-records` | crates.io | exact version |
| `@idiolect-dev/schema` | npm | exact version |
| panproto crates | Git tag | `v0.70.1` for idiolect 0.11.1 |
| `idiolect` CLI | binary release on GitHub | release tag |
| `idiolect-orchestrator` container | `ghcr.io/idiolect-dev/orchestrator` | image SHA |
| `idiolect-observer` container | `ghcr.io/idiolect-dev/observer` | image SHA |

The container images are sigstore-signed. Verification policy is
in
[`docs/ci-cd.md`](https://github.com/idiolect-dev/idiolect/blob/main/docs/ci-cd.md).
