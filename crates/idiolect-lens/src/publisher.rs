//! Generic [`RecordPublisher`] — publish any `idiolect_records::Record`
//! to a PDS.
//!
//! The observer crate's `PdsPublisher` is specialized to `Observation`;
//! every other record kind that needs to be authored — bounties,
//! adapters, verifications, community declarations, dialects,
//! recommendations, corrections — goes through the generic version
//! shipped here. The split matches the layering: the lens crate owns
//! the `PdsWriter` boundary, so generic record publishing also belongs
//! here.
//!
//! # Why generic?
//!
//! `PdsWriter::create_record` takes a raw `serde_json::Value` and a
//! `collection` string. Every caller that publishes a typed record
//! ends up writing the same adapter code:
//!
//! 1. serialize the record,
//! 2. splice the `$type` field in,
//! 3. call `create_record`.
//!
//! `RecordPublisher<W>` absorbs all three steps and exposes one method
//! per lifecycle action (`create`, `put`, `delete`) that accepts the
//! typed record directly.
//!
//! # Authoring auth
//!
//! The publisher does not authenticate. Callers configure their
//! `PdsWriter` (atrium or reqwest) with the credentials of the DID
//! that should own the record; publishing under someone else's DID
//! requires their session, which is a concern beyond this module.

use idiolect_records::Record;

use crate::error::LensError;
use crate::resolver::{
    CreateRecordRequest, DeleteRecordRequest, PdsWriter, PutRecordRequest, WriteRecordResponse,
};

/// Published response for writes that do not return a CID/URI (delete).
pub type DeleteResponse = ();

/// Typed publisher wrapping any [`PdsWriter`].
#[derive(Debug, Clone)]
pub struct RecordPublisher<W> {
    /// Underlying write-capable PDS client.
    writer: W,
    /// DID of the repo records land in. Every publish call writes
    /// under this DID's repo; if a caller needs to author records for
    /// multiple DIDs they construct one `RecordPublisher` per repo.
    repo: String,
}

impl<W> RecordPublisher<W> {
    /// Wrap a writer and pin it to `repo`.
    pub const fn new(writer: W, repo: String) -> Self {
        Self { writer, repo }
    }

    /// Borrow the underlying writer.
    pub const fn writer(&self) -> &W {
        &self.writer
    }

    /// Borrow the repo DID all publish calls target.
    pub fn repo(&self) -> &str {
        &self.repo
    }
}

impl<W: PdsWriter> RecordPublisher<W> {
    /// Publish a fresh record, letting the PDS allocate a TID rkey.
    ///
    /// Equivalent to [`create_with_rkey`](Self::create_with_rkey) with
    /// `rkey = None`. Serializes the record, injects `$type`, and
    /// forwards to the writer.
    ///
    /// # Errors
    ///
    /// Transport failures collapse to [`LensError::Transport`]. Records
    /// that do not serialize into a json object also surface there,
    /// which should not happen for generated record types.
    pub async fn create<R: Record>(&self, record: &R) -> Result<WriteRecordResponse, LensError> {
        self.create_with_rkey(record, None, None).await
    }

    /// Publish a fresh record, optionally with a caller-chosen rkey
    /// and a `validate` hint.
    ///
    /// # Errors
    ///
    /// See [`create`](Self::create).
    pub async fn create_with_rkey<R: Record>(
        &self,
        record: &R,
        rkey: Option<String>,
        validate: Option<bool>,
    ) -> Result<WriteRecordResponse, LensError> {
        let body = record_with_type(record)?;
        self.writer
            .create_record(CreateRecordRequest {
                repo: self.repo.clone(),
                collection: R::NSID.to_owned(),
                rkey,
                record: body,
                validate,
            })
            .await
    }

    /// Overwrite a record at `(repo, R::NSID, rkey)`.
    ///
    /// Pass `swap_record = Some(cid)` for optimistic concurrency; the
    /// PDS rejects the write when the current record's CID does not
    /// match.
    ///
    /// # Errors
    ///
    /// See [`create`](Self::create). A CAS mismatch surfaces as
    /// [`LensError::Transport`] with the atrium/reqwest message
    /// verbatim.
    pub async fn put<R: Record>(
        &self,
        rkey: impl Into<String>,
        record: &R,
        swap_record: Option<String>,
        validate: Option<bool>,
    ) -> Result<WriteRecordResponse, LensError> {
        let body = record_with_type(record)?;
        self.writer
            .put_record(PutRecordRequest {
                repo: self.repo.clone(),
                collection: R::NSID.to_owned(),
                rkey: rkey.into(),
                record: body,
                validate,
                swap_record,
            })
            .await
    }

    /// Delete a record at `(repo, R::NSID, rkey)`.
    ///
    /// `R` is only used as a type-level witness for the collection
    /// nsid; the record body is not needed to delete.
    ///
    /// # Errors
    ///
    /// See [`create`](Self::create).
    pub async fn delete<R: Record>(
        &self,
        rkey: impl Into<String>,
        swap_record: Option<String>,
    ) -> Result<DeleteResponse, LensError> {
        self.writer
            .delete_record(DeleteRecordRequest {
                repo: self.repo.clone(),
                collection: R::NSID.to_owned(),
                rkey: rkey.into(),
                swap_record,
            })
            .await
    }
}

/// Serialize `record` and splice the `$type` field into the resulting
/// json object so the PDS recognizes the record as lexicon-typed.
///
/// Returns [`LensError::Transport`] when the serialized form is not an
/// object (generated record types always serialize to objects; this is
/// a defensive error rather than an expected path).
fn record_with_type<R: Record>(record: &R) -> Result<serde_json::Value, LensError> {
    let mut value = serde_json::to_value(record)
        .map_err(|e| LensError::Transport(format!("serialize {}: {e}", R::NSID)))?;
    let obj = value.as_object_mut().ok_or_else(|| {
        LensError::Transport(format!("{} did not serialize to a json object", R::NSID))
    })?;
    obj.insert(
        "$type".to_owned(),
        serde_json::Value::String(R::NSID.to_owned()),
    );
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::{PdsClient, PdsWriter};
    use idiolect_records::generated::dev::idiolect::bounty::{Bounty, BountyStatus, BountyWants, WantAdapter};
    use std::sync::Mutex;

    use std::sync::Arc;

    #[derive(Clone, Default)]
    #[allow(clippy::struct_field_names)]
    struct CapturingWriter {
        last_create: Arc<Mutex<Option<CreateRecordRequest>>>,
        last_put: Arc<Mutex<Option<PutRecordRequest>>>,
        last_delete: Arc<Mutex<Option<DeleteRecordRequest>>>,
    }

    impl CapturingWriter {
        fn new() -> Self {
            Self::default()
        }
    }

    impl PdsClient for CapturingWriter {
        async fn get_record(
            &self,
            _did: &str,
            _collection: &str,
            _rkey: &str,
        ) -> Result<serde_json::Value, LensError> {
            Err(LensError::Transport("not wired".into()))
        }
    }

    impl PdsWriter for CapturingWriter {
        async fn create_record(
            &self,
            req: CreateRecordRequest,
        ) -> Result<WriteRecordResponse, LensError> {
            *self.last_create.lock().unwrap() = Some(req);
            Ok(WriteRecordResponse {
                uri: "at://did:plc:x/dev.idiolect.bounty/3l5".into(),
                cid: "bafyrei-create".into(),
            })
        }

        async fn put_record(
            &self,
            req: PutRecordRequest,
        ) -> Result<WriteRecordResponse, LensError> {
            *self.last_put.lock().unwrap() = Some(req);
            Ok(WriteRecordResponse {
                uri: "at://did:plc:x/dev.idiolect.bounty/3l5".into(),
                cid: "bafyrei-put".into(),
            })
        }

        async fn delete_record(&self, req: DeleteRecordRequest) -> Result<(), LensError> {
            *self.last_delete.lock().unwrap() = Some(req);
            Ok(())
        }
    }

    fn sample_bounty() -> Bounty {
        Bounty {
            basis: None,
            constraints: None,
            eligibility: None,
            fulfillment: None,
            occurred_at: "2026-04-20T00:00:00Z".into(),
            requester: "did:plc:alice".into(),
            reward: None,
            status: Some(BountyStatus::Open),
            wants: BountyWants::WantAdapter(WantAdapter {
                framework: "hasura".into(),
                version_range: None,
            }),
        }
    }

    #[tokio::test]
    async fn create_injects_type_and_sets_collection() {
        let writer = CapturingWriter::new();
        let publisher = RecordPublisher::new(writer.clone(), "did:plc:alice".into());
        let resp = publisher.create(&sample_bounty()).await.unwrap();
        assert_eq!(resp.cid, "bafyrei-create");

        let req = writer.last_create.lock().unwrap().clone().unwrap();
        assert_eq!(req.repo, "did:plc:alice");
        assert_eq!(req.collection, "dev.idiolect.bounty");
        assert_eq!(
            req.record.get("$type").unwrap().as_str().unwrap(),
            "dev.idiolect.bounty"
        );
        assert_eq!(req.rkey, None);
    }

    #[tokio::test]
    async fn create_with_rkey_forwards_rkey_and_validate() {
        let writer = CapturingWriter::new();
        let publisher = RecordPublisher::new(writer.clone(), "did:plc:alice".into());
        publisher
            .create_with_rkey(&sample_bounty(), Some("my-rkey".into()), Some(true))
            .await
            .unwrap();
        let req = writer.last_create.lock().unwrap().clone().unwrap();
        assert_eq!(req.rkey.as_deref(), Some("my-rkey"));
        assert_eq!(req.validate, Some(true));
    }

    #[tokio::test]
    async fn put_forwards_rkey_swap() {
        let writer = CapturingWriter::new();
        let publisher = RecordPublisher::new(writer.clone(), "did:plc:alice".into());
        publisher
            .put("3l5", &sample_bounty(), Some("bafyrei-stale".into()), None)
            .await
            .unwrap();
        let req = writer.last_put.lock().unwrap().clone().unwrap();
        assert_eq!(req.rkey, "3l5");
        assert_eq!(req.swap_record.as_deref(), Some("bafyrei-stale"));
        assert_eq!(
            req.record.get("$type").unwrap().as_str().unwrap(),
            "dev.idiolect.bounty"
        );
    }

    #[tokio::test]
    async fn delete_requires_only_type_parameter() {
        let writer = CapturingWriter::new();
        let publisher = RecordPublisher::new(writer.clone(), "did:plc:alice".into());
        publisher.delete::<Bounty>("3l5", None).await.unwrap();
        let req = writer.last_delete.lock().unwrap().clone().unwrap();
        assert_eq!(req.collection, "dev.idiolect.bounty");
        assert_eq!(req.rkey, "3l5");
    }
}
