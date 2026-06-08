# Cutting a release

The release pipeline is tag-triggered: pushing a tag `v<semver>`
runs `.github/workflows/release.yml`, which (a) re-runs the full CI
suite at the tagged commit, (b) builds binaries, containers, and a
source tarball, (c) signs everything via sigstore, and (d)
publishes the GitHub Release plus the npm and crates.io packages.

This document walks the maintainer through the manual steps that
precede the tag.

## Preconditions

- The branch you are releasing from is `main`, green on CI, and
  you have pushed the commit you intend to tag.
- All PRs intended for the release are merged. CHANGELOG's
  `[Unreleased]` section lists their user-visible effects.
- `docs/deployment.md` reflects any new operator-visible knobs.

## 1. Bump versions

A release is atomic across the Rust workspace and the npm package.
Bump both to the new semver. Three places carry a version, not two:
the workspace `version`, the npm `package.json`, and the explicit
`version = "..."` requirement each crate pins on its sibling
`idiolect-*` path dependencies. The path-dep requirements must track
the workspace version or `cargo build` fails with a "no matching
version" error before CI ever sees the tag.

```sh
# Pick one and use it consistently.
new=0.1.0

# Rust workspace.
sed -i.bak "s/^version = \".*\"$/version = \"$new\"/" Cargo.toml
rm Cargo.toml.bak

# Inter-crate path-dep requirements (idiolect-* deps only; this leaves
# external deps like `sha2 = "0.10"` untouched because the substitution
# is scoped to lines that point at a sibling crate via `path`).
sed -i.bak -E "/path = \"\.\.\/idiolect-/ s/version = \"[^\"]*\"/version = \"$new\"/" crates/*/Cargo.toml
rm crates/*/Cargo.toml.bak

# @idiolect-dev/schema.
jq --arg v "$new" '.version = $v' packages/schema/package.json \
  > packages/schema/package.json.tmp
mv packages/schema/package.json.tmp packages/schema/package.json

# Refresh the lockfile so the workspace crate versions in Cargo.lock
# match the bumped manifests.
cargo build --workspace
```

Commit the bump on its own:

```sh
git add Cargo.toml Cargo.lock packages/schema/package.json crates/*/Cargo.toml
git commit -m "release: v$new"
```

## 2. Finalize the changelog

Rename `[Unreleased]` in `CHANGELOG.md` to `[$new] - <date>` and
open a fresh empty `[Unreleased]` above it.

```md
## [Unreleased]

### Added
### Changed
### Deprecated
### Removed
### Fixed
### Security

## [0.1.0] - 2026-04-22

### Added
- … (the items moved from the previous Unreleased)
```

Commit:

```sh
git add CHANGELOG.md
git commit -m "changelog: v$new"
```

## 3. Push and tag

```sh
git push origin main
git tag "v$new" -m "v$new"
git push origin "v$new"
```

The tag push triggers `.github/workflows/release.yml`.

## 4. Monitor the release run

Watch the Actions tab. The pipeline has several independent jobs
that fan out; a failure in any one fails the release:

- `verify-version` — asserts the tag matches both manifests.
- `ci-on-tag` — full CI at the tagged commit.
- `build-binaries` — 8-row matrix of `(binary, target)` archives.
- `build-containers` — multi-arch ghcr.io pushes + cosign sign +
  SBOM.
- `source-tarball` — source archive + signed checksum.
- `publish-release` — aggregates and publishes the GitHub Release.
- `publish-npm` — `@idiolect-dev/schema` to the npm registry.
- `publish-crates` — `idiolect-records`, `-oauth`, `-indexer`,
  `-lens` to crates.io in topological order.

## 5. Verify the release artifacts

After the workflow is green, sanity-check the release:

```sh
# Containers pulled and signature verified.
cosign verify ghcr.io/idiolect-dev/orchestrator:$new \
  --certificate-identity-regexp 'https://github.com/idiolect-dev/idiolect/\.github/workflows/release\.yml@refs/tags/v'"$new" \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com

# Source-tarball checksum verified.
gh release download "v$new" -p 'idiolect-*-src.tar.gz*'
cosign verify-blob \
  --bundle idiolect-$new-src.tar.gz.sig \
  --certificate-identity-regexp 'https://github.com/idiolect-dev/idiolect/' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  idiolect-$new-src.tar.gz

# Binary archive checksums match the signed checksums.txt.
gh release download "v$new" -p 'checksums.txt' -p 'checksums.txt.sig'
cosign verify-blob \
  --bundle checksums.txt.sig \
  --certificate-identity-regexp 'https://github.com/idiolect-dev/idiolect/' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  checksums.txt
```

## 6. Announce

Announcements happen on:

- GitHub Release page (auto-posted, but confirm the body)
- The repository's README-linked announcement channel (when one exists)

## Rolling back

Never delete the tag or release artifacts after consumers have
pulled them. If a release ships with a regression, publish a patch
release from a hotfix branch.

## Secrets required in the repo settings

- `CARGO_REGISTRY_TOKEN` — crates.io publish token. Optional; the
  `publish-crates` job skips cleanly when absent.
- `NPM_TOKEN` — npm publish token. Optional; `publish-npm` skips
  when absent. Using an npm automation token scoped to `@idiolect-dev`
  is recommended.

Sigstore keyless signing via the GitHub Actions OIDC token requires
no secrets — the workflow's `id-token: write` permission mints the
certificate at sign time.
