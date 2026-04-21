# Security policy

## Reporting a vulnerability

Do not open a public issue for security vulnerabilities. Use GitHub
Security Advisories:

- https://github.com/idiolect-dev/idiolect/security/advisories/new

A maintainer will acknowledge receipt within 72 hours and triage
within seven days. If the issue is an exploitable bug in a shipped
artifact (crate, container image, published record schema), the
project follows an embargoed-disclosure track: private notification
to conformant-orchestrator implementers, up to 30-day embargo,
accelerated fix, public disclosure on a shared date.

If the issue is in a dependency rather than idiolect itself, please
also notify the upstream maintainers directly.

## Supported versions

Security fixes land on the most recent released minor version. In
the pre-1.0 phase, this is the latest tag only; there is no
long-term-support commitment yet.

## Verifying release artifacts

Every release's binaries, container images, source tarball, and
aggregate checksum file is signed with sigstore keyless via the
GitHub Actions OIDC token. Anyone can verify an artifact came from
this repository's release pipeline without trusting a private key.

Concrete commands: see `RELEASE.md` section "Verify the release
artifacts."

## Scope

The project treats the following as security-relevant, worth the
embargoed-disclosure track:

- Exploitable behavior in a shipped binary or container.
- A lexicon shape that enables attack (e.g. a record that exhausts
  a conformant orchestrator's resources at ingest).
- A trait contract whose documented semantics admit an unsafe
  implementation consumers assume is safe.

The following are **not** security issues; use the normal bug
report template:

- A downstream lens record someone published that produces wrong
  translations. The architecture relies on observers to surface
  this; the project does not adjudicate ecology content.
- A deployment's misconfiguration (exposed admin endpoints, missing
  auth on a custom orchestrator extension).
- A dispute between communities about a verification's validity.
