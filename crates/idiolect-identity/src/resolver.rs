//! The [`IdentityResolver`] trait + reference impls.
//!
//! One async method, `resolve(did) -> DidDocument`, plus the usual
//! `Send + Sync` bounds. Callers that need a handle or a PDS URL walk
//! the returned [`DidDocument`] themselves; a second crate-level
//! helper [`IdentityResolver::resolve_pds_url`] is the common case.

use crate::did::Did;
use crate::document::DidDocument;
use crate::error::IdentityError;

/// Async DID-to-document resolver.
///
/// Every backend (in-memory, plc.directory over reqwest, a caching
/// proxy, an authenticated entryway client, …) implements this one
/// surface.
#[allow(async_fn_in_trait)]
pub trait IdentityResolver: Send + Sync {
    /// Resolve a DID into its document.
    ///
    /// # Errors
    ///
    /// Returns [`IdentityError::NotFound`] when no identity matches,
    /// [`IdentityError::Transport`] for transport-level failures, and
    /// [`IdentityError::InvalidDocument`] when the backend returns a
    /// document that fails to parse.
    async fn resolve(&self, did: &Did) -> Result<DidDocument, IdentityError>;

    /// Resolve a DID and extract the PDS service URL. Convenience
    /// wrapper around [`resolve`](Self::resolve) + [`DidDocument::pds_url`].
    ///
    /// # Errors
    ///
    /// In addition to [`resolve`](Self::resolve)'s errors, returns
    /// [`IdentityError::NoPdsEndpoint`] when the document has no
    /// `#atproto_pds` entry.
    async fn resolve_pds_url(&self, did: &Did) -> Result<String, IdentityError> {
        let doc = self.resolve(did).await?;
        doc.pds_url()
            .map(str::to_owned)
            .ok_or_else(|| IdentityError::NoPdsEndpoint(did.to_string()))
    }

    /// Resolve a DID and extract its primary handle (the first
    /// `alsoKnownAs` entry under the `at://` scheme, stripped of the
    /// prefix).
    ///
    /// # Errors
    ///
    /// In addition to [`resolve`](Self::resolve)'s errors, returns
    /// [`IdentityError::NoHandle`] when the document declares no
    /// handle.
    async fn resolve_handle(&self, did: &Did) -> Result<String, IdentityError> {
        let doc = self.resolve(did).await?;
        doc.handle()
            .map(str::to_owned)
            .ok_or_else(|| IdentityError::NoHandle(did.to_string()))
    }
}

/// In-memory resolver. `HashMap` of DID → document, populated by the
/// caller via [`InMemoryIdentityResolver::insert`].
#[derive(Debug, Default, Clone)]
pub struct InMemoryIdentityResolver {
    entries: std::collections::HashMap<String, DidDocument>,
}

impl InMemoryIdentityResolver {
    /// Construct an empty resolver.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace the document for `did`.
    pub fn insert(&mut self, did: &Did, doc: DidDocument) {
        self.entries.insert(did.as_str().to_owned(), doc);
    }

    /// Number of DIDs currently stored.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the resolver has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl IdentityResolver for InMemoryIdentityResolver {
    async fn resolve(&self, did: &Did) -> Result<DidDocument, IdentityError> {
        self.entries
            .get(did.as_str())
            .cloned()
            .ok_or_else(|| IdentityError::NotFound(did.to_string()))
    }
}

/// Forward [`IdentityResolver`] through a shared `Arc<T>`. Lets a
/// single resolver (e.g. a cached `ReqwestIdentityResolver`) be shared
/// across an axum router, a firehose handler, and a session-refresh
/// task without cloning the underlying http client or cache state.
impl<T: IdentityResolver + ?Sized> IdentityResolver for std::sync::Arc<T> {
    async fn resolve(&self, did: &Did) -> Result<DidDocument, IdentityError> {
        (**self).resolve(did).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(id: &str) -> DidDocument {
        DidDocument {
            id: id.to_owned(),
            also_known_as: vec!["at://alice.test".to_owned()],
            service: vec![crate::document::Service {
                id: "#atproto_pds".to_owned(),
                service_type: "AtprotoPersonalDataServer".to_owned(),
                service_endpoint: "https://pds.example".to_owned(),
            }],
            verification_method: vec![],
            extras: serde_json::Map::default(),
        }
    }

    #[tokio::test]
    async fn in_memory_hit() {
        let did = Did::parse("did:plc:alice").unwrap();
        let mut r = InMemoryIdentityResolver::new();
        r.insert(&did, doc("did:plc:alice"));
        let got = r.resolve(&did).await.unwrap();
        assert_eq!(got.id, "did:plc:alice");
    }

    #[tokio::test]
    async fn in_memory_miss() {
        let did = Did::parse("did:plc:absent").unwrap();
        let r = InMemoryIdentityResolver::new();
        let err = r.resolve(&did).await.unwrap_err();
        assert!(matches!(err, IdentityError::NotFound(_)));
    }

    #[tokio::test]
    async fn resolve_pds_url_via_trait_default() {
        let did = Did::parse("did:plc:alice").unwrap();
        let mut r = InMemoryIdentityResolver::new();
        r.insert(&did, doc("did:plc:alice"));
        assert_eq!(r.resolve_pds_url(&did).await.unwrap(), "https://pds.example");
    }

    #[tokio::test]
    async fn resolve_handle_returns_first_at_uri() {
        let did = Did::parse("did:plc:alice").unwrap();
        let mut r = InMemoryIdentityResolver::new();
        r.insert(&did, doc("did:plc:alice"));
        assert_eq!(r.resolve_handle(&did).await.unwrap(), "alice.test");
    }

    #[tokio::test]
    async fn resolve_handle_errors_when_absent() {
        let did = Did::parse("did:plc:alice").unwrap();
        let mut r = InMemoryIdentityResolver::new();
        let mut d = doc("did:plc:alice");
        d.also_known_as.clear();
        r.insert(&did, d);
        let err = r.resolve_handle(&did).await.unwrap_err();
        assert!(matches!(err, IdentityError::NoHandle(_)));
    }

    #[tokio::test]
    async fn resolve_pds_url_error_when_service_missing() {
        let did = Did::parse("did:plc:alice").unwrap();
        let mut r = InMemoryIdentityResolver::new();
        let mut d = doc("did:plc:alice");
        d.service.clear();
        r.insert(&did, d);
        let err = r.resolve_pds_url(&did).await.unwrap_err();
        assert!(matches!(err, IdentityError::NoPdsEndpoint(_)));
    }
}
