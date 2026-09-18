//! Portable, host-independent community exports.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{CommunityError, CommunityResult};
use crate::governance::now;
use crate::model::{CommunityRelease, WorkspaceManifest};
use crate::workspace::{DEFAULT_MANIFEST_NAME, load_manifest, save_json};

/// Machine-readable inventory written into every portable export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportInventory {
    /// Export format version.
    pub format_version: u32,
    /// Community DID.
    pub community: String,
    /// RFC-3339 export time.
    pub exported_at: String,
    /// Relative file path to SHA-256 digest.
    pub files: BTreeMap<String, String>,
}

/// Export the workspace into a new portable directory.
///
/// Symlinks are rejected so a package cannot make the export silently read
/// data outside the workspace. Existing non-empty destinations are also
/// rejected; this operation never replaces an earlier export.
///
/// # Errors
///
/// Returns an error when the manifest is invalid, a source is a symbolic link,
/// the destination is non-empty, or a file cannot be copied or inventoried.
pub fn export_workspace(
    root: &Path,
    destination: &Path,
    release: Option<&CommunityRelease>,
) -> CommunityResult<ExportInventory> {
    let manifest = load_manifest(&root.join(DEFAULT_MANIFEST_NAME))?;
    ensure_destination(destination)?;
    fs::create_dir_all(destination).map_err(|e| CommunityError::io(destination, e))?;

    copy_file(
        &root.join(DEFAULT_MANIFEST_NAME),
        &destination.join(DEFAULT_MANIFEST_NAME),
    )?;
    if manifest.exit.include_packages {
        for package in &manifest.packages {
            copy_tree(
                &root.join(&package.path),
                &destination.join("packages").join(&package.name),
            )?;
        }
    }
    if manifest.exit.include_history {
        copy_optional_tree(
            &root.join(".idiolect/changes"),
            &destination.join("history/changes"),
        )?;
    }
    if manifest.exit.include_migrations {
        copy_optional_tree(
            &root.join(".idiolect/migrations"),
            &destination.join("history/migrations"),
        )?;
    }
    if manifest.exit.include_releases {
        copy_optional_tree(
            &root.join(".idiolect/releases"),
            &destination.join("releases"),
        )?;
    }
    if let Some(release) = release {
        save_json(&destination.join("community-release.json"), release)?;
    }
    write_readme(destination, &manifest)?;

    let mut inventory = ExportInventory {
        format_version: 1,
        community: manifest.community.did.clone(),
        exported_at: now()?,
        files: BTreeMap::new(),
    };
    collect_inventory(destination, destination, &mut inventory.files)?;
    save_json(&destination.join("export-inventory.json"), &inventory)?;
    Ok(inventory)
}

fn ensure_destination(destination: &Path) -> CommunityResult<()> {
    if destination.exists() {
        let mut entries =
            fs::read_dir(destination).map_err(|e| CommunityError::io(destination, e))?;
        if entries.next().is_some() {
            return Err(CommunityError::Invalid(format!(
                "export destination {} is not empty",
                destination.display()
            )));
        }
    }
    Ok(())
}

fn copy_optional_tree(source: &Path, destination: &Path) -> CommunityResult<()> {
    if source.exists() {
        copy_tree(source, destination)?;
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> CommunityResult<()> {
    let metadata = fs::symlink_metadata(source).map_err(|e| CommunityError::io(source, e))?;
    if metadata.file_type().is_symlink() {
        return Err(CommunityError::Invalid(format!(
            "portable exports do not follow symlink {}",
            source.display()
        )));
    }
    if metadata.is_file() {
        return copy_file(source, destination);
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(destination).map_err(|e| CommunityError::io(destination, e))?;
    let mut entries: Vec<_> = fs::read_dir(source)
        .map_err(|e| CommunityError::io(source, e))?
        .collect::<Result<_, _>>()
        .map_err(|e| CommunityError::io(source, e))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
    }
    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> CommunityResult<()> {
    let metadata = fs::symlink_metadata(source).map_err(|e| CommunityError::io(source, e))?;
    if metadata.file_type().is_symlink() {
        return Err(CommunityError::Invalid(format!(
            "portable exports do not follow symlink {}",
            source.display()
        )));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| CommunityError::io(parent, e))?;
    }
    fs::copy(source, destination).map_err(|e| CommunityError::io(destination, e))?;
    Ok(())
}

fn collect_inventory(
    root: &Path,
    directory: &Path,
    out: &mut BTreeMap<String, String>,
) -> CommunityResult<()> {
    let mut entries: Vec<_> = fs::read_dir(directory)
        .map_err(|e| CommunityError::io(directory, e))?
        .collect::<Result<_, _>>()
        .map_err(|e| CommunityError::io(directory, e))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|e| CommunityError::Invalid(e.to_string()))?;
        if relative == Path::new("export-inventory.json") {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).map_err(|e| CommunityError::io(&path, e))?;
        if metadata.is_dir() {
            collect_inventory(root, &path, out)?;
        } else if metadata.is_file() {
            let bytes = fs::read(&path).map_err(|e| CommunityError::io(&path, e))?;
            out.insert(
                portable_path(relative),
                format!("sha256:{:x}", Sha256::digest(bytes)),
            );
        }
    }
    Ok(())
}

fn portable_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn write_readme(destination: &Path, manifest: &WorkspaceManifest) -> CommunityResult<()> {
    let path = destination.join("README.md");
    let text = format!(
        "# {} community export\n\n\
         This directory is a portable Idiolect community workspace. It includes\n\
         the definitions, decision history, migration evidence, and signed releases\n\
         selected by the community's exit policy.\n\n\
         Community DID: `{}`\n\n\
         Start with `idiolect doctor --workspace .`, then inspect\n\
         `export-inventory.json` before importing or forking the workspace.\n",
        manifest.community.name, manifest.community.did
    );
    fs::write(&path, text).map_err(|e| CommunityError::io(path, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::init_workspace;

    #[test]
    fn export_contains_manifest_packages_and_inventory() {
        let tmp = tempfile::tempdir().unwrap();
        init_workspace(tmp.path(), "Community", "did:plc:community").unwrap();
        fs::write(tmp.path().join("lexicons/note.json"), b"{}").unwrap();
        let destination = tmp.path().join("portable");
        let inventory = export_workspace(tmp.path(), &destination, None).unwrap();
        assert!(destination.join("idiolect.toml").is_file());
        assert!(destination.join("packages/lexicons/note.json").is_file());
        assert!(inventory.files.contains_key("idiolect.toml"));
    }

    #[test]
    fn export_refuses_nonempty_destination() {
        let tmp = tempfile::tempdir().unwrap();
        init_workspace(tmp.path(), "Community", "did:plc:community").unwrap();
        let destination = tmp.path().join("portable");
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("keep.txt"), b"keep").unwrap();
        assert!(export_workspace(tmp.path(), &destination, None).is_err());
    }
}
