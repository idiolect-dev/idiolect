# CI/CD

Three GitHub Actions workflows and a small set of supporting files.
This doc describes what each workflow does, what it gates, and how
a downstream consumer verifies release artifacts.

## Workflows

### `ci.yml` — every PR and push to `main`

Seven jobs, parallel after the drift gate. A PR must turn every one
green before merge (enforced by branch protection).

| Job | Runs on | Purpose |
|---|---|---|
| `codegen` | ubuntu | Drift gate. Regenerates every `idiolect-codegen` output and fails if the working tree changes. |
| `rust` | ubuntu + macos-14 | `cargo fmt`, `clippy --workspace --all-targets -D warnings`, `nextest run --workspace`, doctests. |
| `feature-matrix` | ubuntu | Per-crate feature combinations that the default pass does not touch (cursor-sqlite, pds-atrium, daemon composites, etc.). |
| `cargo-deny` | ubuntu | Advisories, licences, bans, and source policy per `deny.toml`. |
| `check-compat` | ubuntu (PR only) | Diffs `lexicons/dev/` vs the merge base and fails on any breaking change. |
| `typescript` | ubuntu | biome lint, tsc typecheck, bun test. |

The workflow is also exposed via `workflow_call` so `release.yml`
can invoke it on the tagged commit.

### `release.yml` — tag push matching `v*`

The whole chain triggers when a maintainer pushes a tag. Publishes
eight classes of artifact:

| Artifact | Where | Signed |
|---|---|---|
| Binary archive per `(bin, target)` | GitHub Release | ✓ (aggregate `checksums.txt.sig`) |
| Container image (orchestrator, observer) | ghcr.io | ✓ (cosign keyless) |
| Multi-arch image manifest | ghcr.io (amd64 + arm64) | ✓ |
| SBOM per container (SPDX JSON) | GitHub Release + image attestation | ✓ (via docker-buildx `sbom: true`) |
| Source tarball | GitHub Release | ✓ |
| `checksums.txt` across every binary | GitHub Release | ✓ |
| `@idiolect-dev/schema` npm package | npm | ✓ (npm provenance) |
| Publishable crates | crates.io | — (no cosign for crates) |

The tag-triggered shape means a release is reproducible — cutting
the same tag at the same commit produces the same artifacts byte-
for-byte (modulo non-deterministic inputs like sigstore's Rekor
entry). A maintainer never runs `cargo publish` or `docker push` by
hand; doing so would bypass the signing and provenance the pipeline
attaches.

### `audit.yml` — daily 09:00 UTC

Three jobs:

- `cargo-audit` — RustSec advisory scan. Fails the run on an active
  advisory.
- `cargo-deny-advisories` — same, via the `deny.toml` policy.
- `npm-audit` — npm advisory scan over the JS workspace.

The workflow runs out-of-band and does not gate PRs. Failures
should be triaged via the issues it produces.

## Configuration files

| File | Purpose |
|---|---|
| `deny.toml` | Supply-chain policy consumed by `cargo-deny`. |
| `.github/dependabot.yml` | Weekly dep PRs for cargo and npm, monthly for actions and docker. |
| `.github/CODEOWNERS` | Review routing by path. |
| `.github/ISSUE_TEMPLATE/*.yml` | Structured bug + feature issue forms. |
| `.github/PULL_REQUEST_TEMPLATE.md` | Reviewer-oriented PR checklist. |
| `.cargo/config.toml` | Workspace build defaults (`jobs = 4`). |
| `rust-toolchain.toml` | Pinned Rust version for everyone, CI and local. |
| `RELEASE.md` | Step-by-step cut-a-release runbook. |
| `SECURITY.md` | Vulnerability-reporting process. |
| `CHANGELOG.md` | User-visible change log. Release-notes extracted automatically. |

## Verifying a release artifact

Every binary archive, container image, source tarball, and the
aggregate `checksums.txt` carries a sigstore keyless signature.
Verifying confirms three things: the artifact came from this
repository, it was built by the `release.yml` workflow, and it
belongs to a specific tag.

Prerequisites: [`cosign`](https://github.com/sigstore/cosign) and
the GitHub CLI (`gh`).

### Container image

```sh
cosign verify ghcr.io/idiolect-dev/orchestrator:0.1.0 \
  --certificate-identity-regexp 'https://github.com/idiolect-dev/idiolect/\.github/workflows/release\.yml@refs/tags/v0\.1\.0' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

The command exits 0 only if the image's signature was produced by
the tagged release workflow run for `v0.1.0`.

### Binary archive

Download and verify the aggregate checksums file, then check the
archive's hash against it:

```sh
gh release download v0.1.0 -p 'checksums.txt' -p 'checksums.txt.sig'

cosign verify-blob \
  --bundle checksums.txt.sig \
  --certificate-identity-regexp 'https://github.com/idiolect-dev/idiolect/' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  checksums.txt

# Pick the archive for your platform and verify its hash.
gh release download v0.1.0 -p 'idiolect-0.1.0-aarch64-apple-darwin.tar.gz'
grep 'idiolect-0.1.0-aarch64-apple-darwin.tar.gz' checksums.txt \
  | shasum -a 256 -c
```

### Source tarball

```sh
gh release download v0.1.0 -p 'idiolect-0.1.0-src.tar.gz*'

cosign verify-blob \
  --bundle idiolect-0.1.0-src.tar.gz.sig \
  --certificate-identity-regexp 'https://github.com/idiolect-dev/idiolect/' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  idiolect-0.1.0-src.tar.gz
```

### SBOM

Each container has an SPDX SBOM attached to the release. Inspect
with:

```sh
gh release download v0.1.0 -p 'sbom-orchestrator.spdx.json'
jq '.packages[].name' sbom-orchestrator.spdx.json | head
```

Container-level attestations are also retrievable:

```sh
cosign download attestation ghcr.io/idiolect-dev/orchestrator:0.1.0 \
  | jq '.payload | @base64d | fromjson'
```

## Adding a job

When a new check belongs in CI:

- Failing the check should block merge (i.e., it is correctness, not
  surveillance). Otherwise, put it in `audit.yml` or a scheduled
  workflow.
- It must run in under a reasonable fraction of the longest existing
  job (target: 15 minutes for fast jobs, 30 minutes for matrix jobs)
  or it dominates the PR feedback loop.
- It must use the pinned toolchain (`rust-toolchain.toml` for Rust,
  `bun-version: "1.2.2"` for TypeScript). Unpinned tool versions
  drift silently and break reproducibility.
- Cache via `Swatinem/rust-cache@v2` with a `shared-key` that
  reflects the job's unique compilation axis (target, feature set).
  Shared keys across jobs that compile different things pollute the
  cache and hurt everyone.

## Troubleshooting

### "Generated sources out of sync" in CI

Run `cargo run -p idiolect-codegen -- generate` locally and commit
the regenerated files. The gate enforces that the lexicons are the
single source of truth; any drift between a lexicon edit and the
regenerated Rust / TypeScript is a CI failure.

### "check-compat detected breaking changes"

The PR's lexicon edits are not backward-compatible with `main`. Two
paths forward:

1. Rework the change to avoid the breakage (add an optional field
   instead of a required one; rename via deprecation instead of
   outright).
2. If the breaking change is intentional, mark it explicitly per
   the project's stewardship process — a schema change that breaks
   existing records is a conscious decision, not a side effect.

### `cargo-deny` licence failure

A newly-added dep ships under a licence not in `deny.toml`'s
`allow` list. Review whether the licence is compatible with the
project's distribution terms; if yes, add it to the allow list in a
separate PR for review. If no, find a replacement dep.
