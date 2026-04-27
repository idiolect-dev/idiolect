//! Family-of-records abstraction.
//!
//! [`RecordFamily`] is the trait every other workspace boundary
//! parameterises over once a consumer wants to index, observe, or
//! orchestrate a non-`dev.idiolect.*` record set. The hand-written
//! `dev.idiolect.*` family ([`AnyRecord`](crate::AnyRecord),
//! [`decode_record`](crate::decode_record), and friends) is the
//! generated `IdiolectFamily` shipped with this crate; downstream
//! consumers like `layers-pub` point `idiolect-codegen` at their own
//! lexicon set and get an isomorphic family back.
//!
//! # Composition
//!
//! Two families compose via [`OrFamily`] into a single family that
//! recognises every NSID either side claims. The compound's
//! [`AnyRecord`](RecordFamily::AnyRecord) is [`OrAny`], a tagged
//! union over the two halves. This is the dialect / curated-bundle
//! shape: a community curates a sub-family, and any consumer that
//! wants both can just use `OrFamily<F1, F2>`.
//!
//! Identical NSIDs claimed by both halves of an [`OrFamily`] are a
//! configuration error; [`OrFamily::decode`] resolves the left
//! family first, so the right family is shadowed in that case.
//! Detect overlaps via [`detect_or_family_overlap`] when wiring
//! production code paths.

use std::marker::PhantomData;

use crate::nsid::Nsid;
use crate::record::DecodeError;

/// A family is a discriminated set of record types: a membership
/// predicate over NSIDs, a decoder from JSON to a typed
/// [`AnyRecord`](Self::AnyRecord), and a serializer back.
///
/// Implementors are typically zero-sized marker types
/// (`pub struct IdiolectFamily;`); the trait's associated type and
/// const carry the family's whole identity, so the marker exists
/// only to dispatch through the trait system.
///
/// # Required methods
///
/// - [`contains`](Self::contains) — does this family carry a record
///   type at the given NSID?
/// - [`decode`](Self::decode) — turn a wire-form JSON body into the
///   corresponding `AnyRecord` variant. Returns `Ok(None)` for
///   out-of-family NSIDs so callers can chain through compound
///   families without an error variant.
/// - [`nsid_str`](Self::nsid_str) — the canonical NSID string of the
///   variant carried by an `AnyRecord`.
/// - [`to_typed_json`](Self::to_typed_json) — serialise an
///   `AnyRecord` back to wire form. Mirrors
///   [`AnyRecord::to_typed_json`](crate::AnyRecord::to_typed_json).
pub trait RecordFamily: Send + Sync + 'static {
    /// Discriminated-union view over every record type in this
    /// family.
    type AnyRecord: Clone + std::fmt::Debug + Send + Sync;

    /// Informational identifier for the family (e.g.
    /// `"dev.idiolect"` for the family this crate ships,
    /// `"pub.layers"` for `layers-pub`, or a community-curated id
    /// for a dialect bundle).
    ///
    /// This does *not* define membership — see
    /// [`contains`](Self::contains). Two families may share an ID
    /// prefix without being the same family, and a curated
    /// sub-family may have an ID that is not an NSID prefix at all.
    const ID: &'static str;

    /// Membership predicate. The family defines its own NSID set;
    /// not tied to a single prefix so curated / compound families
    /// work correctly.
    fn contains(nsid: &Nsid) -> bool;

    /// Decode a record body into the family's `AnyRecord`. Returns
    /// `Ok(None)` when `nsid` is not a member of this family, so
    /// callers can chain across families without error handling.
    ///
    /// # Errors
    ///
    /// [`DecodeError::Serde`] when `nsid` is in the family but
    /// `body` fails to deserialize into the matching record type.
    /// `DecodeError::UnknownNsid` is *not* used here —
    /// out-of-family NSIDs return `Ok(None)`.
    fn decode(nsid: &Nsid, body: serde_json::Value)
    -> Result<Option<Self::AnyRecord>, DecodeError>;

    /// Canonical NSID string of the variant carried by `record`.
    fn nsid_str(record: &Self::AnyRecord) -> &'static str;

    /// Serialize `record` back to wire form (record body + a
    /// `$type` field). Used by record publishers.
    ///
    /// # Errors
    ///
    /// [`serde_json::Error`] when the inner record fails to
    /// serialize, or when its serialized form is not a JSON object.
    fn to_typed_json(record: &Self::AnyRecord) -> Result<serde_json::Value, serde_json::Error>;
}

/// Tagged union over two record families.
///
/// Used as the `AnyRecord` of [`OrFamily<F1, F2>`]. `Left` carries
/// `F1::AnyRecord`, `Right` carries `F2::AnyRecord`. The variant
/// names track which side decoded the record, not any preference
/// ordering — both sides are equally first-class.
#[derive(Debug, Clone)]
pub enum OrAny<A1, A2> {
    /// Decoded by the left family.
    Left(A1),
    /// Decoded by the right family.
    Right(A2),
}

/// Compose two record families into a single family that recognises
/// every NSID either half claims.
///
/// Membership is the union of the two sides. Decoding tries the
/// left family first; on `Ok(None)` it tries the right. An NSID
/// claimed by both halves is shadowed by the left.
///
/// # Example
///
/// ```ignore
/// // pseudo-code; the real example would import generated families
/// type Combined = OrFamily<IdiolectFamily, LayersFamily>;
/// // drive_indexer::<Combined, _, _, _>(...)
/// ```
pub struct OrFamily<F1, F2>(PhantomData<(F1, F2)>);

impl<F1, F2> RecordFamily for OrFamily<F1, F2>
where
    F1: RecordFamily,
    F2: RecordFamily,
{
    type AnyRecord = OrAny<F1::AnyRecord, F2::AnyRecord>;

    /// `OrFamily<F1, F2>` reports its ID as `"<F1>+<F2>"`. Curated
    /// bundles that want a stable ID should wrap an `OrFamily`
    /// chain in their own [`RecordFamily`] impl with a chosen ID.
    const ID: &'static str = "OrFamily";

    fn contains(nsid: &Nsid) -> bool {
        F1::contains(nsid) || F2::contains(nsid)
    }

    fn decode(
        nsid: &Nsid,
        body: serde_json::Value,
    ) -> Result<Option<Self::AnyRecord>, DecodeError> {
        // F1 first; only fall through on F1's "not in this family"
        // signal. A serde error from F1 still surfaces — partial
        // decode mid-flight is not silently ignored.
        if F1::contains(nsid) {
            return F1::decode(nsid, body).map(|opt| opt.map(OrAny::Left));
        }
        if F2::contains(nsid) {
            return F2::decode(nsid, body).map(|opt| opt.map(OrAny::Right));
        }
        Ok(None)
    }

    fn nsid_str(record: &Self::AnyRecord) -> &'static str {
        match record {
            OrAny::Left(left) => F1::nsid_str(left),
            OrAny::Right(right) => F2::nsid_str(right),
        }
    }

    fn to_typed_json(record: &Self::AnyRecord) -> Result<serde_json::Value, serde_json::Error> {
        match record {
            OrAny::Left(left) => F1::to_typed_json(left),
            OrAny::Right(right) => F2::to_typed_json(right),
        }
    }
}

/// Diagnostic helper: walk the NSIDs `F1` claims and report any that
/// `F2` also claims. Use at boot time when wiring an
/// [`OrFamily`] to catch configuration errors that would otherwise
/// only manifest as a silently-shadowed right-side decode.
///
/// Returns the empty vec when there is no overlap. Callers that want
/// strict semantics should `assert!(detect_or_family_overlap(...).is_empty())`.
///
/// The probe set is the caller's responsibility — pass the NSIDs
/// you care about (e.g. the union of [`F1::ID`](RecordFamily::ID)
/// and [`F2::ID`](RecordFamily::ID)-shaped probes, or a fixed
/// audit list).
#[must_use]
pub fn detect_or_family_overlap<F1, F2>(probe: &[Nsid]) -> Vec<Nsid>
where
    F1: RecordFamily,
    F2: RecordFamily,
{
    probe
        .iter()
        .filter(|n| F1::contains(n) && F2::contains(n))
        .cloned()
        .collect()
}
