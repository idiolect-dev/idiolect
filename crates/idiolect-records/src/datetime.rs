//! Typed [`Datetime`](https://atproto.com/specs/lexicon#datetime).
//!
//! atproto's `format: "datetime"` is RFC 3339. This module enforces
//! that at parse time so a value of type [`Datetime`] is always
//! parseable as a valid RFC 3339 timestamp. The canonical string
//! form is preserved on the wire (no normalisation), and a
//! `to_offset_date_time()` helper hands back a parsed
//! `time::OffsetDateTime` for arithmetic when callers need it.
//!
//! `Datetime` is a thin newtype over `String`. Codegen emits it for
//! every lexicon field whose declaration carries
//! `{"type": "string", "format": "datetime"}`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// A parsed and validated RFC 3339 datetime.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Datetime {
    /// Canonical wire form, exactly as supplied to `parse`. Round-
    /// trips byte-for-byte through serde so consumers comparing
    /// against fixture json get stable equality.
    canonical: String,
}

/// Errors returned by [`Datetime::parse`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DatetimeError {
    /// Input is empty.
    #[error("datetime is empty")]
    Empty,
    /// Input does not parse as RFC 3339.
    #[error("invalid RFC 3339 datetime {input:?}: {reason}")]
    InvalidRfc3339 {
        /// The original input for diagnostics.
        input: String,
        /// Why parsing failed.
        reason: String,
    },
}

impl Datetime {
    /// Parse and validate an RFC 3339 datetime.
    ///
    /// # Errors
    ///
    /// [`DatetimeError::Empty`] for an empty string,
    /// [`DatetimeError::InvalidRfc3339`] for any RFC 3339 parse
    /// failure (timezone missing, malformed offset, etc.).
    pub fn parse(input: impl Into<String>) -> Result<Self, DatetimeError> {
        let canonical: String = input.into();
        if canonical.is_empty() {
            return Err(DatetimeError::Empty);
        }
        OffsetDateTime::parse(&canonical, &Rfc3339).map_err(|e| DatetimeError::InvalidRfc3339 {
            input: canonical.clone(),
            reason: e.to_string(),
        })?;
        Ok(Self { canonical })
    }

    /// The full datetime as a string slice, in its original wire form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// Parse the canonical form into a `time::OffsetDateTime` for
    /// arithmetic. Re-parses each call; cache the result if you need
    /// it hot.
    ///
    /// # Errors
    ///
    /// Cannot fail in practice — the canonical string was validated
    /// at construction. The signature returns a `Result` for the
    /// rare case where `time` rejects a string it accepted earlier
    /// after a crate upgrade.
    pub fn to_offset_date_time(&self) -> Result<OffsetDateTime, time::error::Parse> {
        OffsetDateTime::parse(&self.canonical, &Rfc3339)
    }
}

impl fmt::Display for Datetime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical)
    }
}

impl FromStr for Datetime {
    type Err = DatetimeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl AsRef<str> for Datetime {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::ops::Deref for Datetime {
    type Target = str;

    fn deref(&self) -> &str {
        &self.canonical
    }
}

impl std::borrow::Borrow<str> for Datetime {
    fn borrow(&self) -> &str {
        &self.canonical
    }
}

impl Serialize for Datetime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.canonical.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Datetime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical() {
        let d = Datetime::parse("2026-04-26T00:00:00.000Z").unwrap();
        assert_eq!(d.as_str(), "2026-04-26T00:00:00.000Z");
    }

    #[test]
    fn parses_with_offset() {
        let d = Datetime::parse("2026-04-26T00:00:00-05:00").unwrap();
        let odt = d.to_offset_date_time().unwrap();
        assert_eq!(odt.year(), 2026);
    }

    #[test]
    fn rejects_empty() {
        assert!(matches!(Datetime::parse(""), Err(DatetimeError::Empty)));
    }

    #[test]
    fn rejects_naive_local_time() {
        // RFC 3339 requires a timezone offset.
        assert!(matches!(
            Datetime::parse("2026-04-26T00:00:00"),
            Err(DatetimeError::InvalidRfc3339 { .. })
        ));
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!(
            Datetime::parse("not a datetime"),
            Err(DatetimeError::InvalidRfc3339 { .. })
        ));
    }

    #[test]
    fn fromstr_works() {
        let d: Datetime = "2026-04-26T00:00:00Z".parse().unwrap();
        assert_eq!(d.as_str(), "2026-04-26T00:00:00Z");
    }

    #[test]
    fn serde_roundtrip_preserves_wire_form() {
        let original = "2026-04-26T00:00:00.500-05:00";
        let d = Datetime::parse(original).unwrap();
        let s = serde_json::to_string(&d).unwrap();
        assert_eq!(s, format!("\"{original}\""));
        let d2: Datetime = serde_json::from_str(&s).unwrap();
        assert_eq!(d, d2);
    }

    #[test]
    fn serde_rejects_invalid_on_deserialize() {
        let bad: Result<Datetime, _> = serde_json::from_str("\"not-a-date\"");
        assert!(bad.is_err());
    }
}
