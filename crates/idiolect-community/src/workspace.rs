//! Workspace initialization, persistence, validation, and diagnostics.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{CommunityError, CommunityResult};
use crate::model::{
    Authority, ChangePacket, CommunityIdentity, CommunityRelease, Diagnostic, DiagnosticSeverity,
    DoctorReport, MANIFEST_VERSION, PackageSpec, WorkspaceManifest,
};

/// Conventional community manifest filename.
pub const DEFAULT_MANIFEST_NAME: &str = "idiolect.toml";

/// Create a new community workspace with safe, inspectable defaults.
///
/// The function refuses to overwrite an existing manifest. It creates the
/// `.idiolect` artifact directories eagerly so later commands do not need to
/// guess where durable state belongs.
///
/// # Errors
///
/// Returns an error when a manifest already exists or workspace directories
/// and the initial manifest cannot be written.
pub fn init_workspace(
    root: &Path,
    name: impl Into<String>,
    did: impl Into<String>,
) -> CommunityResult<WorkspaceManifest> {
    let manifest_path = root.join(DEFAULT_MANIFEST_NAME);
    if manifest_path.exists() {
        return Err(CommunityError::Invalid(format!(
            "{} already exists",
            manifest_path.display()
        )));
    }
    let did = did.into();
    let manifest = WorkspaceManifest {
        manifest_version: MANIFEST_VERSION,
        community: CommunityIdentity {
            name: name.into(),
            did: did.clone(),
            description: "Describe who this community serves and what it governs.".to_owned(),
            record: None,
        },
        packages: vec![PackageSpec {
            name: "lexicons".to_owned(),
            path: "lexicons".to_owned(),
            protocol: "atproto".to_owned(),
            cross_document: true,
        }],
        authorities: vec![Authority {
            did,
            roles: vec!["maintainer".to_owned(), "steward".to_owned()],
        }],
        governance: crate::model::GovernancePolicy::default(),
        resources: crate::model::ResourcePolicy::default(),
        federation: Vec::new(),
        release: crate::model::ReleasePolicy::default(),
        exit: crate::model::ExitPolicy::default(),
    };
    fs::create_dir_all(root.join("lexicons"))
        .map_err(|e| CommunityError::io(root.join("lexicons"), e))?;
    for dir in ["changes", "migrations", "releases", "keys"] {
        let path = root.join(".idiolect").join(dir);
        fs::create_dir_all(&path).map_err(|e| CommunityError::io(path, e))?;
    }
    save_manifest(&manifest_path, &manifest)?;
    Ok(manifest)
}

/// Load and semantically validate a workspace manifest.
///
/// # Errors
///
/// Returns an error when the file cannot be read, parsed, or passes syntax but
/// fails semantic validation.
pub fn load_manifest(path: &Path) -> CommunityResult<WorkspaceManifest> {
    let text = fs::read_to_string(path).map_err(|e| CommunityError::io(path, e))?;
    let manifest: WorkspaceManifest = toml::from_str(&text)?;
    let diagnostics = validate_manifest(&manifest);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == DiagnosticSeverity::Error)
        .map(|d| d.message.as_str())
        .collect();
    if errors.is_empty() {
        Ok(manifest)
    } else {
        Err(CommunityError::Invalid(errors.join("; ")))
    }
}

/// Serialize a workspace manifest as stable, pretty TOML.
///
/// # Errors
///
/// Returns an error when serialization or the atomic file write fails.
pub fn save_manifest(path: &Path, manifest: &WorkspaceManifest) -> CommunityResult<()> {
    let text = toml::to_string_pretty(manifest)?;
    write_atomic(path, text.as_bytes())
}

/// Load a change packet JSON file.
///
/// # Errors
///
/// Returns an error when the file cannot be read or decoded.
pub fn load_packet(path: &Path) -> CommunityResult<ChangePacket> {
    load_json(path)
}

/// Save a change packet JSON file.
///
/// # Errors
///
/// Returns an error when serialization or the atomic file write fails.
pub fn save_packet(path: &Path, packet: &ChangePacket) -> CommunityResult<()> {
    save_json(path, packet)
}

/// Load a community release JSON file.
///
/// # Errors
///
/// Returns an error when the file cannot be read or decoded.
pub fn load_release(path: &Path) -> CommunityResult<CommunityRelease> {
    load_json(path)
}

/// Save any serializable artifact as stable, pretty JSON.
///
/// # Errors
///
/// Returns an error when serialization or the atomic file write fails.
pub fn save_json<T: Serialize>(path: &Path, value: &T) -> CommunityResult<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    write_atomic(path, &bytes)
}

pub(crate) fn load_json<T: DeserializeOwned>(path: &Path) -> CommunityResult<T> {
    let bytes = fs::read(path).map_err(|e| CommunityError::io(path, e))?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> CommunityResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| CommunityError::io(parent, e))?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or("file")
    ));
    fs::write(&tmp, bytes).map_err(|e| CommunityError::io(&tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| CommunityError::io(path, e))?;
    Ok(())
}

/// Validate only the manifest and referenced package layout.
///
/// # Errors
///
/// Returns an error when the workspace cannot be inspected.
pub fn check_workspace(root: &Path) -> CommunityResult<DoctorReport> {
    doctor_workspace(root, false)
}

/// Inspect a workspace and return actionable diagnostics plus artifact counts.
///
/// # Errors
///
/// Returns an error for I/O failures other than an absent manifest, or when an
/// optional artifact inventory cannot be constructed.
pub fn doctor_workspace(root: &Path, include_inventory: bool) -> CommunityResult<DoctorReport> {
    let path = root.join(DEFAULT_MANIFEST_NAME);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(DoctorReport {
                healthy: false,
                diagnostics: vec![Diagnostic {
                    severity: DiagnosticSeverity::Error,
                    code: "manifest-missing".to_owned(),
                    message: format!("{} is missing", path.display()),
                    help: Some("Run `idiolect init --name NAME --did DID`.".to_owned()),
                }],
                inventory: BTreeMap::new(),
            });
        }
        Err(error) => return Err(CommunityError::io(path, error)),
    };
    let manifest: WorkspaceManifest = match toml::from_str(&text) {
        Ok(manifest) => manifest,
        Err(error) => {
            return Ok(DoctorReport {
                healthy: false,
                diagnostics: vec![Diagnostic {
                    severity: DiagnosticSeverity::Error,
                    code: "manifest-invalid".to_owned(),
                    message: error.to_string(),
                    help: Some("Fix the TOML syntax, then run `idiolect check`.".to_owned()),
                }],
                inventory: BTreeMap::new(),
            });
        }
    };

    let mut diagnostics = validate_manifest(&manifest);
    for package in &manifest.packages {
        let package_path = root.join(&package.path);
        if !package_path.exists() {
            diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Error,
                code: "package-missing".to_owned(),
                message: format!(
                    "package `{}` points to missing path {}",
                    package.name,
                    package_path.display()
                ),
                help: Some("Create the directory or update `packages[].path`.".to_owned()),
            });
        }
    }
    for expected in ["changes", "migrations", "releases"] {
        let path = root.join(".idiolect").join(expected);
        if !path.is_dir() {
            diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Warning,
                code: "artifact-directory-missing".to_owned(),
                message: format!("{} has not been created", path.display()),
                help: Some("Run `idiolect doctor --repair` to create it.".to_owned()),
            });
        }
    }
    if manifest.federation.iter().any(|d| d.mappings.is_empty()) {
        diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Info,
            code: "federation-unmapped".to_owned(),
            message: "one or more federation relationships have no published mapping".to_owned(),
            help: Some(
                "Add a lens or mapping URI before claiming semantic interoperability.".to_owned(),
            ),
        });
    }

    let inventory = if include_inventory {
        artifact_inventory(root)?
    } else {
        BTreeMap::new()
    };
    let healthy = !diagnostics
        .iter()
        .any(|d| d.severity == DiagnosticSeverity::Error);
    Ok(DoctorReport {
        healthy,
        diagnostics,
        inventory,
    })
}

fn validate_manifest(manifest: &WorkspaceManifest) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    if manifest.manifest_version != MANIFEST_VERSION {
        out.push(error(
            "manifest-version",
            format!(
                "manifest version {} is unsupported; expected {MANIFEST_VERSION}",
                manifest.manifest_version
            ),
        ));
    }
    if manifest.community.name.trim().is_empty() {
        out.push(error("community-name", "community.name cannot be empty"));
    }
    if !manifest.community.did.starts_with("did:") {
        out.push(error(
            "community-did",
            "community.did must be a decentralized identifier beginning with `did:`",
        ));
    }
    if manifest.packages.is_empty() {
        out.push(error(
            "packages-empty",
            "at least one governed package is required",
        ));
    }
    let mut names = BTreeSet::new();
    for package in &manifest.packages {
        if !names.insert(package.name.as_str()) {
            out.push(error(
                "package-duplicate",
                format!("package name `{}` occurs more than once", package.name),
            ));
        }
        if package.protocol.trim().is_empty() || package.path.trim().is_empty() {
            out.push(error(
                "package-incomplete",
                format!("package `{}` needs both path and protocol", package.name),
            ));
        }
    }
    if manifest.authorities.is_empty() {
        out.push(error(
            "authorities-empty",
            "at least one authority is required to approve and sign releases",
        ));
    }
    if manifest.governance.min_approvals == 0 || manifest.governance.quorum == 0 {
        out.push(error(
            "governance-zero-threshold",
            "governance minApprovals and quorum must be positive",
        ));
    }
    if !(0.0..=1.0).contains(&manifest.governance.approval_threshold) {
        out.push(error(
            "governance-ratio",
            "governance approvalThreshold must be between 0 and 1",
        ));
    }
    if manifest.release.min_signatures == 0 {
        out.push(error(
            "release-unsigned",
            "release minSignatures must be positive",
        ));
    }
    if manifest.resources.max_documents == 0
        || manifest.resources.max_schema_bytes == 0
        || manifest.resources.max_search_steps == 0
        || manifest.resources.max_verification_cases == 0
    {
        out.push(error(
            "resource-limit-zero",
            "resource limits must be positive",
        ));
    }
    out
}

fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        severity: DiagnosticSeverity::Error,
        code: code.to_owned(),
        message: message.into(),
        help: None,
    }
}

fn artifact_inventory(root: &Path) -> CommunityResult<BTreeMap<String, u64>> {
    let mut out = BTreeMap::new();
    for kind in ["changes", "migrations", "releases"] {
        let path = root.join(".idiolect").join(kind);
        let count = match fs::read_dir(&path) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
                .count() as u64,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(CommunityError::io(path, error)),
        };
        out.insert(kind.to_owned(), count);
    }
    Ok(out)
}

/// Create missing artifact directories without changing the manifest.
///
/// # Errors
///
/// Returns an error when a missing directory cannot be created.
pub fn repair_workspace(root: &Path) -> CommunityResult<Vec<PathBuf>> {
    let mut created = Vec::new();
    for dir in ["changes", "migrations", "releases", "keys"] {
        let path = root.join(".idiolect").join(dir);
        if !path.exists() {
            fs::create_dir_all(&path).map_err(|e| CommunityError::io(&path, e))?;
            created.push(path);
        }
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_round_trips_manifest_and_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = init_workspace(tmp.path(), "Mutual Aid", "did:plc:community").unwrap();
        assert_eq!(manifest.manifest_version, MANIFEST_VERSION);
        assert!(tmp.path().join(".idiolect/changes").is_dir());
        assert_eq!(
            load_manifest(&tmp.path().join(DEFAULT_MANIFEST_NAME)).unwrap(),
            manifest
        );
    }

    #[test]
    fn doctor_explains_missing_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let report = doctor_workspace(tmp.path(), true).unwrap();
        assert!(!report.healthy);
        assert_eq!(report.diagnostics[0].code, "manifest-missing");
    }
}
