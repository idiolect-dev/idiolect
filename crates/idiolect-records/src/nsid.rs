//! Typed [`Nsid`](https://atproto.com/specs/nsid).
//!
//! Per the atproto NSID spec, an NSID has at least three segments
//! (reverse-DNS authority + camelCase name) drawn from a restricted
//! ASCII alphabet, and a maximum total length of 317 bytes. This
//! module enforces those rules at parse time and exposes structural
//! accessors (`authority()`, `name()`, `segments()`, `starts_with()`)
//! used throughout idiolect for routing, codegen, and dispatch.
//!
//! `Nsid` is a thin newtype around `String` plus a precomputed split
//! point between authority and name. It is `Eq + Hash + Clone +
//! Display + Serialize + Deserialize` and intended to be used by
//! value (not behind a reference) since it is small.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A parsed and validated atproto Namespaced Identifier.
///
/// Always conforms to <https://atproto.com/specs/nsid>: at least
/// three segments, ASCII only, total length ≤ 317 bytes, last
/// segment is camelCase (no hyphens, no leading digit, ≤ 63 chars).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Nsid {
    /// Canonical string form. Always equals `format!("{}.{}", authority, name)`.
    canonical: String,
    /// Byte index in `canonical` where the name segment starts (one
    /// past the final `.`). Authority is `&canonical[..authority_end]`
    /// where `authority_end == name_start - 1`.
    name_start: usize,
}

/// Errors returned by [`Nsid::parse`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NsidError {
    /// NSID is empty.
    #[error("NSID is empty")]
    Empty,
    /// NSID contains a non-ASCII byte.
    #[error("NSID contains non-ASCII characters: {0:?}")]
    NonAscii(String),
    /// NSID exceeds the 317-byte total limit.
    #[error("NSID exceeds 317 bytes: {actual} bytes")]
    TooLong {
        /// Actual length in bytes.
        actual: usize,
    },
    /// NSID has fewer than three segments.
    #[error("NSID needs at least 3 segments, got {got} ({input:?})")]
    TooFewSegments {
        /// Number of segments seen.
        got: usize,
        /// The original input for diagnostics.
        input: String,
    },
    /// An authority segment is invalid.
    #[error("invalid authority segment {segment:?} in {input:?}: {reason}")]
    InvalidAuthoritySegment {
        /// The offending segment.
        segment: String,
        /// The original input for diagnostics.
        input: String,
        /// Why it's invalid.
        reason: &'static str,
    },
    /// The name segment is invalid.
    #[error("invalid name segment {segment:?} in {input:?}: {reason}")]
    InvalidNameSegment {
        /// The offending segment.
        segment: String,
        /// The original input for diagnostics.
        input: String,
        /// Why it's invalid.
        reason: &'static str,
    },
}

const MAX_TOTAL_LEN: usize = 317;
const MAX_AUTHORITY_SEGMENT_LEN: usize = 63;
const MAX_NAME_LEN: usize = 63;

impl Nsid {
    /// Parse and validate an NSID. Stricter than the reference regex:
    /// also enforces the 317-byte total-length cap and the
    /// per-segment 63-byte cap.
    ///
    /// # Errors
    ///
    /// Returns [`NsidError`] when the input violates any of the
    /// rules in the atproto NSID spec.
    pub fn parse(input: impl Into<String>) -> Result<Self, NsidError> {
        let canonical: String = input.into();
        if canonical.is_empty() {
            return Err(NsidError::Empty);
        }
        if !canonical.is_ascii() {
            return Err(NsidError::NonAscii(canonical));
        }
        if canonical.len() > MAX_TOTAL_LEN {
            return Err(NsidError::TooLong {
                actual: canonical.len(),
            });
        }

        // Use byte iteration since the input is ASCII.
        let mut segment_start = 0usize;
        let mut segment_indices: Vec<(usize, usize)> = Vec::new();
        for (i, b) in canonical.bytes().enumerate() {
            if b == b'.' {
                if i == segment_start {
                    return Err(NsidError::InvalidAuthoritySegment {
                        segment: String::new(),
                        input: canonical.clone(),
                        reason: "empty segment",
                    });
                }
                segment_indices.push((segment_start, i));
                segment_start = i + 1;
            }
        }
        if segment_start == canonical.len() {
            return Err(NsidError::InvalidNameSegment {
                segment: String::new(),
                input: canonical.clone(),
                reason: "trailing dot",
            });
        }
        segment_indices.push((segment_start, canonical.len()));

        if segment_indices.len() < 3 {
            return Err(NsidError::TooFewSegments {
                got: segment_indices.len(),
                input: canonical,
            });
        }

        let last_idx = segment_indices.len() - 1;
        // Validate authority segments (everything except the last).
        for (idx, (start, end)) in segment_indices[..last_idx].iter().enumerate() {
            let seg = &canonical[*start..*end];
            validate_authority_segment(seg, &canonical, idx)?;
        }
        // Validate the name segment (last).
        let (name_start, name_end) = segment_indices[last_idx];
        validate_name_segment(&canonical[name_start..name_end], &canonical)?;

        Ok(Self {
            canonical,
            name_start,
        })
    }

    /// The full NSID as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// The reverse-DNS authority (everything before the final segment).
    #[must_use]
    pub fn authority(&self) -> &str {
        // SAFETY: `name_start` is always one past a `.` boundary.
        &self.canonical[..self.name_start - 1]
    }

    /// The final (name) segment.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.canonical[self.name_start..]
    }

    /// All segments in order, authority first then name.
    #[must_use]
    pub fn segments(&self) -> Vec<&str> {
        self.canonical.split('.').collect()
    }

    /// Whether this NSID is `prefix` itself or a descendant of it,
    /// matched on segment boundaries.
    ///
    /// `prefix` is a dotted string that does not need to be a full
    /// NSID — `dev`, `dev.idiolect`, and `dev.idiolect.encounter`
    /// are all valid prefixes. A trailing `.` is tolerated
    /// (`dev.idiolect.` and `dev.idiolect` behave identically). An
    /// empty prefix matches everything.
    ///
    /// Examples:
    /// - `dev.idiolect.encounter` matches prefix `dev`
    /// - `dev.idiolect.encounter` matches prefix `dev.idiolect`
    /// - `dev.idiolect.encounter` matches prefix `dev.idiolect.encounter`
    /// - `dev.idiolect.encounter` does NOT match prefix `dev.idio`
    ///   (no segment boundary at byte index 7)
    #[must_use]
    pub fn starts_with_authority(&self, prefix: &str) -> bool {
        let prefix = prefix.strip_suffix('.').unwrap_or(prefix);
        if prefix.is_empty() {
            return true;
        }
        if self.canonical == prefix {
            return true;
        }
        // Match on a `.` boundary so `dev.idio` doesn't match
        // `dev.idiolect.encounter`.
        self.canonical.starts_with(prefix)
            && self.canonical.as_bytes().get(prefix.len()) == Some(&b'.')
    }
}

fn validate_authority_segment(seg: &str, input: &str, idx: usize) -> Result<(), NsidError> {
    if seg.is_empty() {
        return Err(NsidError::InvalidAuthoritySegment {
            segment: seg.to_owned(),
            input: input.to_owned(),
            reason: "empty",
        });
    }
    if seg.len() > MAX_AUTHORITY_SEGMENT_LEN {
        return Err(NsidError::InvalidAuthoritySegment {
            segment: seg.to_owned(),
            input: input.to_owned(),
            reason: "segment exceeds 63 chars",
        });
    }
    let bytes = seg.as_bytes();
    if bytes[0] == b'-' || *bytes.last().unwrap() == b'-' {
        return Err(NsidError::InvalidAuthoritySegment {
            segment: seg.to_owned(),
            input: input.to_owned(),
            reason: "leading or trailing hyphen",
        });
    }
    if idx == 0 && bytes[0].is_ascii_digit() {
        return Err(NsidError::InvalidAuthoritySegment {
            segment: seg.to_owned(),
            input: input.to_owned(),
            reason: "first authority segment must not start with a digit",
        });
    }
    for &b in bytes {
        if !(b.is_ascii_alphanumeric() || b == b'-') {
            return Err(NsidError::InvalidAuthoritySegment {
                segment: seg.to_owned(),
                input: input.to_owned(),
                reason: "non-alphanumeric / non-hyphen byte",
            });
        }
    }
    Ok(())
}

fn validate_name_segment(seg: &str, input: &str) -> Result<(), NsidError> {
    if seg.is_empty() {
        return Err(NsidError::InvalidNameSegment {
            segment: seg.to_owned(),
            input: input.to_owned(),
            reason: "empty",
        });
    }
    if seg.len() > MAX_NAME_LEN {
        return Err(NsidError::InvalidNameSegment {
            segment: seg.to_owned(),
            input: input.to_owned(),
            reason: "name exceeds 63 chars",
        });
    }
    let bytes = seg.as_bytes();
    if !bytes[0].is_ascii_alphabetic() {
        return Err(NsidError::InvalidNameSegment {
            segment: seg.to_owned(),
            input: input.to_owned(),
            reason: "name must start with an ASCII letter",
        });
    }
    for &b in bytes {
        if !b.is_ascii_alphanumeric() {
            return Err(NsidError::InvalidNameSegment {
                segment: seg.to_owned(),
                input: input.to_owned(),
                reason: "name allows only ASCII letters and digits",
            });
        }
    }
    Ok(())
}

impl fmt::Display for Nsid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical)
    }
}

impl FromStr for Nsid {
    type Err = NsidError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl AsRef<str> for Nsid {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::ops::Deref for Nsid {
    type Target = str;

    fn deref(&self) -> &str {
        &self.canonical
    }
}

impl std::borrow::Borrow<str> for Nsid {
    fn borrow(&self) -> &str {
        &self.canonical
    }
}

impl Serialize for Nsid {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.canonical.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Nsid {
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
        let n = Nsid::parse("dev.idiolect.encounter").unwrap();
        assert_eq!(n.as_str(), "dev.idiolect.encounter");
        assert_eq!(n.authority(), "dev.idiolect");
        assert_eq!(n.name(), "encounter");
        assert_eq!(n.segments(), vec!["dev", "idiolect", "encounter"]);
    }

    #[test]
    fn parses_deeply_nested() {
        let n = Nsid::parse("dev.panproto.schema.lensAttestation").unwrap();
        assert_eq!(n.authority(), "dev.panproto.schema");
        assert_eq!(n.name(), "lensAttestation");
        assert_eq!(
            n.segments(),
            vec!["dev", "panproto", "schema", "lensAttestation"]
        );
    }

    #[test]
    fn rejects_too_few_segments() {
        assert!(matches!(
            Nsid::parse("dev.encounter"),
            Err(NsidError::TooFewSegments { got: 2, .. })
        ));
    }

    #[test]
    fn rejects_empty() {
        assert!(matches!(Nsid::parse(""), Err(NsidError::Empty)));
    }

    #[test]
    fn rejects_non_ascii() {
        assert!(matches!(
            Nsid::parse("dev.idiolect.café"),
            Err(NsidError::NonAscii(_))
        ));
    }

    #[test]
    fn rejects_too_long() {
        let long = format!("a.b.{}", "c".repeat(MAX_TOTAL_LEN));
        assert!(matches!(Nsid::parse(long), Err(NsidError::TooLong { .. })));
    }

    #[test]
    fn rejects_name_with_hyphen() {
        assert!(matches!(
            Nsid::parse("dev.idiolect.encounter-record"),
            Err(NsidError::InvalidNameSegment { .. })
        ));
    }

    #[test]
    fn rejects_name_with_leading_digit() {
        assert!(matches!(
            Nsid::parse("dev.idiolect.1encounter"),
            Err(NsidError::InvalidNameSegment { .. })
        ));
    }

    #[test]
    fn rejects_authority_segment_with_leading_hyphen() {
        assert!(matches!(
            Nsid::parse("dev.-bad.thing"),
            Err(NsidError::InvalidAuthoritySegment { .. })
        ));
    }

    #[test]
    fn rejects_authority_first_segment_starting_with_digit() {
        assert!(matches!(
            Nsid::parse("1foo.bar.thing"),
            Err(NsidError::InvalidAuthoritySegment { .. })
        ));
    }

    #[test]
    fn allows_authority_non_first_segment_starting_with_digit() {
        // The spec allows non-first authority segments to start with digits.
        let n = Nsid::parse("foo.4chan.thing").unwrap();
        assert_eq!(n.authority(), "foo.4chan");
    }

    #[test]
    fn rejects_double_dot() {
        assert!(Nsid::parse("dev..encounter").is_err());
    }

    #[test]
    fn starts_with_authority_segment_boundary() {
        let n = Nsid::parse("dev.idiolect.encounter").unwrap();
        assert!(n.starts_with_authority("dev"));
        assert!(n.starts_with_authority("dev.idiolect"));
        // The full NSID itself matches: a filter pinned to a single
        // collection should accept records of that exact collection.
        assert!(n.starts_with_authority("dev.idiolect.encounter"));
        // Sub-name matches without a segment boundary are rejected.
        assert!(!n.starts_with_authority("dev.idio"));
        // Trailing dot tolerated for legacy prefix strings.
        assert!(n.starts_with_authority("dev.idiolect."));
        // A prefix beyond the NSID's depth never matches.
        assert!(!n.starts_with_authority("dev.idiolect.encounter.deeper"));
        // Empty matches everything.
        assert!(n.starts_with_authority(""));
    }

    #[test]
    fn fromstr_works() {
        let n: Nsid = "dev.idiolect.encounter".parse().unwrap();
        assert_eq!(n.as_str(), "dev.idiolect.encounter");
    }

    #[test]
    fn serde_roundtrip() {
        let n = Nsid::parse("dev.idiolect.encounter").unwrap();
        let s = serde_json::to_string(&n).unwrap();
        assert_eq!(s, "\"dev.idiolect.encounter\"");
        let n2: Nsid = serde_json::from_str(&s).unwrap();
        assert_eq!(n, n2);
    }

    #[test]
    fn serde_rejects_invalid_on_deserialize() {
        let bad: Result<Nsid, _> = serde_json::from_str("\"too.few\"");
        assert!(bad.is_err());
    }
}
