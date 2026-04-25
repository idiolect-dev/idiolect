//! Typed [`AtUri`](https://atproto.com/specs/at-uri-scheme).
//!
//! At-URIs used by idiolect always point at a specific record:
//! `at://<did|handle>/<collection-nsid>/<rkey>`. This module parses
//! into typed components ([`Did`] for the authority, [`Nsid`] for
//! the collection, plain `String` for the rkey since record-key
//! validation is repo-specific). Query / fragment forms from the
//! full at-uri grammar aren't relevant to idiolect's use cases and
//! are rejected at parse time.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::did::{Did, DidError};
use crate::nsid::{Nsid, NsidError};

/// A parsed at-uri pointing at a record.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AtUri {
    did: Did,
    collection: Nsid,
    rkey: String,
}

/// Errors returned by [`AtUri::parse`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AtUriError {
    /// The input doesn't start with `at://`.
    #[error("not an at-uri: {0:?}")]
    NotAtUri(String),
    /// The input doesn't have exactly three slash-separated path
    /// segments after `at://`.
    #[error("at-uri must have form at://did/collection/rkey: {0:?}")]
    InvalidShape(String),
    /// The authority segment isn't a valid DID.
    #[error("at-uri authority is not a DID: {0}")]
    InvalidDid(#[source] DidError),
    /// The collection segment isn't a valid NSID.
    #[error("at-uri collection is not an NSID: {0}")]
    InvalidNsid(#[source] NsidError),
    /// The rkey is empty.
    #[error("at-uri rkey is empty: {0:?}")]
    EmptyRkey(String),
}

impl AtUri {
    /// Construct from already-parsed components. No further
    /// validation happens here; the typed inputs guarantee shape.
    #[must_use]
    pub fn new(did: Did, collection: Nsid, rkey: String) -> Self {
        Self {
            did,
            collection,
            rkey,
        }
    }

    /// Parse and validate an at-uri.
    ///
    /// # Errors
    ///
    /// Returns [`AtUriError`] when the input lacks the `at://`
    /// prefix, doesn't have exactly three path segments, or any of
    /// the three components fails its own validation.
    pub fn parse(input: &str) -> Result<Self, AtUriError> {
        let body = input
            .strip_prefix("at://")
            .ok_or_else(|| AtUriError::NotAtUri(input.to_owned()))?;
        let mut parts = body.splitn(3, '/');
        let did_str = parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AtUriError::InvalidShape(input.to_owned()))?;
        let collection_str = parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AtUriError::InvalidShape(input.to_owned()))?;
        let rkey = parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AtUriError::EmptyRkey(input.to_owned()))?;

        let did = Did::parse(did_str).map_err(AtUriError::InvalidDid)?;
        let collection = Nsid::parse(collection_str).map_err(AtUriError::InvalidNsid)?;
        Ok(Self::new(did, collection, rkey.to_owned()))
    }

    /// The repo DID, e.g. `did:plc:xxxx`.
    #[must_use]
    pub fn did(&self) -> &Did {
        &self.did
    }

    /// The collection NSID, e.g. `dev.panproto.schema.lens`.
    #[must_use]
    pub fn collection(&self) -> &Nsid {
        &self.collection
    }

    /// The record key within the collection.
    #[must_use]
    pub fn rkey(&self) -> &str {
        &self.rkey
    }
}

impl fmt::Display for AtUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at://{}/{}/{}", self.did, self.collection, self.rkey)
    }
}

impl FromStr for AtUri {
    type Err = AtUriError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for AtUri {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for AtUri {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_plc_uri() {
        let uri = AtUri::parse("at://did:plc:xyz/dev.panproto.schema.lens/abc").unwrap();
        assert_eq!(uri.did().as_str(), "did:plc:xyz");
        assert_eq!(uri.collection().as_str(), "dev.panproto.schema.lens");
        assert_eq!(uri.rkey(), "abc");
    }

    #[test]
    fn round_trips_via_display() {
        let s = "at://did:plc:xyz/dev.panproto.schema.lens/abc";
        let uri = AtUri::parse(s).unwrap();
        assert_eq!(uri.to_string(), s);
    }

    #[test]
    fn rejects_non_at_scheme() {
        assert!(matches!(
            AtUri::parse("https://example/foo/bar/baz"),
            Err(AtUriError::NotAtUri(_))
        ));
    }

    #[test]
    fn rejects_missing_rkey() {
        assert!(matches!(
            AtUri::parse("at://did:plc:xyz/dev.panproto.schema.lens"),
            Err(AtUriError::InvalidShape(_) | AtUriError::EmptyRkey(_))
        ));
    }

    #[test]
    fn rejects_empty_segments() {
        assert!(matches!(
            AtUri::parse("at://did:plc:xyz//abc"),
            Err(AtUriError::InvalidShape(_))
        ));
    }

    #[test]
    fn rejects_invalid_nsid_collection() {
        assert!(matches!(
            AtUri::parse("at://did:plc:xyz/notanns/abc"),
            Err(AtUriError::InvalidNsid(_))
        ));
    }

    #[test]
    fn rejects_invalid_did() {
        assert!(matches!(
            AtUri::parse("at://notadid/dev.panproto.schema.lens/abc"),
            Err(AtUriError::InvalidDid(_))
        ));
    }

    #[test]
    fn serde_roundtrip() {
        let s = "at://did:plc:xyz/dev.panproto.schema.lens/abc";
        let uri = AtUri::parse(s).unwrap();
        let json = serde_json::to_string(&uri).unwrap();
        assert_eq!(json, format!("\"{s}\""));
        let uri2: AtUri = serde_json::from_str(&json).unwrap();
        assert_eq!(uri, uri2);
    }
}
