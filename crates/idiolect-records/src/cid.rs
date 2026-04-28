//! Typed [`Cid`](https://atproto.com/specs/lexicon#cid-link).
//!
//! atproto's `cid-link` wire shape is `{"$link": "<cid-string>"}`,
//! where the inner string is a base32-encoded multibase CID
//! (typically `CIDv1` with the `dag-cbor` codec). This module is the
//! parse-once-validated newtype the codegen emits for those fields.
//!
//! Validation goes through [`cid::Cid::try_from`] which decodes the
//! multibase string and checks the multihash. The canonical wire
//! form is preserved verbatim so byte-for-byte fixture round-trips
//! stay stable: re-rendering a CID through `to_string()` can switch
//! base or normalise the multihash, and that would break drift
//! gates against checked-in baselines.
//!
//! # Wire form
//!
//! On the wire, an atproto `cid-link` is an object — `{"$link": "..."}`.
//! The codegen translates that to a Rust field of type [`Cid`] whose
//! serde shape is the wrapped string itself. The
//! `{"$link": "..."}` envelope is added by an outer serde adapter at
//! the field boundary; this newtype only owns the string payload.

use std::fmt;
use std::str::FromStr;

use cid::Cid as MultiformatsCid;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A parsed and validated atproto `cid-link` payload.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Cid {
    /// Canonical wire form, preserved exactly as supplied to
    /// [`Cid::parse`]. We avoid re-rendering through
    /// [`MultiformatsCid::to_string`] because the canonical form
    /// can switch multibase representation (base32 vs base58btc),
    /// and that would break byte-for-byte round-trips against
    /// fixture json.
    canonical: String,
}

/// Errors returned by [`Cid::parse`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CidError {
    /// Input is empty.
    #[error("CID is empty")]
    Empty,
    /// Input does not parse as a valid CID. The underlying error
    /// from [`cid::Error`] is not exposed because that type is not
    /// `PartialEq`; we lose its discriminant in exchange for a
    /// stable comparable error type.
    #[error("invalid CID {input:?}: {reason}")]
    Invalid {
        /// The original input for diagnostics.
        input: String,
        /// Why parsing failed.
        reason: String,
    },
}

impl Cid {
    /// Parse and validate an atproto `cid-link` payload.
    ///
    /// Accepts any multibase-encoded CID the [`cid`] crate can
    /// decode (`CIDv0` base58btc legacy form, `CIDv1` base32,
    /// `CIDv1` base58, etc.). The parsed CID is discarded; only the
    /// input string is retained.
    ///
    /// # Errors
    ///
    /// [`CidError::Empty`] for an empty string,
    /// [`CidError::Invalid`] for any input the `cid` crate rejects.
    pub fn parse(input: impl Into<String>) -> Result<Self, CidError> {
        let canonical: String = input.into();
        if canonical.is_empty() {
            return Err(CidError::Empty);
        }
        MultiformatsCid::try_from(canonical.as_str()).map_err(|e| CidError::Invalid {
            input: canonical.clone(),
            reason: e.to_string(),
        })?;
        Ok(Self { canonical })
    }

    /// The CID as a string slice, in its original wire form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// Re-parse into a [`MultiformatsCid`] for codec / multihash /
    /// version access. Re-parses each call; cheap because the input
    /// is already known-valid.
    ///
    /// # Errors
    ///
    /// Cannot fail in practice — the canonical string was validated
    /// at construction. The signature returns a `Result` to keep the
    /// API surface uniform with [`Datetime::to_offset_date_time`](crate::Datetime).
    pub fn to_multiformats(&self) -> Result<MultiformatsCid, cid::Error> {
        MultiformatsCid::try_from(self.canonical.as_str())
    }
}

impl fmt::Display for Cid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical)
    }
}

impl FromStr for Cid {
    type Err = CidError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl AsRef<str> for Cid {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::ops::Deref for Cid {
    type Target = str;

    fn deref(&self) -> &str {
        &self.canonical
    }
}

impl std::borrow::Borrow<str> for Cid {
    fn borrow(&self) -> &str {
        &self.canonical
    }
}

impl Serialize for Cid {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.canonical.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Cid {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // CIDv1 base32 of an empty `dag-cbor` block — a real CID
    // produced by `cid::Cid::new_v1(0x71, multihash::Code::Sha2_256.digest(b""))`.
    const VALID_CID_V1_B32: &str =
        "bafyreidkacrnonh3pkjnntp7ujnkfeudaodpqzxa67mtbmovd5sayuctfm";

    // Legacy CIDv0 base58btc form — `Qm...` prefix.
    const VALID_CID_V0_B58: &str = "QmPZ9gcCEpqKTo6aq61g2nXGUhM4iCL3ewB6LDXZCtioEB";

    #[test]
    fn parses_v1_base32() {
        let c = Cid::parse(VALID_CID_V1_B32).unwrap();
        assert_eq!(c.as_str(), VALID_CID_V1_B32);
    }

    #[test]
    fn parses_v0_base58() {
        let c = Cid::parse(VALID_CID_V0_B58).unwrap();
        assert_eq!(c.as_str(), VALID_CID_V0_B58);
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(Cid::parse(""), Err(CidError::Empty));
    }

    #[test]
    fn rejects_garbage() {
        let result = Cid::parse("not a cid");
        assert!(matches!(result, Err(CidError::Invalid { .. })));
    }

    #[test]
    fn round_trips_canonical_form() {
        let c = Cid::parse(VALID_CID_V1_B32).unwrap();
        let json = serde_json::to_string(&c).unwrap();
        // The serialized form is a JSON string carrying the
        // canonical bytes verbatim.
        assert_eq!(json, format!("\"{VALID_CID_V1_B32}\""));
        let back: Cid = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn from_str_works() {
        let c: Cid = VALID_CID_V1_B32.parse().unwrap();
        assert_eq!(c.as_str(), VALID_CID_V1_B32);
    }

    #[test]
    fn to_multiformats_round_trips_codec() {
        let c = Cid::parse(VALID_CID_V1_B32).unwrap();
        let mf = c.to_multiformats().unwrap();
        // The dag-cbor codec is `0x71`.
        assert_eq!(mf.codec(), 0x71);
    }
}
