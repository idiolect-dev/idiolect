//! The `Record` trait — the single abstraction appview code uses to
//! treat every `dev.idiolect.*` record type uniformly.
//!
//! Every record type in [`crate::generated`] implements `Record` via
//! a blanket impl emitted by `idiolect-codegen`. Appview indexers,
//! xrpc handlers, and test fixtures can then be generic over `R:
//! Record` instead of matching on nsids by hand.
//!
//! The companion [`AnyRecord`] enum is the dynamic counterpart: it
//! discriminates by nsid on the wire, so callers that receive
//! untyped json from the firehose or from an xrpc request can decode
//! once into the right variant.

use serde::{Serialize, Serializer, de::DeserializeOwned};

/// Every `dev.idiolect.*` record type implements this trait.
///
/// The generated `impl` block supplies the [`NSID`][Record::NSID]
/// constant and constrains the associated serde bounds. Appview code
/// written against `Record` works for any new record type added to
/// the lexicon family without touching call sites.
///
/// # Examples
///
/// ```
/// use idiolect_records::{Encounter, Record};
/// assert_eq!(Encounter::NSID, "dev.idiolect.encounter");
/// ```
pub trait Record: Serialize + DeserializeOwned + Clone + std::fmt::Debug + 'static {
    /// Fully-qualified lexicon nsid, e.g. `"dev.idiolect.encounter"`.
    const NSID: &'static str;

    /// Human-readable short name of the record kind, e.g.
    /// `"encounter"`. Derived from the nsid's last segment.
    #[must_use]
    fn kind() -> &'static str {
        let n = Self::NSID;
        n.rsplit('.').next().unwrap_or(n)
    }
}

/// Discriminated-union view over every record type in the family.
///
/// Produced by [`decode_record`] when an appview receives json whose
/// nsid is only known at runtime (e.g. firehose traffic). Each variant
/// carries the strongly-typed record; pattern-match to dispatch.
// `Clone` on a variant-heavy enum surfaces clippy::large_enum_variant
// once one variant grows meaningfully larger than its siblings. The
// tag-dispatch shape is the whole reason this type exists; pay the
// allocation cost elsewhere if the allocation cost matters.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum AnyRecord {
    /// A `dev.idiolect.community` record.
    Community(crate::Community),
    /// A `dev.idiolect.dialect` record.
    Dialect(crate::Dialect),
    /// A `dev.idiolect.encounter` record.
    Encounter(crate::Encounter),
    /// A `dev.idiolect.correction` record.
    Correction(crate::Correction),
    /// A `dev.idiolect.verification` record.
    Verification(crate::Verification),
    /// A `dev.idiolect.observation` record.
    Observation(crate::Observation),
    /// A `dev.idiolect.retrospection` record.
    Retrospection(crate::Retrospection),
    /// A `dev.idiolect.recommendation` record.
    Recommendation(crate::Recommendation),
    /// A `dev.idiolect.adapter` record.
    Adapter(crate::Adapter),
    /// A `dev.idiolect.bounty` record.
    Bounty(crate::Bounty),
    /// A `dev.idiolect.belief` record — a second-order doxastic
    /// claim about another record, generic over the subject kind.
    Belief(crate::Belief),
}

impl AnyRecord {
    /// Nsid of the contained record.
    #[must_use]
    pub const fn nsid(&self) -> &'static str {
        match self {
            Self::Community(_) => crate::Community::NSID,
            Self::Dialect(_) => crate::Dialect::NSID,
            Self::Encounter(_) => crate::Encounter::NSID,
            Self::Correction(_) => crate::Correction::NSID,
            Self::Verification(_) => crate::Verification::NSID,
            Self::Observation(_) => crate::Observation::NSID,
            Self::Retrospection(_) => crate::Retrospection::NSID,
            Self::Recommendation(_) => crate::Recommendation::NSID,
            Self::Adapter(_) => crate::Adapter::NSID,
            Self::Bounty(_) => crate::Bounty::NSID,
            Self::Belief(_) => crate::Belief::NSID,
        }
    }

    /// Serialize the inner record into a [`serde_json::Value`] and
    /// splice a `$type` field corresponding to [`Self::nsid`].
    ///
    /// This is the standard wire form atproto records take when
    /// serialized into a `com.atproto.repo.*` xrpc request or a
    /// firehose frame — the union discriminator lives inside the
    /// object next to the fields. Prefer [`Serialize`] directly when
    /// the enum-tagged form is acceptable; this helper is for writes
    /// to a PDS where the PDS inspects `$type`.
    ///
    /// # Errors
    ///
    /// Returns [`serde_json::Error`] when the inner record fails to
    /// serialize, or when its serialized form is not a json object
    /// (which never happens for generated record types).
    pub fn to_typed_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        let mut value = self.inner_to_json()?;
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "$type".to_owned(),
                serde_json::Value::String(self.nsid().to_owned()),
            );
            Ok(value)
        } else {
            // generated record types always serialize to an object;
            // reach for a custom serde_json error so callers don't
            // have to match on a panic.
            Err(serde::ser::Error::custom(
                "record did not serialize to a json object",
            ))
        }
    }

    /// Decode a json value into the variant identified by its
    /// embedded `$type` field.
    ///
    /// Intended for callers reading raw PDS record bodies that carry
    /// `$type` inline (e.g. direct `getRecord` responses before the
    /// resolver strips the envelope). Internally this reads the
    /// `$type` string and delegates to [`decode_record`].
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::UnknownNsid`] when `value.$type` is
    /// missing / not a string / not a known nsid, and
    /// [`DecodeError::Serde`] when the body fails to match the
    /// variant its `$type` selects.
    pub fn from_typed_json(mut value: serde_json::Value) -> Result<Self, DecodeError> {
        let Some(serde_json::Value::String(nsid)) =
            value.as_object_mut().and_then(|o| o.remove("$type"))
        else {
            return Err(DecodeError::UnknownNsid("<missing $type field>".to_owned()));
        };
        decode_record(&nsid, value)
    }

    /// Serialize the inner record as a json value, without the
    /// `$type` envelope.
    fn inner_to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        match self {
            Self::Adapter(r) => serde_json::to_value(r),
            Self::Belief(r) => serde_json::to_value(r),
            Self::Bounty(r) => serde_json::to_value(r),
            Self::Community(r) => serde_json::to_value(r),
            Self::Correction(r) => serde_json::to_value(r),
            Self::Dialect(r) => serde_json::to_value(r),
            Self::Encounter(r) => serde_json::to_value(r),
            Self::Observation(r) => serde_json::to_value(r),
            Self::Recommendation(r) => serde_json::to_value(r),
            Self::Retrospection(r) => serde_json::to_value(r),
            Self::Verification(r) => serde_json::to_value(r),
        }
    }
}

/// Serialize an [`AnyRecord`] in its typed-json wire form (the inner
/// record body + a `$type` field carrying the nsid). This is the form
/// every atproto PDS expects on the wire, so callers that treat
/// `AnyRecord` as an opaque handle can serialize it directly without
/// pattern-matching and re-dispatching.
impl Serialize for AnyRecord {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value = self.to_typed_json().map_err(serde::ser::Error::custom)?;
        value.serialize(serializer)
    }
}

impl std::fmt::Display for AnyRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // "AnyRecord(dev.idiolect.encounter)" — concise and operator-
        // friendly. The full body is not rendered; callers who want it
        // go through serde_json directly.
        write!(f, "AnyRecord({})", self.nsid())
    }
}

/// Decode a json value into the [`AnyRecord`] variant selected by
/// `nsid`.
///
/// Appviews use this when consuming the firehose: each commit arrives
/// with a `collection` string identifying the nsid, and the raw cbor
/// blob decodes into whichever record type matches. Unknown nsids
/// return [`DecodeError::UnknownNsid`] without touching the json.
///
/// # Errors
///
/// - [`DecodeError::UnknownNsid`] if `nsid` is not one of the
///   `dev.idiolect.*` records.
/// - [`DecodeError::Serde`] if `value` does not deserialize into the
///   record type selected by `nsid`.
pub fn decode_record(nsid: &str, value: serde_json::Value) -> Result<AnyRecord, DecodeError> {
    fn from<R: Record>(value: serde_json::Value) -> Result<R, DecodeError> {
        serde_json::from_value(value).map_err(DecodeError::Serde)
    }
    match nsid {
        s if s == crate::Community::NSID => Ok(AnyRecord::Community(from(value)?)),
        s if s == crate::Dialect::NSID => Ok(AnyRecord::Dialect(from(value)?)),
        s if s == crate::Encounter::NSID => Ok(AnyRecord::Encounter(from(value)?)),
        s if s == crate::Correction::NSID => Ok(AnyRecord::Correction(from(value)?)),
        s if s == crate::Verification::NSID => Ok(AnyRecord::Verification(from(value)?)),
        s if s == crate::Observation::NSID => Ok(AnyRecord::Observation(from(value)?)),
        s if s == crate::Retrospection::NSID => Ok(AnyRecord::Retrospection(from(value)?)),
        s if s == crate::Recommendation::NSID => Ok(AnyRecord::Recommendation(from(value)?)),
        s if s == crate::Adapter::NSID => Ok(AnyRecord::Adapter(from(value)?)),
        s if s == crate::Bounty::NSID => Ok(AnyRecord::Bounty(from(value)?)),
        s if s == crate::Belief::NSID => Ok(AnyRecord::Belief(from(value)?)),
        other => Err(DecodeError::UnknownNsid(other.to_owned())),
    }
}

/// Errors produced by [`decode_record`].
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    /// The nsid is not one of the `dev.idiolect.*` records.
    #[error("unknown dev.idiolect.* nsid: {0}")]
    UnknownNsid(String),
    /// Deserialization into the selected record type failed.
    #[error("record deserialization failed: {0}")]
    Serde(#[from] serde_json::Error),
}
