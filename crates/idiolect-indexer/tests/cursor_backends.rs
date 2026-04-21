//! Tests for the filesystem- and sqlite-backed cursor stores.
//!
//! Both stores must pass the same contract as
//! [`InMemoryCursorStore`](idiolect_indexer::InMemoryCursorStore):
//!
//! - `load` for a never-seen subscription id returns `None`.
//! - `commit` followed by `load` returns the committed value.
//! - Overwriting a commit is allowed (it rewinds or advances).
//! - Multiple subscription ids are independent.
//! - Commits survive reopening the store at the same path.

#![cfg(any(feature = "cursor-filesystem", feature = "cursor-sqlite"))]

use idiolect_indexer::CursorStore;

#[cfg(feature = "cursor-filesystem")]
mod fs_tests {
    use super::*;
    use idiolect_indexer::FilesystemCursorStore;
    use tempfile::tempdir;

    #[tokio::test]
    async fn fs_round_trip_single_subscription() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cursors.json");
        let store = FilesystemCursorStore::open(&path).unwrap();

        assert!(store.load("sub").await.unwrap().is_none());

        store.commit("sub", 42).await.unwrap();
        assert_eq!(store.load("sub").await.unwrap(), Some(42));

        store.commit("sub", 100).await.unwrap();
        assert_eq!(store.load("sub").await.unwrap(), Some(100));
    }

    #[tokio::test]
    async fn fs_list_returns_every_committed_subscription() {
        let dir = tempdir().unwrap();
        let store = FilesystemCursorStore::open(dir.path().join("c.json")).unwrap();
        assert!(store.list().await.unwrap().is_empty());
        store.commit("b", 2).await.unwrap();
        store.commit("a", 1).await.unwrap();
        store.commit("c", 3).await.unwrap();
        // list() is sorted by subscription id.
        assert_eq!(
            store.list().await.unwrap(),
            vec![("a".into(), 1), ("b".into(), 2), ("c".into(), 3)],
        );
    }

    #[tokio::test]
    async fn fs_multiple_subscriptions_are_independent() {
        let dir = tempdir().unwrap();
        let store = FilesystemCursorStore::open(dir.path().join("c.json")).unwrap();
        store.commit("a", 1).await.unwrap();
        store.commit("b", 2).await.unwrap();
        store.commit("c", 3).await.unwrap();
        assert_eq!(store.load("a").await.unwrap(), Some(1));
        assert_eq!(store.load("b").await.unwrap(), Some(2));
        assert_eq!(store.load("c").await.unwrap(), Some(3));
        assert!(store.load("d").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn fs_survives_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cursors.json");
        {
            let store = FilesystemCursorStore::open(&path).unwrap();
            store.commit("sub-a", 7).await.unwrap();
            store.commit("sub-b", 99).await.unwrap();
        }
        // a fresh store at the same path reads the on-disk state.
        let reopened = FilesystemCursorStore::open(&path).unwrap();
        assert_eq!(reopened.load("sub-a").await.unwrap(), Some(7));
        assert_eq!(reopened.load("sub-b").await.unwrap(), Some(99));
    }

    #[tokio::test]
    async fn fs_missing_file_starts_empty() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("absent.json");
        let store = FilesystemCursorStore::open(&path).unwrap();
        assert!(store.load("anything").await.unwrap().is_none());
        // commit creates the file.
        store.commit("s", 1).await.unwrap();
        assert!(path.exists());
    }

    #[tokio::test]
    async fn fs_rejects_corrupt_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, b"not json").unwrap();
        let err = FilesystemCursorStore::open(&path).unwrap_err();
        assert!(err.to_string().contains("parse"), "unexpected error: {err}");
    }
}

#[cfg(feature = "cursor-sqlite")]
mod sqlite_tests {
    use super::*;
    use idiolect_indexer::SqliteCursorStore;
    use tempfile::tempdir;

    #[tokio::test]
    async fn sqlite_round_trip_single_subscription() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cursors.db");
        let store = SqliteCursorStore::open(&path).unwrap();

        assert!(store.load("sub").await.unwrap().is_none());

        store.commit("sub", 42).await.unwrap();
        assert_eq!(store.load("sub").await.unwrap(), Some(42));

        store.commit("sub", 100).await.unwrap();
        assert_eq!(store.load("sub").await.unwrap(), Some(100));
    }

    #[tokio::test]
    async fn sqlite_list_returns_sorted_subscriptions() {
        let dir = tempdir().unwrap();
        let store = SqliteCursorStore::open(dir.path().join("c.db")).unwrap();
        assert!(store.list().await.unwrap().is_empty());
        store.commit("z", 100).await.unwrap();
        store.commit("a", 1).await.unwrap();
        store.commit("m", 50).await.unwrap();
        assert_eq!(
            store.list().await.unwrap(),
            vec![("a".into(), 1), ("m".into(), 50), ("z".into(), 100)],
        );
    }

    #[tokio::test]
    async fn sqlite_multiple_subscriptions_are_independent() {
        let dir = tempdir().unwrap();
        let store = SqliteCursorStore::open(dir.path().join("c.db")).unwrap();
        store.commit("a", 1).await.unwrap();
        store.commit("b", 2).await.unwrap();
        store.commit("c", 3).await.unwrap();
        assert_eq!(store.load("a").await.unwrap(), Some(1));
        assert_eq!(store.load("b").await.unwrap(), Some(2));
        assert_eq!(store.load("c").await.unwrap(), Some(3));
        assert!(store.load("d").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn sqlite_survives_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cursors.db");
        {
            let store = SqliteCursorStore::open(&path).unwrap();
            store.commit("sub-a", 7).await.unwrap();
            store.commit("sub-b", 99).await.unwrap();
        }
        let reopened = SqliteCursorStore::open(&path).unwrap();
        assert_eq!(reopened.load("sub-a").await.unwrap(), Some(7));
        assert_eq!(reopened.load("sub-b").await.unwrap(), Some(99));
    }

    #[tokio::test]
    async fn sqlite_rejects_out_of_range_seq() {
        let dir = tempdir().unwrap();
        let store = SqliteCursorStore::open(dir.path().join("c.db")).unwrap();
        // i64::MAX fits; MAX+1 does not.
        #[allow(clippy::cast_sign_loss)]
        let ok = i64::MAX as u64;
        store.commit("ok", ok).await.unwrap();
        let too_big = ok + 1;
        let err = store.commit("too-big", too_big).await.unwrap_err();
        assert!(err.to_string().contains("exceeds sqlite i64 range"));
    }
}
