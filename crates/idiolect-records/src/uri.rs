//! Typed [`Uri`](https://atproto.com/specs/lexicon#uri).
//!
//! atproto's `format: "uri"` is permissive — any valid URI per
//! RFC 3986 — and is used for `policyUri`, `MaterialSpec.uri`,
//! generator pointers, and other generic outbound references that
//! aren't atproto-shaped enough to be `at-uri`. This module is the
//! parse-once-validated newtype the codegen emits for those
//! fields.
//!
//! Validation goes through [`url::Url::parse`], which accepts any
//! IETF URI with a scheme + scheme-specific part — `https://`,
//! `http://`, `mailto:`, `urn:`, `file://`, and so on. The `url`
//! crate is stricter than RFC 3986 about authority shape, in
//! particular it rejects atproto's `at://did:plc:.../...` form
//! because the DID's colons read as malformed ports. That's fine:
//! a lexicon that needs an at-uri declares `format: "at-uri"` and
//! gets the dedicated [`AtUri`](crate::AtUri) typed wrapper
//! instead. `Uri` is for the residual category of generic outbound
//! references where the wire-form happens to be a URI but isn't
//! atproto-shaped.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use url::Url;

/// A parsed and validated generic URI.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Uri {
    /// Canonical wire form. We preserve the input verbatim rather
    /// than re-rendering through `Url::to_string()` because the
    /// re-rendered form can normalise (drop default ports, percent-
    /// encode authority idn, etc.) and that would break byte-for-
    /// byte fixture round-trips.
    canonical: String,
}

/// Errors returned by [`Uri::parse`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UriError {
    /// Input is empty.
    #[error("URI is empty")]
    Empty,
    /// Input does not parse as an RFC 3986 URI.
    #[error("invalid URI {input:?}: {reason}")]
    Invalid {
        /// The original input for diagnostics.
        input: String,
        /// Why parsing failed.
        reason: String,
    },
}

impl Uri {
    /// Parse and validate a URI.
    ///
    /// # Errors
    ///
    /// [`UriError::Empty`] for an empty string,
    /// [`UriError::Invalid`] for any input that `url::Url::parse`
    /// rejects (missing scheme, bad authority, etc.).
    pub fn parse(input: impl Into<String>) -> Result<Self, UriError> {
        let canonical: String = input.into();
        if canonical.is_empty() {
            return Err(UriError::Empty);
        }
        Url::parse(&canonical).map_err(|e| UriError::Invalid {
            input: canonical.clone(),
            reason: e.to_string(),
        })?;
        Ok(Self { canonical })
    }

    /// The full URI as a string slice, in its original wire form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// Re-parse into a `url::Url` for scheme / host / path access.
    /// Re-parses each call.
    ///
    /// # Errors
    ///
    /// Cannot fail in practice — the canonical string was validated
    /// at construction. The signature returns a `Result` to keep
    /// the API surface uniform with [`Datetime::to_offset_date_time`](crate::Datetime).
    pub fn to_url(&self) -> Result<Url, url::ParseError> {
        Url::parse(&self.canonical)
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical)
    }
}

impl FromStr for Uri {
    type Err = UriError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl AsRef<str> for Uri {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::ops::Deref for Uri {
    type Target = str;

    fn deref(&self) -> &str {
        &self.canonical
    }
}

impl std::borrow::Borrow<str> for Uri {
    fn borrow(&self) -> &str {
        &self.canonical
    }
}

impl Serialize for Uri {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.canonical.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Uri {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_https() {
        let u = Uri::parse("https://example.com/path?q=1").unwrap();
        assert_eq!(u.as_str(), "https://example.com/path?q=1");
        assert_eq!(u.to_url().unwrap().host_str(), Some("example.com"));
    }

    #[test]
    fn parses_mailto() {
        let u = Uri::parse("mailto:alice@example.com").unwrap();
        assert_eq!(u.to_url().unwrap().scheme(), "mailto");
    }

    #[test]
    fn rejects_empty() {
        assert!(matches!(Uri::parse(""), Err(UriError::Empty)));
    }

    #[test]
    fn rejects_no_scheme() {
        assert!(matches!(
            Uri::parse("just a string"),
            Err(UriError::Invalid { .. })
        ));
    }

    #[test]
    fn fromstr_works() {
        let u: Uri = "https://example.com".parse().unwrap();
        assert_eq!(u.as_str(), "https://example.com");
    }

    #[test]
    fn serde_roundtrip_preserves_wire_form() {
        let original = "https://example.com/a/b?c=d";
        let u = Uri::parse(original).unwrap();
        let s = serde_json::to_string(&u).unwrap();
        assert_eq!(s, format!("\"{original}\""));
        let u2: Uri = serde_json::from_str(&s).unwrap();
        assert_eq!(u, u2);
    }

    #[test]
    fn serde_rejects_invalid_on_deserialize() {
        let bad: Result<Uri, _> = serde_json::from_str("\"not a uri\"");
        assert!(bad.is_err());
    }
}
