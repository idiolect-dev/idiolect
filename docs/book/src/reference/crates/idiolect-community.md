# `idiolect-community`

`idiolect-community` implements the file-backed community lifecycle introduced in 0.13.0. It is independent of network publication and can be embedded in a CLI, service, desktop application, or test application.

```toml
[dependencies]
idiolect-community = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.13.0" }
```

## Modules

| Module | Responsibility |
|---|---|
| `model` | Manifest, packet, decision, verification, migration, release, diagnostic, and signing data. |
| `workspace` | Initialization, atomic persistence, validation, diagnostics, repair, and inventory. |
| `analyze` | Resource-bounded Panproto parsing, diffing, compatibility, protolens, optic classification, and consequence messages. |
| `governance` | Role-aware reviews, five decision models, review periods, and three-state verification gate. |
| `migration` | Durable run creation and monotonic checkpoint advancement. |
| `release` | Artifact hashing, canonical bundle digest, P-256 keys, ES256 signatures, and tamper verification. |
| `export` | Symlink-safe, non-overwriting portable workspace exports with SHA-256 inventory. |

## Principal types

- `WorkspaceManifest`
- `ChangePacket` and `ConsequenceReport`
- `GovernanceDecision`, `Review`, and `VerificationEvidence`
- `MigrationRun`
- `CommunityRelease` and `CommunitySigningKey`
- `DoctorReport` and `ExportInventory`

## Safety properties

File writes use a sibling temporary file followed by rename. Export refuses symbolic links and non-empty destinations. Schema analysis receives an explicit Panproto budget derived from `ResourcePolicy`. Private signing keys are serializable because callers need durable storage; the CLI restricts generated files to `0600` on Unix, and embedders must provide an equivalent policy.

## Minimal embedding

```rust
use std::path::Path;
use idiolect_community::{doctor_workspace, init_workspace};

fn main() -> Result<(), idiolect_community::CommunityError> {
    let root = Path::new("my-community");
    let _manifest = init_workspace(root, "My Community", "did:plc:community")?;
    let report = doctor_workspace(root, true)?;
    assert!(report.healthy);
    Ok(())
}
```

See [Operate a community workspace](../../guide/community-workspaces.md) for the complete lifecycle.
