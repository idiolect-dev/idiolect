//! Tests for the sqlite-backed OAuth token store.

#![cfg(feature = "store-sqlite")]

use idiolect_oauth::{OAuthSession, OAuthTokenStore, SqliteOAuthTokenStore};
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
    let store = SqliteOAuthTokenStore::open(dir.path().join("s.db")).unwrap();
    let s = session("did:plc:alice");
    store.save(&s).await.unwrap();
    assert_eq!(store.load("did:plc:alice").await.unwrap().unwrap(), s);
}

#[tokio::test]
async fn load_missing_returns_none() {
    let dir = tempdir().unwrap();
    let store = SqliteOAuthTokenStore::open(dir.path().join("s.db")).unwrap();
    assert!(store.load("did:plc:absent").await.unwrap().is_none());
}

#[tokio::test]
async fn save_overwrites_on_conflict() {
    let dir = tempdir().unwrap();
    let store = SqliteOAuthTokenStore::open(dir.path().join("s.db")).unwrap();
    store.save(&session("did:plc:alice")).await.unwrap();
    let mut s2 = session("did:plc:alice");
    s2.access_jwt = "rotated".into();
    store.save(&s2).await.unwrap();
    assert_eq!(
        store.load("did:plc:alice").await.unwrap().unwrap().access_jwt,
        "rotated"
    );
}

#[tokio::test]
async fn delete_removes_row() {
    let dir = tempdir().unwrap();
    let store = SqliteOAuthTokenStore::open(dir.path().join("s.db")).unwrap();
    store.save(&session("did:plc:alice")).await.unwrap();
    store.delete("did:plc:alice").await.unwrap();
    assert!(store.load("did:plc:alice").await.unwrap().is_none());
    // Double-delete is a no-op.
    store.delete("did:plc:alice").await.unwrap();
}

#[tokio::test]
async fn list_dids_returns_every_saved_session_sorted() {
    let dir = tempdir().unwrap();
    let store = SqliteOAuthTokenStore::open(dir.path().join("s.db")).unwrap();
    store.save(&session("did:plc:zeta")).await.unwrap();
    store.save(&session("did:plc:alpha")).await.unwrap();
    store.save(&session("did:plc:mu")).await.unwrap();
    assert_eq!(
        store.list_dids().await.unwrap(),
        vec![
            "did:plc:alpha".to_owned(),
            "did:plc:mu".to_owned(),
            "did:plc:zeta".to_owned(),
        ],
    );
}

#[tokio::test]
async fn reopen_sees_committed_rows() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("s.db");
    {
        let store = SqliteOAuthTokenStore::open(&path).unwrap();
        store.save(&session("did:plc:a")).await.unwrap();
        store.save(&session("did:plc:b")).await.unwrap();
    }
    let store = SqliteOAuthTokenStore::open(&path).unwrap();
    assert_eq!(store.list_dids().await.unwrap().len(), 2);
}
