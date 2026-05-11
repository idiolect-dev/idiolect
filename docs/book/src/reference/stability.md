# Stability and versioning

idiolect is pre-1.0. Releases in the `0.x` series may include
arbitrary breaking changes between minor versions: Rust APIs,
lexicon shapes, wire formats, daemon HTTP routes, and CLI surfaces
are all in scope.

Pin to an exact version if you depend on this project. Read the
[changelog](https://github.com/idiolect-dev/idiolect/blob/main/CHANGELOG.md)
before bumping.

## What changes between minor versions

Pre-1.0:

- **Trait signatures** can tighten or widen between minor
  versions. The most recent example is the `Resolver` /
  `SchemaLoader` Send bound in v0.8.0.
- **Lexicon shapes** can change. Wire-compatible changes go through
  the [lexicon-evolution policy](../concepts/lexicon-evolution.md);
  breaking changes ship with a derived migration lens.
- **CLI subcommands** can rename or reshape. The output JSON shape
  is more stable than the flag surface.
- **HTTP routes** can change under the `v1` prefix between minor
  versions. After 1.0 they will not.

## What does not change

- The `dev.idiolect.*` namespace stays as is. NSID renames are
  possible but extraordinarily unusual; one would ship with a
  deprecation note in `dev.idiolect.dialect#deprecations`.
- Records that pass validation continue to pass validation. A
  record valid against v0.7's lexicon is also valid against v0.8's
  (the new fields are optional).
- The architectural commitments listed in the README do not
  change between minor versions: records are signed and
  content-addressed, lenses obey their stated laws, the lexicons
  are the single source of truth, the codegen drift gate is on.

## What changes at 1.0

- Breaking changes between minor versions stop. Breaking changes
  ship in major versions only.
- The lexicon-evolution `check-compat` gate flips from advisory to
  a hard fail.
- The HTTP API's `v1` prefix becomes a stability commitment; new
  endpoints are additive.
- Trait signatures in `idiolect-records`, `idiolect-lens`,
  `idiolect-indexer`, and `idiolect-orchestrator` become
  semver-stable.

The 1.0 release date is not committed. The pre-1.0 series
deliberately churns to find the right shape; 1.0 ships when the
shape stops moving.

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
| `idiolect` CLI | binary release on GitHub | release tag |
| `idiolect-orchestrator` container | `ghcr.io/idiolect-dev/orchestrator` | image SHA |
| `idiolect-observer` container | `ghcr.io/idiolect-dev/observer` | image SHA |

The container images are sigstore-signed; verification policy is
in
[`docs/ci-cd.md`](https://github.com/idiolect-dev/idiolect/blob/main/docs/ci-cd.md).
