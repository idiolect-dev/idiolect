//! Resolvers locate a [`PanprotoLens`] record given an at-uri.
//!
//! The `Resolver` trait is the single abstraction the lens runtime
//! consumes: any backend that can turn an at-uri into a
//! `PanprotoLens` (in-memory fixture table, live PDS, panproto vcs
//! store) implements one trait and slots into the same `apply_lens`
//! pipeline.
//!
//! Three backends ship in this crate:
//!
//! - [`InMemoryResolver`] — a `HashMap` lookup. Used in unit tests and
//!   for offline fixtures.
//! - [`PdsResolver`] — generic over a [`PdsClient`]. The resolver does
//!   the at-uri split; the client does the http GET. This split keeps
//!   the core crate free of a concrete http dep so downstream code can
//!   inject reqwest, ureq, hyper, or a test double without any feature
//!   flag gymnastics.
//! - [`PanprotoVcsResolver`] — generic over a [`PanprotoVcsClient`].
//!   The resolver asks the client for the at-uri's current ref hash
//!   and then for the content-addressed lens object. The mutable ref
//!   table and the immutable object store both live behind the
//!   client; the resolver itself is stateless. An
//!   [`InMemoryVcsClient`] is included so tests and fixtures can
//!   stand up a content-addressed store without an external
//!   panproto-vcs dependency.
//!
//! The write side of a PDS — creating, updating, and deleting records —
//! lives behind the separate [`PdsWriter`] trait so callers that only
//! need read access can accept the narrower capability. The atrium
//! implementation (`AtriumPdsClient`, behind `pds-atrium`) implements
//! both.

use std::collections::HashMap;

use idiolect_records::PanprotoLens;

use crate::AtUri;
use crate::error::LensError;

// -----------------------------------------------------------------
// trait
// -----------------------------------------------------------------

/// Backend-agnostic lens resolver.
///
/// Implementations are responsible for all transport and decode work.
/// `apply_lens` consumers only see this one trait, so swapping a
/// test double for a live PDS client changes a constructor arg, not a
/// call site.
///
/// The trait uses native async fn; `async_fn_in_trait` is allowed at
/// the crate level. `Send + Sync` bounds are required so the
/// resolver can be shared across tokio tasks and its futures stay
/// `Send`.
#[allow(async_fn_in_trait)]
pub trait Resolver: Send + Sync {
    /// Fetch the lens record identified by `uri`.
    ///
    /// # Errors
    ///
    /// See [`LensError`] for the normalized variants every backend
    /// produces. Backend-specific errors are wrapped into one of those
    /// variants at the resolver layer; callers never pattern-match on
    /// transport types.
    async fn resolve(&self, uri: &AtUri) -> Result<PanprotoLens, LensError>;
}

// -----------------------------------------------------------------
// in-memory
// -----------------------------------------------------------------

/// A `HashMap`-backed resolver, keyed by the full at-uri string.
///
/// Intended for fixtures and unit tests. The stored `PanprotoLens`
/// values are cloned on every `resolve` call; that is cheap for
/// fixture data and keeps the trait signature symmetric with the
/// network-backed resolvers.
#[derive(Debug, Default, Clone)]
pub struct InMemoryResolver {
    entries: HashMap<String, PanprotoLens>,
}

impl InMemoryResolver {
    /// Construct an empty resolver.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or replace) a lens record at the given at-uri.
    pub fn insert(&mut self, uri: &AtUri, lens: PanprotoLens) {
        self.entries.insert(uri.to_string(), lens);
    }

    /// Number of registered lens records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether any lens records are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Resolver for InMemoryResolver {
    async fn resolve(&self, uri: &AtUri) -> Result<PanprotoLens, LensError> {
        self.entries
            .get(&uri.to_string())
            .cloned()
            .ok_or_else(|| LensError::NotFound(uri.to_string()))
    }
}

/// Forward [`Resolver`] through a shared `Arc<T>`. Lets a single
/// resolver instance be shared across the lens runtime, the caching
/// layer, and any other consumer without a manual delegate impl.
impl<T: Resolver + ?Sized> Resolver for std::sync::Arc<T> {
    async fn resolve(&self, uri: &AtUri) -> Result<PanprotoLens, LensError> {
        (**self).resolve(uri).await
    }
}

// -----------------------------------------------------------------
// pds
// -----------------------------------------------------------------

/// A thin abstraction over `com.atproto.repo.getRecord` fetches.
///
/// Implementations decide how to map a did to a PDS endpoint (static
/// config, plc.directory lookup, bsky entryway, etc.) and how to run
/// the http request. The returned json is the raw record body
/// (the `value` field of the xrpc response, not the response
/// envelope).
#[allow(async_fn_in_trait)]
pub trait PdsClient: Send + Sync {
    /// Fetch a record's body json by repo / collection / rkey.
    ///
    /// # Errors
    ///
    /// Implementations should return [`LensError::NotFound`] for
    /// `RecordNotFound`-shaped backend responses and
    /// [`LensError::Transport`] for any other failure. Decode errors
    /// are surfaced by the caller (`PdsResolver::resolve`), not by the
    /// client itself, since the client returns raw json.
    async fn get_record(
        &self,
        did: &str,
        collection: &str,
        rkey: &str,
    ) -> Result<serde_json::Value, LensError>;

    /// List records in a repo + collection, paginated.
    ///
    /// Mirrors `com.atproto.repo.listRecords`. Returns
    /// `(records, next_cursor)` where each record is the xrpc
    /// response's `records[i]` shape — `{ uri, cid, value }` — and
    /// `next_cursor` is `Some(s)` when more records exist.
    ///
    /// The default implementation returns
    /// [`LensError::Transport`] with an "unsupported" message; real
    /// PDS clients (`ReqwestPdsClient`, `AtriumPdsClient`) override
    /// with the actual request. In-memory test fixtures that only
    /// need `get_record` can keep the default.
    ///
    /// # Errors
    ///
    /// Returns [`LensError::Transport`] for backend failures. A
    /// `repo not found` returns [`LensError::NotFound`].
    async fn list_records(
        &self,
        _did: &str,
        _collection: &str,
        _limit: Option<u32>,
        _cursor: Option<&str>,
    ) -> Result<ListRecordsResponse, LensError> {
        Err(LensError::Transport(
            "list_records not implemented on this PdsClient".into(),
        ))
    }
}

/// Response shape from `com.atproto.repo.listRecords`.
///
/// Mirrors the xrpc response body one-to-one. Callers decode each
/// entry's `value` into a typed record via
/// [`RecordFetcher::list_records`](crate::RecordFetcher::list_records).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ListRecordsResponse {
    /// One entry per returned record.
    pub records: Vec<ListedRecord>,
    /// Opaque pagination cursor; `None` when the last page was returned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// A single entry in a [`ListRecordsResponse`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ListedRecord {
    /// Canonical at-uri for the record.
    pub uri: String,
    /// Content-addressed identifier (the record's CID).
    pub cid: String,
    /// Raw record body — decode to a typed
    /// [`Record`](idiolect_records::Record) via serde.
    pub value: serde_json::Value,
}

/// Resolves lens records from a PDS via a [`PdsClient`].
///
/// The resolver is transport-agnostic: it only knows how to split an
/// at-uri into `(did, collection, rkey)` and hand those three strings
/// to the injected client. Swapping a live reqwest client for a
/// replay client is a single `let` binding.
#[derive(Debug, Clone)]
pub struct PdsResolver<C> {
    client: C,
}

impl<C> PdsResolver<C> {
    /// Wrap a `PdsClient`.
    #[must_use]
    pub const fn new(client: C) -> Self {
        Self { client }
    }

    /// Borrow the underlying client, e.g. to inspect it in a test.
    #[must_use]
    pub const fn client(&self) -> &C {
        &self.client
    }
}

/// Forward [`PdsClient`] through a shared `Arc<T>`. Makes it
/// practical to hand the same authenticated client to a resolver and
/// a fetcher and a writer without cloning the underlying transport.
impl<T: PdsClient + ?Sized> PdsClient for std::sync::Arc<T> {
    async fn get_record(
        &self,
        did: &str,
        collection: &str,
        rkey: &str,
    ) -> Result<serde_json::Value, LensError> {
        (**self).get_record(did, collection, rkey).await
    }

    async fn list_records(
        &self,
        did: &str,
        collection: &str,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> Result<ListRecordsResponse, LensError> {
        (**self).list_records(did, collection, limit, cursor).await
    }
}

impl<C: PdsClient> Resolver for PdsResolver<C> {
    async fn resolve(&self, uri: &AtUri) -> Result<PanprotoLens, LensError> {
        let body = self
            .client
            .get_record(uri.did().as_str(), uri.collection().as_str(), uri.rkey())
            .await?;

        serde_json::from_value::<PanprotoLens>(body).map_err(LensError::from)
    }
}

// -----------------------------------------------------------------
// pds writer
// -----------------------------------------------------------------

/// Parameters for a `com.atproto.repo.createRecord` call.
///
/// Kept as a plain struct so callers can populate it without pulling
/// any atrium types into their signatures. The transport-agnostic
/// shape also keeps it usable by alternative clients (ureq, isahc, a
/// test double) without forcing them to adopt atrium's validated
/// string newtypes.
#[derive(Debug, Clone)]
pub struct CreateRecordRequest {
    /// DID (or handle) of the repo the record lands in.
    pub repo: String,
    /// Collection NSID (e.g. `dev.idiolect.observation`).
    pub collection: String,
    /// Optional record key. When `None`, the PDS allocates a TID.
    pub rkey: Option<String>,
    /// Record body as raw json. Should already include the `$type`
    /// field matching `collection`; serializing a typed record via
    /// `serde_json::to_value` is the shortest path.
    pub record: serde_json::Value,
    /// Ask the PDS to validate against its loaded lexicon. `None`
    /// means "validate for known lexicons" (PDS default); `Some(true)`
    /// forces strict validation; `Some(false)` skips it.
    pub validate: Option<bool>,
}

/// Parameters for a `com.atproto.repo.putRecord` call.
///
/// `rkey` is required for put — the whole point is addressing an
/// existing record. `swap_record` supplies optimistic concurrency: if
/// the current record's CID does not match, the PDS rejects the
/// write.
#[derive(Debug, Clone)]
pub struct PutRecordRequest {
    /// DID (or handle) of the repo.
    pub repo: String,
    /// Collection NSID.
    pub collection: String,
    /// Record key being overwritten.
    pub rkey: String,
    /// New record body as raw json.
    pub record: serde_json::Value,
    /// See [`CreateRecordRequest::validate`].
    pub validate: Option<bool>,
    /// Compare-and-swap CID of the existing record. When set, the PDS
    /// rejects the write if the current record has a different CID.
    pub swap_record: Option<String>,
}

/// Parameters for a `com.atproto.repo.deleteRecord` call.
#[derive(Debug, Clone)]
pub struct DeleteRecordRequest {
    /// DID (or handle) of the repo.
    pub repo: String,
    /// Collection NSID.
    pub collection: String,
    /// Record key to delete.
    pub rkey: String,
    /// See [`PutRecordRequest::swap_record`].
    pub swap_record: Option<String>,
}

/// Successful response to a create / put call.
///
/// The PDS returns `cid` and `uri`; downstream callers typically want
/// the at-uri so they can echo it in the response and the cid so they
/// can seed a `strongRef` pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteRecordResponse {
    /// AT-URI of the newly committed record.
    pub uri: String,
    /// CID of the committed record, as a string.
    pub cid: String,
}

/// Backend-agnostic PDS write capability.
///
/// Separate from [`PdsClient`] so read-only consumers (the lens
/// resolver) can accept the narrower capability and so test doubles
/// for read paths do not have to stub writes they never exercise.
///
/// Implementations MUST be [`Send`] + [`Sync`] so the runtime can
/// share one instance across the firehose task and the publisher task.
/// Errors follow the same `LensError::NotFound` / `LensError::Transport`
/// contract as [`PdsClient`]: backend-specific failures collapse to
/// one of those two variants so callers never reach for a transport
/// type.
#[allow(async_fn_in_trait)]
pub trait PdsWriter: Send + Sync {
    /// Create a new record. Mirrors `com.atproto.repo.createRecord`.
    ///
    /// When `request.rkey` is `None` the server assigns a TID. The
    /// returned [`WriteRecordResponse::uri`] is the canonical handle
    /// for the newly committed record regardless.
    ///
    /// # Errors
    ///
    /// Implementations should return [`LensError::Transport`] for any
    /// backend-level failure. Authenticated flows that hit an expired
    /// session surface the atrium message verbatim.
    async fn create_record(
        &self,
        request: CreateRecordRequest,
    ) -> Result<WriteRecordResponse, LensError>;

    /// Overwrite an existing record. Mirrors
    /// `com.atproto.repo.putRecord`.
    ///
    /// # Errors
    ///
    /// See [`create_record`](Self::create_record). An `InvalidSwap`
    /// response (`swap_record` did not match the current CID) also
    /// becomes [`LensError::Transport`] so callers can log the atrium
    /// message; richer variants can be added without a breaking change
    /// because [`LensError`] is `#[non_exhaustive]`.
    async fn put_record(&self, request: PutRecordRequest)
    -> Result<WriteRecordResponse, LensError>;

    /// Remove an existing record. Mirrors
    /// `com.atproto.repo.deleteRecord`.
    ///
    /// # Errors
    ///
    /// See [`create_record`](Self::create_record).
    async fn delete_record(&self, request: DeleteRecordRequest) -> Result<(), LensError>;
}

/// Forward [`PdsWriter`] through a shared `Arc<T>`. Matches the
/// `Arc<dyn PdsWriter>` pattern downstream crates use to inject an
/// authenticated client once and share it across publishers.
impl<T: PdsWriter + ?Sized> PdsWriter for std::sync::Arc<T> {
    async fn create_record(
        &self,
        request: CreateRecordRequest,
    ) -> Result<WriteRecordResponse, LensError> {
        (**self).create_record(request).await
    }

    async fn put_record(
        &self,
        request: PutRecordRequest,
    ) -> Result<WriteRecordResponse, LensError> {
        (**self).put_record(request).await
    }

    async fn delete_record(&self, request: DeleteRecordRequest) -> Result<(), LensError> {
        (**self).delete_record(request).await
    }
}

// -----------------------------------------------------------------
// panproto vcs
// -----------------------------------------------------------------

/// Abstraction over a panproto-vcs-shaped content-addressed store
/// plus its mutable ref table.
///
/// Mirrors the `dev.panproto.sync.*` xrpc surface: object reads
/// (`getObject`), ref reads + writes (`getRef` / `setRef` /
/// `listRefs`), commit-graph traversal (`getHead` / `listCommits`),
/// the schema-tree view (`getSchemaTree`), and registry listings
/// (`listTheories` / `listAlignments`). Concrete impls in scope for
/// idiolect: a live client that speaks the xrpc surface, and the
/// [`InMemoryVcsClient`] fixture shipped below.
#[allow(async_fn_in_trait)]
pub trait PanprotoVcsClient: Send + Sync {
    /// Fetch the content-addressed object identified by `object_hash`.
    ///
    /// # Errors
    ///
    /// Return [`LensError::NotFound`] when no object matches the hash
    /// and [`LensError::Transport`] for any backend-level failure.
    async fn get_object(&self, object_hash: &str) -> Result<serde_json::Value, LensError>;

    /// Resolve a ref name to its current object hash, or `None` if
    /// the ref does not exist.
    ///
    /// # Errors
    ///
    /// [`LensError::Transport`] for any backend-level failure.
    async fn get_ref(&self, name: &str) -> Result<Option<String>, LensError>;

    /// Point `name` at `object_hash`. Creates the ref if absent.
    ///
    /// # Errors
    ///
    /// [`LensError::Transport`] for any backend-level failure.
    async fn set_ref(&self, name: &str, object_hash: &str) -> Result<(), LensError>;

    /// List every `(name, object_hash)` ref the store knows about.
    /// Order is implementation-defined.
    ///
    /// # Errors
    ///
    /// [`LensError::Transport`] for any backend-level failure.
    async fn list_refs(&self) -> Result<Vec<(String, String)>, LensError>;

    /// Walk the commit chain starting at `ref_name`'s head, oldest-
    /// to-newest. `limit = None` means "every commit reachable".
    /// Returns the object hashes of the visited commits.
    ///
    /// # Errors
    ///
    /// [`LensError::NotFound`] when the ref is unknown,
    /// [`LensError::Transport`] for any backend-level failure.
    async fn list_commits(
        &self,
        ref_name: &str,
        limit: Option<u32>,
    ) -> Result<Vec<String>, LensError>;

    /// Object hash of the most recent commit at `ref_name`, or `None`
    /// when the ref is empty.
    ///
    /// # Errors
    ///
    /// [`LensError::Transport`] for any backend-level failure.
    async fn get_head(&self, ref_name: &str) -> Result<Option<String>, LensError>;

    /// Fetch the cst-shaped schema tree rooted at `object_hash`.
    /// Used by the lens runtime when it needs the structural view of
    /// a schema rather than the file blob.
    ///
    /// # Errors
    ///
    /// [`LensError::NotFound`] when the tree is missing,
    /// [`LensError::Transport`] for any backend-level failure.
    async fn get_schema_tree(&self, object_hash: &str) -> Result<serde_json::Value, LensError>;

    /// List every theory id the store recognises (registry view).
    ///
    /// # Errors
    ///
    /// [`LensError::Transport`] for any backend-level failure.
    async fn list_theories(&self) -> Result<Vec<String>, LensError>;

    /// List every theory-alignment id the store recognises (registry
    /// view used by the verify runners to find proof artifacts).
    ///
    /// # Errors
    ///
    /// [`LensError::Transport`] for any backend-level failure.
    async fn list_alignments(&self) -> Result<Vec<String>, LensError>;
}

/// Resolves lens records by asking a [`PanprotoVcsClient`] for the
/// at-uri's current ref hash and then for the object at that hash.
///
/// State (refs + objects) lives in the client; the resolver is
/// stateless. This matches the upstream panproto-vcs contract:
/// `dev.panproto.sync.getRef` plus `dev.panproto.sync.getObject`.
#[derive(Debug, Clone)]
pub struct PanprotoVcsResolver<C> {
    client: C,
}

impl<C> PanprotoVcsResolver<C> {
    /// Wrap a vcs client.
    #[must_use]
    pub const fn new(client: C) -> Self {
        Self { client }
    }

    /// Borrow the underlying client.
    #[must_use]
    pub const fn client(&self) -> &C {
        &self.client
    }
}

impl<C: PanprotoVcsClient> PanprotoVcsResolver<C> {
    /// Point the at-uri `uri` at the content-addressed object
    /// identified by `object_hash` via the underlying client.
    ///
    /// # Errors
    ///
    /// Forwards any [`LensError`] from the client's `set_ref`.
    pub async fn set_ref(&self, uri: &AtUri, object_hash: &str) -> Result<(), LensError> {
        self.client.set_ref(&uri.to_string(), object_hash).await
    }
}

impl<C: PanprotoVcsClient> Resolver for PanprotoVcsResolver<C> {
    async fn resolve(&self, uri: &AtUri) -> Result<PanprotoLens, LensError> {
        let key = uri.to_string();
        let object_hash = self
            .client
            .get_ref(&key)
            .await?
            .ok_or(LensError::NotFound(key))?;

        let body = self.client.get_object(&object_hash).await?;

        serde_json::from_value::<PanprotoLens>(body).map_err(LensError::from)
    }
}

// -----------------------------------------------------------------
// in-memory vcs client (fixture)
// -----------------------------------------------------------------

/// An in-memory panproto-vcs client for tests and fixtures.
///
/// Holds three maps under a single `Mutex`: the immutable object
/// store keyed by content hash, the mutable ref table mapping ref
/// names to head hashes, and a per-ref linear commit history (oldest
/// to newest). Schema-tree, theory, and alignment data are stored
/// alongside as opaque blobs the runner under test can prepopulate.
#[derive(Debug, Default)]
pub struct InMemoryVcsClient {
    state: std::sync::Mutex<InMemoryVcsState>,
}

#[derive(Debug, Default)]
struct InMemoryVcsState {
    objects: HashMap<String, serde_json::Value>,
    refs: HashMap<String, String>,
    commits: HashMap<String, Vec<String>>,
    schema_trees: HashMap<String, serde_json::Value>,
    theories: Vec<String>,
    alignments: Vec<String>,
}

impl InMemoryVcsClient {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, InMemoryVcsState> {
        self.state
            .lock()
            .expect("InMemoryVcsClient state mutex poisoned")
    }

    /// Insert a content-addressed object. The caller is responsible
    /// for ensuring `object_hash` is a faithful hash of `value`; this
    /// store performs no verification.
    pub fn insert_object(&self, object_hash: String, value: serde_json::Value) {
        self.lock().objects.insert(object_hash, value);
    }

    /// Pre-populate a per-ref commit chain (oldest to newest). The
    /// last element becomes the ref's head.
    pub fn insert_commit_chain(&self, ref_name: String, hashes: Vec<String>) {
        let mut state = self.lock();
        if let Some(head) = hashes.last().cloned() {
            state.refs.insert(ref_name.clone(), head);
        }
        state.commits.insert(ref_name, hashes);
    }

    /// Insert the schema-tree blob keyed by `object_hash`.
    pub fn insert_schema_tree(&self, object_hash: String, value: serde_json::Value) {
        self.lock().schema_trees.insert(object_hash, value);
    }

    /// Replace the registry of theory ids.
    pub fn set_theories(&self, theories: Vec<String>) {
        self.lock().theories = theories;
    }

    /// Replace the registry of alignment ids.
    pub fn set_alignments(&self, alignments: Vec<String>) {
        self.lock().alignments = alignments;
    }

    /// Number of stored objects.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().objects.len()
    }

    /// Whether the object store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lock().objects.is_empty()
    }
}

impl PanprotoVcsClient for InMemoryVcsClient {
    async fn get_object(&self, object_hash: &str) -> Result<serde_json::Value, LensError> {
        self.lock()
            .objects
            .get(object_hash)
            .cloned()
            .ok_or_else(|| LensError::NotFound(format!("object:{object_hash}")))
    }

    async fn get_ref(&self, name: &str) -> Result<Option<String>, LensError> {
        Ok(self.lock().refs.get(name).cloned())
    }

    async fn set_ref(&self, name: &str, object_hash: &str) -> Result<(), LensError> {
        self.lock()
            .refs
            .insert(name.to_owned(), object_hash.to_owned());
        Ok(())
    }

    async fn list_refs(&self) -> Result<Vec<(String, String)>, LensError> {
        Ok(self
            .lock()
            .refs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }

    async fn list_commits(
        &self,
        ref_name: &str,
        limit: Option<u32>,
    ) -> Result<Vec<String>, LensError> {
        let state = self.lock();
        let chain = state
            .commits
            .get(ref_name)
            .cloned()
            .ok_or_else(|| LensError::NotFound(format!("ref:{ref_name}")))?;
        let take = limit.map_or(chain.len(), |n| (n as usize).min(chain.len()));
        Ok(chain.into_iter().take(take).collect())
    }

    async fn get_head(&self, ref_name: &str) -> Result<Option<String>, LensError> {
        Ok(self.lock().refs.get(ref_name).cloned())
    }

    async fn get_schema_tree(&self, object_hash: &str) -> Result<serde_json::Value, LensError> {
        self.lock()
            .schema_trees
            .get(object_hash)
            .cloned()
            .ok_or_else(|| LensError::NotFound(format!("schema-tree:{object_hash}")))
    }

    async fn list_theories(&self) -> Result<Vec<String>, LensError> {
        Ok(self.lock().theories.clone())
    }

    async fn list_alignments(&self) -> Result<Vec<String>, LensError> {
        Ok(self.lock().alignments.clone())
    }
}

/// Forward [`PanprotoVcsClient`] through a shared `Arc<T>`.
impl<T: PanprotoVcsClient + ?Sized> PanprotoVcsClient for std::sync::Arc<T> {
    async fn get_object(&self, object_hash: &str) -> Result<serde_json::Value, LensError> {
        (**self).get_object(object_hash).await
    }
    async fn get_ref(&self, name: &str) -> Result<Option<String>, LensError> {
        (**self).get_ref(name).await
    }
    async fn set_ref(&self, name: &str, object_hash: &str) -> Result<(), LensError> {
        (**self).set_ref(name, object_hash).await
    }
    async fn list_refs(&self) -> Result<Vec<(String, String)>, LensError> {
        (**self).list_refs().await
    }
    async fn list_commits(
        &self,
        ref_name: &str,
        limit: Option<u32>,
    ) -> Result<Vec<String>, LensError> {
        (**self).list_commits(ref_name, limit).await
    }
    async fn get_head(&self, ref_name: &str) -> Result<Option<String>, LensError> {
        (**self).get_head(ref_name).await
    }
    async fn get_schema_tree(&self, object_hash: &str) -> Result<serde_json::Value, LensError> {
        (**self).get_schema_tree(object_hash).await
    }
    async fn list_theories(&self) -> Result<Vec<String>, LensError> {
        (**self).list_theories().await
    }
    async fn list_alignments(&self) -> Result<Vec<String>, LensError> {
        (**self).list_alignments().await
    }
}

// -----------------------------------------------------------------
// tests
// -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_lens() -> PanprotoLens {
        PanprotoLens {
            blob: None,
            created_at: "2026-04-19T00:00:00.000Z".to_owned(),
            laws_verified: Some(true),
            object_hash: "sha256:deadbeef".to_owned(),
            round_trip_class: Some("isomorphism".to_owned()),
            source_schema: "sha256:aaa".to_owned(),
            target_schema: "sha256:bbb".to_owned(),
        }
    }

    #[tokio::test]
    async fn in_memory_resolver_hit() {
        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/l1").unwrap();
        let mut r = InMemoryResolver::new();
        r.insert(&uri, fixture_lens());

        let got = r.resolve(&uri).await.unwrap();
        assert_eq!(got.object_hash, "sha256:deadbeef");
    }

    #[tokio::test]
    async fn in_memory_resolver_miss() {
        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/missing").unwrap();
        let r = InMemoryResolver::new();

        let err = r.resolve(&uri).await.unwrap_err();
        assert!(matches!(err, LensError::NotFound(_)));
    }

    struct StaticPdsClient(serde_json::Value);

    impl PdsClient for StaticPdsClient {
        async fn get_record(
            &self,
            _did: &str,
            _collection: &str,
            _rkey: &str,
        ) -> Result<serde_json::Value, LensError> {
            Ok(self.0.clone())
        }
    }

    #[tokio::test]
    async fn pds_resolver_decodes_client_response() {
        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/l1").unwrap();
        let body = serde_json::to_value(fixture_lens()).unwrap();

        let r = PdsResolver::new(StaticPdsClient(body));
        let got = r.resolve(&uri).await.unwrap();

        assert_eq!(got.source_schema, "sha256:aaa");
    }

    #[tokio::test]
    async fn pds_resolver_surfaces_client_error() {
        struct FailingClient;
        impl PdsClient for FailingClient {
            async fn get_record(
                &self,
                _did: &str,
                _collection: &str,
                _rkey: &str,
            ) -> Result<serde_json::Value, LensError> {
                Err(LensError::Transport("boom".to_owned()))
            }
        }

        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/l1").unwrap();
        let r = PdsResolver::new(FailingClient);

        let err = r.resolve(&uri).await.unwrap_err();
        assert!(matches!(err, LensError::Transport(msg) if msg == "boom"));
    }

    #[tokio::test]
    async fn vcs_resolver_follows_ref_to_object() {
        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/head").unwrap();
        let lens = fixture_lens();

        let client = InMemoryVcsClient::new();
        client.insert_object(
            "sha256:obj1".to_owned(),
            serde_json::to_value(&lens).unwrap(),
        );

        let r = PanprotoVcsResolver::new(client);
        r.set_ref(&uri, "sha256:obj1").await.unwrap();

        let got = r.resolve(&uri).await.unwrap();
        assert_eq!(got.target_schema, "sha256:bbb");
    }

    #[tokio::test]
    async fn vcs_resolver_missing_ref() {
        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/unknown").unwrap();
        let r = PanprotoVcsResolver::new(InMemoryVcsClient::new());

        let err = r.resolve(&uri).await.unwrap_err();
        assert!(matches!(err, LensError::NotFound(_)));
    }

    #[tokio::test]
    async fn vcs_resolver_missing_object() {
        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/head").unwrap();
        let r = PanprotoVcsResolver::new(InMemoryVcsClient::new());
        r.set_ref(&uri, "sha256:absent").await.unwrap();

        let err = r.resolve(&uri).await.unwrap_err();
        assert!(matches!(err, LensError::NotFound(_)));
    }

    #[tokio::test]
    async fn in_memory_vcs_round_trips_refs_objects_commits() {
        let client = InMemoryVcsClient::new();
        client.insert_object(
            "sha256:c1".to_owned(),
            serde_json::json!({"kind": "commit", "n": 1}),
        );
        client.insert_object(
            "sha256:c2".to_owned(),
            serde_json::json!({"kind": "commit", "n": 2}),
        );
        client.insert_commit_chain(
            "main".to_owned(),
            vec!["sha256:c1".to_owned(), "sha256:c2".to_owned()],
        );

        assert_eq!(
            client.get_head("main").await.unwrap(),
            Some("sha256:c2".to_owned())
        );
        assert_eq!(
            client.list_commits("main", None).await.unwrap(),
            vec!["sha256:c1".to_owned(), "sha256:c2".to_owned()]
        );
        assert_eq!(
            client.list_commits("main", Some(1)).await.unwrap(),
            vec!["sha256:c1".to_owned()]
        );
        let head = client.get_object("sha256:c2").await.unwrap();
        assert_eq!(head["n"], 2);
    }

    #[tokio::test]
    async fn in_memory_vcs_registry_views_round_trip() {
        let client = InMemoryVcsClient::new();
        client.set_theories(vec!["t.first-order".to_owned()]);
        client.set_alignments(vec!["a.coq.peano".to_owned()]);
        client.insert_schema_tree("sha256:tree".to_owned(), serde_json::json!({"root": "S"}));

        assert_eq!(
            client.list_theories().await.unwrap(),
            vec!["t.first-order".to_owned()]
        );
        assert_eq!(
            client.list_alignments().await.unwrap(),
            vec!["a.coq.peano".to_owned()]
        );
        assert_eq!(
            client
                .get_schema_tree("sha256:tree")
                .await
                .unwrap()
                .get("root")
                .and_then(|v| v.as_str()),
            Some("S")
        );
    }
}
