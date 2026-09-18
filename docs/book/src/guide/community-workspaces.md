# Operate a community workspace

A community workspace is a directory rooted at `idiolect.toml`. It is suitable for a personal checkout, a shared repository, or a CI job mounted over community-owned storage.

## Directory contract

`idiolect init` creates this layout:

```text
idiolect.toml
lexicons/
.idiolect/
  changes/
  migrations/
  releases/
  keys/
```

Package paths and the release directory are configurable. Artifact directories are implementation state, though their JSON contents are stable enough to review and archive. Private keys should remain outside Git and participant-facing exports.

## Health checks

Use `idiolect check` for a concise validation gate and `idiolect doctor` for actionable diagnostics and an artifact inventory:

```console
idiolect check --workspace .
idiolect doctor --workspace . --json
```

Both commands validate the manifest version, community identity, authority roles, governance thresholds, resource bounds, unique package names, relative package paths, release policy, and referenced package directories. `doctor` also reports absent managed directories and federation relationships with no mappings.

`doctor --repair` creates missing `.idiolect/{changes,migrations,releases,keys}` directories. It deliberately does not repair definitions or policy because doing so would require community judgment.

## Multiple packages

Add a `[[packages]]` entry for each governed definition set:

```toml
[[packages]]
name = "profiles"
path = "packages/profiles"
protocol = "atproto"
crossDocument = true

[[packages]]
name = "public-api"
path = "packages/openapi"
protocol = "openapi"
crossDocument = false
```

`protocol` must be a canonical Panproto registry key. `crossDocument` records whether documents in the package are intended to resolve references together; commands that operate on one source and one target still parse each bounded document independently.

## Resource policy

Community inputs may arrive from public repositories or peers. The workspace thus records shared limits for document count, total schema bytes, migration search steps, and verification cases. Pass the same policy to browser, CLI, and service operations when possible; otherwise a proposal might succeed for one participant and exhaust another participant's tooling.

## Backups

Back up the manifest, packages, changes, migrations, and releases. Back up keys under a separate access policy. Run `idiolect export` periodically and verify that a fresh checkout can pass `idiolect doctor --workspace <export>`.

For every field, see the [manifest reference](../reference/community-manifest.md).
