//! Tests for the filesystem-backed OAuth token store.

#![cfg(feature = "store-filesystem")]

use idiolect_oauth::{FilesystemOAuthTokenStore, OAuthSession, OAuthTokenStore};
use tempfile::tempdir;

fn session(did: &str) -> OAuthSession {
    OAuthSession::new(
        did,
        "https://pds.example",
        "access-jwt",
        "refresh-jwt",
        "dpop-jwk",
        "2026-04-19T00:00:00.000Z",
        "2026-04-19T01:00:00.000Z",
    )
}

#[tokio::test]
async fn save_and_load_roundtrip() {
    let dir = tempdir().unwrap();
    let store = FilesystemOAuthTokenStore::new(dir.path()).unwrap();
    let s = session("did:plc:alice");
    store.save(&s).await.unwrap();
    let loaded = store.load("did:plc:alice").await.unwrap().unwrap();
    assert_eq!(loaded, s);
}

#[tokio::test]
async fn load_missing_returns_none() {
    let dir = tempdir().unwrap();
    let store = FilesystemOAuthTokenStore::new(dir.path()).unwrap();
    assert!(store.load("did:plc:absent").await.unwrap().is_none());
}

#[tokio::test]
async fn delete_removes_file() {
    let dir = tempdir().unwrap();
    let store = FilesystemOAuthTokenStore::new(dir.path()).unwrap();
    store.save(&session("did:plc:x")).await.unwrap();
    assert!(store.load("did:plc:x").await.unwrap().is_some());
    store.delete("did:plc:x").await.unwrap();
    assert!(store.load("did:plc:x").await.unwrap().is_none());
    // deleting again is a no-op.
    store.delete("did:plc:x").await.unwrap();
}

#[tokio::test]
async fn save_overwrites_existing() {
    let dir = tempdir().unwrap();
    let store = FilesystemOAuthTokenStore::new(dir.path()).unwrap();
    store.save(&session("did:plc:a")).await.unwrap();
    let mut s2 = session("did:plc:a");
    s2.access_jwt = "rotated".to_owned();
    store.save(&s2).await.unwrap();
    let loaded = store.load("did:plc:a").await.unwrap().unwrap();
    assert_eq!(loaded.access_jwt, "rotated");
}

#[tokio::test]
async fn rejects_non_directory() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("not-a-dir");
    std::fs::write(&file, b"x").unwrap();
    let err = FilesystemOAuthTokenStore::new(file).unwrap_err();
    assert!(err.to_string().contains("not a directory"));
}

#[tokio::test]
async fn sanitizes_did_in_filename() {
    let dir = tempdir().unwrap();
    let store = FilesystemOAuthTokenStore::new(dir.path()).unwrap();
    store.save(&session("did:plc:abc/def")).await.unwrap();
    // the file exists under a sanitized name (no `:` or `/`).
    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert!(entries.iter().any(|n| !n.contains(':') && !n.contains('/')));
    let loaded = store.load("did:plc:abc/def").await.unwrap().unwrap();
    assert_eq!(loaded.did, "did:plc:abc/def");
}

#[tokio::test]
async fn list_dids_reads_every_file_in_directory() {
    let dir = tempdir().unwrap();
    let store = FilesystemOAuthTokenStore::new(dir.path()).unwrap();
    assert!(store.list_dids().await.unwrap().is_empty());

    store.save(&session("did:plc:alpha")).await.unwrap();
    store.save(&session("did:plc:beta")).await.unwrap();
    store.save(&session("did:plc:gamma")).await.unwrap();

    let dids = store.list_dids().await.unwrap();
    assert_eq!(
        dids,
        vec![
            "did:plc:alpha".to_owned(),
            "did:plc:beta".to_owned(),
            "did:plc:gamma".to_owned(),
        ]
    );
}

#[tokio::test]
async fn list_dids_ignores_unrelated_files_and_bad_json() {
    let dir = tempdir().unwrap();
    let store = FilesystemOAuthTokenStore::new(dir.path()).unwrap();
    store.save(&session("did:plc:alice")).await.unwrap();
    // A non-session json file in the dir must not break enumeration.
    std::fs::write(dir.path().join("orphan.json"), b"{not session}").unwrap();
    // A non-json file must also be skipped.
    std::fs::write(dir.path().join("note.txt"), b"hi").unwrap();
    let dids = store.list_dids().await.unwrap();
    assert_eq!(dids, vec!["did:plc:alice".to_owned()]);
}

#[cfg(unix)]
#[tokio::test]
async fn file_mode_is_0600_on_unix() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempdir().unwrap();
    let store = FilesystemOAuthTokenStore::new(dir.path()).unwrap();
    store.save(&session("did:plc:alice")).await.unwrap();
    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap())
        .collect();
    assert_eq!(entries.len(), 1);
    let meta = entries[0].metadata().unwrap();
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
}
