//! Exercises the [`OAuthTokenStore`] trait through the bundled
//! in-memory impl. The trait is intentionally narrow — save, load,
//! delete — so a small fixture-style test covers the observable
//! behavior the appview depends on.

use idiolect_oauth::{InMemoryOAuthTokenStore, OAuthSession, OAuthTokenStore};

fn session(did: &str) -> OAuthSession {
    OAuthSession::new(
        did,
        "https://pds.example.com",
        "access-jwt",
        "refresh-jwt",
        "dpop-jwk",
        "2026-04-19T00:00:00.000Z",
        "2026-04-19T01:00:00.000Z",
    )
}

#[tokio::test(flavor = "current_thread")]
async fn save_then_load_returns_same_session() {
    let store = InMemoryOAuthTokenStore::new();
    let original = session("did:plc:alice");

    store.save(&original).await.expect("save succeeds");

    let loaded = store
        .load("did:plc:alice")
        .await
        .expect("load succeeds")
        .expect("session is present");

    assert_eq!(loaded, original);
}

#[tokio::test(flavor = "current_thread")]
async fn load_missing_did_returns_none() {
    let store = InMemoryOAuthTokenStore::new();
    assert!(store.is_empty());

    let loaded = store.load("did:plc:missing").await.expect("load succeeds");
    assert!(loaded.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn save_replaces_existing_session_for_same_did() {
    let store = InMemoryOAuthTokenStore::new();

    let mut first = session("did:plc:alice");
    first.access_jwt = "old-jwt".to_owned();
    store.save(&first).await.expect("save first");

    let mut second = session("did:plc:alice");
    second.access_jwt = "new-jwt".to_owned();
    store.save(&second).await.expect("save second");

    let loaded = store
        .load("did:plc:alice")
        .await
        .expect("load succeeds")
        .expect("session is present");

    assert_eq!(loaded.access_jwt, "new-jwt");
    assert_eq!(store.len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn delete_removes_session() {
    let store = InMemoryOAuthTokenStore::new();
    store
        .save(&session("did:plc:alice"))
        .await
        .expect("save succeeds");
    assert_eq!(store.len(), 1);

    store
        .delete("did:plc:alice")
        .await
        .expect("delete succeeds");
    assert!(store.is_empty());

    let loaded = store.load("did:plc:alice").await.expect("load succeeds");
    assert!(loaded.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn delete_missing_did_is_a_noop() {
    // storage backends differ in whether deleting a missing key is
    // an error; the trait contract is that it must succeed without
    // raising. the in-memory impl must honor that.
    let store = InMemoryOAuthTokenStore::new();
    store
        .delete("did:plc:never-saved")
        .await
        .expect("delete of missing key is a no-op");
}

#[tokio::test(flavor = "current_thread")]
async fn separate_dids_do_not_collide() {
    let store = InMemoryOAuthTokenStore::new();
    store.save(&session("did:plc:alice")).await.unwrap();
    store.save(&session("did:plc:bob")).await.unwrap();

    assert_eq!(store.len(), 2);

    let alice = store.load("did:plc:alice").await.unwrap();
    let bob = store.load("did:plc:bob").await.unwrap();
    assert!(alice.is_some());
    assert!(bob.is_some());
    assert_eq!(alice.unwrap().did, "did:plc:alice");
    assert_eq!(bob.unwrap().did, "did:plc:bob");
}

#[tokio::test(flavor = "current_thread")]
async fn list_dids_is_sorted_and_covers_every_saved_did() {
    let s = InMemoryOAuthTokenStore::new();
    assert!(s.list_dids().await.unwrap().is_empty());
    s.save(&session("did:plc:zeta")).await.unwrap();
    s.save(&session("did:plc:alpha")).await.unwrap();
    s.save(&session("did:plc:mu")).await.unwrap();
    assert_eq!(
        s.list_dids().await.unwrap(),
        vec![
            "did:plc:alpha".to_owned(),
            "did:plc:mu".to_owned(),
            "did:plc:zeta".to_owned(),
        ]
    );
    // Deleting a session removes it from the list.
    s.delete("did:plc:mu").await.unwrap();
    assert_eq!(s.list_dids().await.unwrap().len(), 2);
}
