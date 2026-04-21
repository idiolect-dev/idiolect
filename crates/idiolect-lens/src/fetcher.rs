//! Typed read side of PDS record access.
//!
//! [`RecordFetcher`] is the read-only companion of
//! [`RecordPublisher`](crate::RecordPublisher): it wraps any
//! [`PdsClient`] and turns `(did, rkey)` pairs into strongly-typed
//! record values by decoding the raw json against the
//! [`idiolect_records::Record`] trait.
//!
//! Without this wrapper every caller that reads a record ends up
//! writing the same three-step adapter — call `get_record`, check for
//! a `value` field (already unwrapped by `PdsClient`), then
//! `serde_json::from_value`. Lifting that into one method keeps
//! downstream code free of json handling.
//!
//! # Authentication
//!
//! Some record kinds in the `dev.idiolect.*` family are published
//! with `visibility = public-detailed`, others require the reader to
//! be in scope. `RecordFetcher` does not authenticate; callers who
//! need auth configure the underlying [`PdsClient`] (a pre-wired
//! reqwest client, an atrium session, etc.) before wrapping it.

use idiolect_records::Record;

use crate::error::LensError;
use crate::resolver::PdsClient;

/// Typed record fetcher over any [`PdsClient`].
#[derive(Debug, Clone)]
pub struct RecordFetcher<C> {
    /// Underlying read-capable PDS client.
    client: C,
}

impl<C> RecordFetcher<C> {
    /// Wrap a `PdsClient`.
    pub const fn new(client: C) -> Self {
        Self { client }
    }

    /// Borrow the underlying client.
    pub const fn client(&self) -> &C {
        &self.client
    }
}

impl<C: PdsClient> RecordFetcher<C> {
    /// Fetch and decode the record at `(did, R::NSID, rkey)`.
    ///
    /// The collection is read from the `Record` trait's `NSID`
    /// constant, so callers never type the collection string at the
    /// call site and the fetcher cannot route traffic at the wrong
    /// collection by mistake.
    ///
    /// # Errors
    ///
    /// - [`LensError::NotFound`] when the PDS returns
    ///   `RecordNotFound`.
    /// - [`LensError::Transport`] for any other transport-level
    ///   failure.
    /// - [`LensError::Decode`] when the record body fails to
    ///   deserialize into `R`.
    pub async fn fetch<R: Record>(&self, did: &str, rkey: &str) -> Result<R, LensError> {
        let body = self.client.get_record(did, R::NSID, rkey).await?;
        serde_json::from_value::<R>(body).map_err(LensError::from)
    }

    /// Convenience: fetch via an at-uri rather than split components.
    ///
    /// The at-uri is parsed and its collection is cross-checked
    /// against `R::NSID`; a mismatch surfaces as
    /// [`LensError::Transport`] with an operator-friendly message.
    ///
    /// # Errors
    ///
    /// See [`fetch`](Self::fetch), plus [`LensError::Transport`] for
    /// malformed at-uris and collection mismatches.
    pub async fn fetch_at_uri<R: Record>(&self, at_uri: &str) -> Result<R, LensError> {
        let parsed = crate::at_uri::parse_at_uri(at_uri)
            .map_err(|e| LensError::Transport(format!("at-uri parse {at_uri}: {e}")))?;
        if parsed.collection() != R::NSID {
            return Err(LensError::Transport(format!(
                "at-uri {at_uri} collection {} does not match expected {}",
                parsed.collection(),
                R::NSID,
            )));
        }
        self.fetch(parsed.did(), parsed.rkey()).await
    }

    /// List records of type `R` in `did`'s repo, decoding each into
    /// the typed record. Returns the decoded page plus the xrpc
    /// `cursor` for pagination.
    ///
    /// # Errors
    ///
    /// - [`LensError::NotFound`] when the PDS cannot find the repo
    ///   or collection.
    /// - [`LensError::Transport`] for any other transport-level
    ///   failure, or when a record's `value` body fails to decode
    ///   into `R`. The failing `uri` is stamped in the error message
    ///   so operators can narrow to a single bad record.
    pub async fn list_records<R: Record>(
        &self,
        did: &str,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> Result<ListedPage<R>, LensError> {
        let raw = self
            .client
            .list_records(did, R::NSID, limit, cursor)
            .await?;
        let mut records = Vec::with_capacity(raw.records.len());
        for entry in raw.records {
            let decoded = serde_json::from_value::<R>(entry.value).map_err(|e| {
                LensError::Transport(format!(
                    "decode record {}: {e}",
                    entry.uri
                ))
            })?;
            records.push(ListedEntry {
                uri: entry.uri,
                cid: entry.cid,
                record: decoded,
            });
        }
        Ok(ListedPage {
            records,
            cursor: raw.cursor,
        })
    }
}

/// Typed counterpart of
/// [`ListRecordsResponse`](crate::resolver::ListRecordsResponse):
/// each entry carries a decoded `R` in place of the raw json value.
#[derive(Debug, Clone)]
pub struct ListedPage<R> {
    /// Decoded records in document order returned by the PDS.
    pub records: Vec<ListedEntry<R>>,
    /// Opaque cursor for the next page, or `None` when the last page
    /// has been returned.
    pub cursor: Option<String>,
}

/// Typed counterpart of
/// [`ListedRecord`](crate::resolver::ListedRecord).
#[derive(Debug, Clone)]
pub struct ListedEntry<R> {
    /// Canonical at-uri for the record.
    pub uri: String,
    /// Content-addressed identifier.
    pub cid: String,
    /// Typed decoded body.
    pub record: R,
}

#[cfg(test)]
mod tests {
    use super::*;
    use idiolect_records::generated::bounty::{Bounty, BountyStatus, BountyWants, WantAdapter};
    use std::sync::Mutex;

    #[derive(Default)]
    struct StaticClient {
        last: Mutex<Option<(String, String, String)>>,
        response: Mutex<Option<serde_json::Value>>,
        fail: Mutex<Option<LensError>>,
    }

    impl PdsClient for StaticClient {
        async fn get_record(
            &self,
            did: &str,
            collection: &str,
            rkey: &str,
        ) -> Result<serde_json::Value, LensError> {
            *self.last.lock().unwrap() =
                Some((did.to_owned(), collection.to_owned(), rkey.to_owned()));
            if let Some(err) = self.fail.lock().unwrap().take() {
                return Err(err);
            }
            self.response
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| LensError::Transport("no response configured".into()))
        }
    }

    fn sample_bounty_json() -> serde_json::Value {
        serde_json::json!({
            "constraints": "x",
            "occurredAt": "2026-04-21T00:00:00Z",
            "requester": "did:plc:alice",
            "status": "open",
            "wants": {
                "$type": "dev.idiolect.bounty#wantAdapter",
                "framework": "hasura"
            }
        })
    }

    #[tokio::test]
    async fn fetch_routes_did_and_nsid() {
        let client = StaticClient::default();
        *client.response.lock().unwrap() = Some(sample_bounty_json());
        let fetcher = RecordFetcher::new(client);

        let got: Bounty = fetcher.fetch("did:plc:alice", "b1").await.unwrap();
        assert_eq!(got.status, Some(BountyStatus::Open));
        assert!(matches!(got.wants, BountyWants::WantAdapter(WantAdapter { .. })));

        let last = fetcher.client().last.lock().unwrap().clone().unwrap();
        assert_eq!(last.0, "did:plc:alice");
        assert_eq!(last.1, "dev.idiolect.bounty");
        assert_eq!(last.2, "b1");
    }

    #[tokio::test]
    async fn fetch_surfaces_not_found_verbatim() {
        let client = StaticClient::default();
        *client.fail.lock().unwrap() = Some(LensError::NotFound("RecordNotFound".into()));
        let fetcher = RecordFetcher::new(client);
        let err = fetcher.fetch::<Bounty>("did:plc:x", "b1").await.unwrap_err();
        assert!(matches!(err, LensError::NotFound(_)));
    }

    #[tokio::test]
    async fn fetch_maps_bad_json_to_decode_error() {
        let client = StaticClient::default();
        *client.response.lock().unwrap() = Some(serde_json::json!({ "unexpected": true }));
        let fetcher = RecordFetcher::new(client);
        let err = fetcher.fetch::<Bounty>("did:plc:x", "b1").await.unwrap_err();
        // `From<serde_json::Error> for LensError` lands in the serde
        // variant — confirm it's not NotFound.
        assert!(!matches!(err, LensError::NotFound(_)));
    }

    #[tokio::test]
    async fn fetch_at_uri_parses_and_routes() {
        let client = StaticClient::default();
        *client.response.lock().unwrap() = Some(sample_bounty_json());
        let fetcher = RecordFetcher::new(client);
        let uri = "at://did:plc:alice/dev.idiolect.bounty/b1";
        let got: Bounty = fetcher.fetch_at_uri(uri).await.unwrap();
        assert_eq!(got.requester, "did:plc:alice");
        let last = fetcher.client().last.lock().unwrap().clone().unwrap();
        assert_eq!(last.0, "did:plc:alice");
        assert_eq!(last.1, "dev.idiolect.bounty");
        assert_eq!(last.2, "b1");
    }

    #[tokio::test]
    async fn fetch_at_uri_collection_mismatch_errors() {
        let client = StaticClient::default();
        let fetcher = RecordFetcher::new(client);
        let uri = "at://did:plc:alice/dev.idiolect.adapter/a1";
        let err = fetcher.fetch_at_uri::<Bounty>(uri).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("does not match expected"), "got: {msg}");
    }

    #[tokio::test]
    async fn fetch_at_uri_malformed_errors() {
        let client = StaticClient::default();
        let fetcher = RecordFetcher::new(client);
        let err = fetcher
            .fetch_at_uri::<Bounty>("http://not-an-at-uri")
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("at-uri parse"), "got: {msg}");
    }
}
