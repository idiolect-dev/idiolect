//! Minimal at-uri parser for lens resolution.
//!
//! At-uris used by the lens runtime always point at a specific record:
//! `at://did:plc:xxx/dev.panproto.schema.lens/rkey`. This module only
//! needs to break that tuple apart — it deliberately does not
//! implement the full grammar from the atproto specs (query strings,
//! fragments, path-less forms). If a caller needs the richer grammar,
//! parse with an upstream library and pass the components to
//! [`AtUri::new`].

use std::fmt;

use crate::error::LensError;

/// An at-uri broken into its three resolver-relevant parts.
///
/// Constructed either by [`parse_at_uri`] (from a `&str`) or by
/// [`AtUri::new`] (from already-split components, e.g. when the caller
/// has done its own parsing).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AtUri {
    did: String,
    collection: String,
    rkey: String,
}

impl AtUri {
    /// Construct an at-uri from its components.
    ///
    /// No validation is performed: callers are responsible for ensuring
    /// `did` is a plc-form did, `collection` is an nsid, and `rkey` is
    /// a non-empty record key.
    #[must_use]
    pub const fn new(did: String, collection: String, rkey: String) -> Self {
        Self {
            did,
            collection,
            rkey,
        }
    }

    /// The repo did, e.g. `did:plc:xxxx`.
    #[must_use]
    pub fn did(&self) -> &str {
        &self.did
    }

    /// The collection nsid, e.g. `dev.panproto.schema.lens`.
    #[must_use]
    pub fn collection(&self) -> &str {
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

/// Parse an at-uri string of the form
/// `at://<did>/<collection>/<rkey>`.
///
/// # Errors
///
/// Returns [`LensError::InvalidUri`] when the input lacks the `at://`
/// prefix or does not contain exactly three slash-separated path
/// segments.
pub fn parse_at_uri(input: &str) -> Result<AtUri, LensError> {
    let body = input
        .strip_prefix("at://")
        .ok_or_else(|| LensError::InvalidUri(input.to_owned()))?;
    let mut parts = body.splitn(3, '/');
    let did = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| LensError::InvalidUri(input.to_owned()))?;
    let collection = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| LensError::InvalidUri(input.to_owned()))?;
    let rkey = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| LensError::InvalidUri(input.to_owned()))?;

    Ok(AtUri::new(
        did.to_owned(),
        collection.to_owned(),
        rkey.to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_plc_uri() {
        let uri = parse_at_uri("at://did:plc:xyz/dev.panproto.schema.lens/abc").unwrap();
        assert_eq!(uri.did(), "did:plc:xyz");
        assert_eq!(uri.collection(), "dev.panproto.schema.lens");
        assert_eq!(uri.rkey(), "abc");
    }

    #[test]
    fn rejects_non_at_scheme() {
        assert!(matches!(
            parse_at_uri("https://example/foo/bar/baz"),
            Err(LensError::InvalidUri(_))
        ));
    }

    #[test]
    fn rejects_missing_rkey() {
        assert!(matches!(
            parse_at_uri("at://did:plc:xyz/dev.panproto.schema.lens"),
            Err(LensError::InvalidUri(_))
        ));
    }

    #[test]
    fn rejects_empty_segments() {
        assert!(matches!(
            parse_at_uri("at://did:plc:xyz//abc"),
            Err(LensError::InvalidUri(_))
        ));
    }

    #[test]
    fn round_trips_via_display() {
        let s = "at://did:plc:xyz/dev.panproto.schema.lens/abc";
        let uri = parse_at_uri(s).unwrap();
        assert_eq!(uri.to_string(), s);
    }
}
