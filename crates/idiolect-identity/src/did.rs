//! DID parsing and classification.
//!
//! idiolect only cares about two methods:
//!
//! - `did:plc:<base32>` — resolved via the PLC directory (plc.directory
//!   by convention, but the registry is pluggable).
//! - `did:web:<host>[:<path>]` — resolved via a `.well-known/did.json`
//!   fetch at the host.
//!
//! Other methods parse but return [`DidError::UnsupportedMethod`].

use std::fmt;

/// A parsed and validated atproto DID.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Did {
    /// The full DID string, e.g. `did:plc:abc123` or `did:web:example.com`.
    canonical: String,
    /// Method discriminator.
    method: DidMethod,
    /// The method-specific identifier (everything after `did:<method>:`).
    identifier: String,
}

/// Supported DID methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DidMethod {
    /// `did:plc:*` — atproto's primary method.
    Plc,
    /// `did:web:*` — DNS-rooted DIDs for self-hosted identities.
    Web,
}

/// Errors from DID parsing / resolution.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DidError {
    /// The input does not start with `did:`.
    #[error("not a DID: {0}")]
    NotADid(String),

    /// The DID is syntactically a DID, but the method is not supported
    /// by this crate. The method name is preserved for operator logs.
    #[error("unsupported DID method: {0}")]
    UnsupportedMethod(String),

    /// The method-specific identifier is empty or invalid.
    #[error("invalid DID identifier: {0}")]
    InvalidIdentifier(String),
}

impl Did {
    /// Parse and validate a DID string.
    ///
    /// # Errors
    ///
    /// Returns [`DidError::NotADid`] for strings that do not begin with
    /// `did:`, [`DidError::UnsupportedMethod`] for methods outside
    /// `plc` and `web`, and [`DidError::InvalidIdentifier`] when the
    /// method-specific identifier is empty.
    pub fn parse(raw: &str) -> Result<Self, DidError> {
        let rest = raw
            .strip_prefix("did:")
            .ok_or_else(|| DidError::NotADid(raw.to_owned()))?;
        let (method_str, identifier) = rest
            .split_once(':')
            .ok_or_else(|| DidError::InvalidIdentifier(raw.to_owned()))?;
        if identifier.is_empty() {
            return Err(DidError::InvalidIdentifier(raw.to_owned()));
        }
        let method = match method_str {
            "plc" => DidMethod::Plc,
            "web" => DidMethod::Web,
            other => return Err(DidError::UnsupportedMethod(other.to_owned())),
        };
        Ok(Self {
            canonical: raw.to_owned(),
            method,
            identifier: identifier.to_owned(),
        })
    }

    /// The full DID as a string (e.g. `did:plc:abc`).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// The method discriminator.
    #[must_use]
    pub const fn method(&self) -> DidMethod {
        self.method
    }

    /// The method-specific identifier (no `did:<method>:` prefix).
    #[must_use]
    pub fn identifier(&self) -> &str {
        &self.identifier
    }
}

impl fmt::Display for Did {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plc() {
        let d = Did::parse("did:plc:abc123").unwrap();
        assert_eq!(d.method(), DidMethod::Plc);
        assert_eq!(d.identifier(), "abc123");
        assert_eq!(d.as_str(), "did:plc:abc123");
    }

    #[test]
    fn parse_web() {
        let d = Did::parse("did:web:example.com").unwrap();
        assert_eq!(d.method(), DidMethod::Web);
        assert_eq!(d.identifier(), "example.com");
    }

    #[test]
    fn parse_web_with_path() {
        // did:web supports colons-as-path-separators: did:web:host:path.
        let d = Did::parse("did:web:example.com:users:alice").unwrap();
        assert_eq!(d.method(), DidMethod::Web);
        assert_eq!(d.identifier(), "example.com:users:alice");
    }

    #[test]
    fn reject_non_did() {
        assert!(matches!(
            Did::parse("https://example"),
            Err(DidError::NotADid(_))
        ));
    }

    #[test]
    fn reject_unsupported_method() {
        assert!(matches!(
            Did::parse("did:key:abc"),
            Err(DidError::UnsupportedMethod(_))
        ));
    }

    #[test]
    fn reject_empty_identifier() {
        assert!(matches!(
            Did::parse("did:plc:"),
            Err(DidError::InvalidIdentifier(_))
        ));
    }
}
