//! The shape of a firehose event once it has been decoded into a
//! record from one (or several composed) record families.
//!
//! `tapped::RecordEvent` gives us metadata (did, rkey, collection,
//! action, live) plus a JSON slice. The indexer lifts that into
//! [`IndexerEvent`] by decoding the slice through the supplied
//! family's
//! [`RecordFamily::decode`](idiolect_records::RecordFamily::decode).
//! Handlers consume `IndexerEvent<F>`, parameterised over their own
//! family, so the core crate stays family-agnostic.

use idiolect_records::{IdiolectFamily, Nsid, RecordFamily};

/// Commit action that produced this event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexerAction {
    /// Record was created in the PDS.
    Create,
    /// Record was updated in place. The appview may or may not have
    /// the prior revision; `tap` does not re-emit it.
    Update,
    /// Record was deleted. No body is present on the event.
    Delete,
}

/// Decoded firehose commit, ready for handler dispatch.
///
/// Generic over `F: RecordFamily`. The `IdiolectFamily` default
/// keeps the public type name `IndexerEvent` source-compatible for
/// existing callers; downstream consumers (e.g. `layers-pub` over
/// `pub.layers.*`) write `IndexerEvent<LayersFamily>` (or compose
/// via [`OrFamily`](idiolect_records::OrFamily)).
#[derive(Debug, Clone)]
pub struct IndexerEvent<F: RecordFamily = IdiolectFamily> {
    /// Sequence id assigned by tap. Monotonic per connection;
    /// suitable for cursor commits on live events.
    pub seq: u64,
    /// True if this event came off the live firehose. False for
    /// backfill / resync replays.
    pub live: bool,
    /// DID of the repo the record belongs to.
    pub did: String,
    /// Repository revision (TID) this commit advanced to.
    pub rev: String,
    /// Record key (usually a TID) within the collection.
    pub rkey: String,
    /// Lexicon NSID of the commit's collection.
    pub collection: Nsid,
    /// Commit action: create / update / delete.
    pub action: IndexerAction,
    /// CID of the record body (`None` on delete).
    pub cid: Option<String>,
    /// Decoded record body. `None` on delete or when the NSID is
    /// not a member of the configured family; otherwise the
    /// strongly-typed `AnyRecord` variant matching `collection`.
    pub record: Option<F::AnyRecord>,
}

impl<F: RecordFamily> IndexerEvent<F> {
    /// Convenience: the at-uri at which this event's record lives.
    #[must_use]
    pub fn at_uri(&self) -> String {
        format!("at://{}/{}/{}", self.did, self.collection, self.rkey)
    }
}
