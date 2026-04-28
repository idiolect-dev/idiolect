//! Typed [`Language`](https://atproto.com/specs/lexicon#language).
//!
//! atproto's `format: "language"` is a [BCP 47] language tag —
//! `en`, `en-US`, `zh-Hant`, `mul`, `und`, `i-klingon`, etc. This
//! module enforces structural validity at parse time via the
//! [`language-tags`] crate so a value of type [`Language`] is
//! always a syntactically valid BCP 47 tag.
//!
//! Validity is structural, not semantic: the IANA registry
//! membership of subtags is not checked. `xx-YY` parses as a
//! well-formed primary-language + region pair even if no such
//! language exists. The wire form is preserved verbatim (no case
//! normalisation) so byte-for-byte fixture round-trips stay
//! stable across emissions.
//!
//! [BCP 47]: https://tools.ietf.org/html/bcp47

use std::fmt;
use std::str::FromStr;

use language_tags::LanguageTag;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A parsed and validated BCP 47 language tag.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Language {
    /// Canonical wire form, exactly as supplied to [`Language::parse`].
    /// Round-trips byte-for-byte through serde so consumers comparing
    /// against fixture json get stable equality. We avoid going
    /// through [`LanguageTag::canonicalize`] because it would re-case
    /// subtags (`en-us` -> `en-US`) and break drift gates against
    /// checked-in baselines.
    canonical: String,
}

/// Errors returned by [`Language::parse`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LanguageError {
    /// Input is empty.
    #[error("language tag is empty")]
    Empty,
    /// Input does not parse as a well-formed BCP 47 tag.
    #[error("invalid BCP 47 language tag {input:?}: {reason}")]
    Invalid {
        /// The original input for diagnostics.
        input: String,
        /// Why parsing failed.
        reason: String,
    },
}

impl Language {
    /// Parse and validate a BCP 47 language tag.
    ///
    /// Validity is purely structural: the tag must parse as a
    /// well-formed BCP 47 sequence (primary language, optional
    /// extlang, script, region, variants, extensions). The IANA
    /// registry is not consulted — `xx-YY` is structurally valid
    /// even if the subtags don't exist.
    ///
    /// # Errors
    ///
    /// [`LanguageError::Empty`] for an empty string,
    /// [`LanguageError::Invalid`] for any input the
    /// [`language-tags`] crate rejects.
    pub fn parse(input: impl Into<String>) -> Result<Self, LanguageError> {
        let canonical: String = input.into();
        if canonical.is_empty() {
            return Err(LanguageError::Empty);
        }
        LanguageTag::parse(&canonical).map_err(|e| LanguageError::Invalid {
            input: canonical.clone(),
            reason: e.to_string(),
        })?;
        Ok(Self { canonical })
    }

    /// The language tag as a string slice, in its original wire form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// Re-parse into a [`LanguageTag`] for primary-language /
    /// region / variant access. Re-parses each call; cheap because
    /// the input is already known-valid.
    ///
    /// # Errors
    ///
    /// Cannot fail in practice — the canonical string was validated
    /// at construction. The signature returns a `Result` to keep
    /// the API surface uniform with [`Datetime::to_offset_date_time`](crate::Datetime).
    pub fn to_language_tag(&self) -> Result<LanguageTag, language_tags::ParseError> {
        LanguageTag::parse(&self.canonical)
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical)
    }
}

impl FromStr for Language {
    type Err = LanguageError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl AsRef<str> for Language {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::ops::Deref for Language {
    type Target = str;

    fn deref(&self) -> &str {
        &self.canonical
    }
}

impl std::borrow::Borrow<str> for Language {
    fn borrow(&self) -> &str {
        &self.canonical
    }
}

impl Serialize for Language {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.canonical.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Language {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_primary_language_only() {
        let l = Language::parse("en").unwrap();
        assert_eq!(l.as_str(), "en");
    }

    #[test]
    fn parses_language_region() {
        let l = Language::parse("en-US").unwrap();
        assert_eq!(l.as_str(), "en-US");
    }

    #[test]
    fn parses_language_script_region() {
        let l = Language::parse("zh-Hant-TW").unwrap();
        assert_eq!(l.as_str(), "zh-Hant-TW");
    }

    #[test]
    fn parses_language_with_variant() {
        let l = Language::parse("de-DE-1996").unwrap();
        assert_eq!(l.as_str(), "de-DE-1996");
    }

    #[test]
    fn parses_grandfathered_tag() {
        // Grandfathered tags from RFC 5646 stay valid.
        let l = Language::parse("i-klingon").unwrap();
        assert_eq!(l.as_str(), "i-klingon");
    }

    #[test]
    fn preserves_input_case() {
        // BCP 47 is case-insensitive at the protocol level but we
        // preserve the wire bytes verbatim so fixtures round-trip.
        let l = Language::parse("en-us").unwrap();
        assert_eq!(l.as_str(), "en-us");
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(Language::parse(""), Err(LanguageError::Empty));
    }

    #[test]
    fn rejects_double_dash() {
        let result = Language::parse("en--US");
        assert!(matches!(result, Err(LanguageError::Invalid { .. })));
    }

    #[test]
    fn rejects_subtag_too_long() {
        // Primary language subtag is 2-8 characters per RFC 5646;
        // 9+ characters is structurally invalid.
        let result = Language::parse("abcdefghi");
        assert!(matches!(result, Err(LanguageError::Invalid { .. })));
    }

    #[test]
    fn rejects_leading_dash() {
        let result = Language::parse("-en");
        assert!(matches!(result, Err(LanguageError::Invalid { .. })));
    }

    #[test]
    fn round_trips_through_serde() {
        let l = Language::parse("zh-Hant-TW").unwrap();
        let json = serde_json::to_string(&l).unwrap();
        assert_eq!(json, "\"zh-Hant-TW\"");
        let back: Language = serde_json::from_str(&json).unwrap();
        assert_eq!(back, l);
    }

    #[test]
    fn from_str_works() {
        let l: Language = "en-GB".parse().unwrap();
        assert_eq!(l.as_str(), "en-GB");
    }

    #[test]
    fn to_language_tag_exposes_subtag_access() {
        let l = Language::parse("zh-Hant-TW").unwrap();
        let tag = l.to_language_tag().unwrap();
        assert_eq!(tag.primary_language(), "zh");
        assert_eq!(tag.script(), Some("Hant"));
        assert_eq!(tag.region(), Some("TW"));
    }
}
