//! The `Record` trait — the abstraction every appview uses to treat
//! one record kind in a family the same as the next.
//!
//! Every record type in [`crate::generated`] implements `Record` via
//! a blanket impl emitted by `idiolect-codegen`. Appview indexers,
//! xrpc handlers, and test fixtures can then be generic over `R:
//! Record` instead of matching on NSIDs by hand.
//!
//! The companion `AnyRecord` enum, `decode_record` function, and
//! `IdiolectFamily` marker type live in
//! [`crate::generated::family`] — they are codegen output, with one
//! variant per record-type lexicon. Adding a record type to the
//! family is a one-line lexicon change. The trait everything
//! parameterises over is [`crate::family::RecordFamily`]; the
//! `dev.idiolect.*` family is one implementor of it, on equal
//! footing with any downstream-curated family
//! (`pub.layers.*`, dialect bundles, etc.).

use serde::{Serialize, de::DeserializeOwned};

use crate::nsid::Nsid;

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
    /// Fully-qualified lexicon nsid as a string, e.g.
    /// `"dev.idiolect.encounter"`. Kept as `&'static str` because the
    /// typed [`Nsid`] cannot be constructed in `const` context;
    /// callers wanting the typed form should use [`Self::nsid()`].
    const NSID: &'static str;

    /// The fully-qualified lexicon NSID as a typed value. The default
    /// impl parses [`Self::NSID`] on each call; this is cheap (no
    /// allocation beyond what `Nsid` itself needs) but if hot, cache
    /// the result locally.
    ///
    /// # Panics
    ///
    /// Panics if [`Self::NSID`] is not a valid atproto NSID. Codegen
    /// emits a unit test per record that proves this never panics in
    /// practice.
    #[must_use]
    fn nsid() -> Nsid {
        Nsid::parse(Self::NSID).expect("Record::NSID must be a valid atproto NSID")
    }

    /// Human-readable short name of the record kind, e.g.
    /// `"encounter"`. Equivalent to `Self::nsid().name()`.
    #[must_use]
    fn kind() -> &'static str {
        let n = Self::NSID;
        n.rsplit('.').next().unwrap_or(n)
    }
}

/// Errors produced by [`crate::decode_record`].
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    /// The nsid is not one of the family's records.
    #[error("unknown nsid for this record family: {0}")]
    UnknownNsid(String),
    /// Deserialization into the selected record type failed.
    #[error("record deserialization failed: {0}")]
    Serde(#[from] serde_json::Error),
}
