//! In-memory catalog of orchestrator-relevant records.
//!
//! The catalog keeps one slot per record at-uri per record kind. It is
//! the idiolect orchestrator's analog of an indexed read model: a
//! `RecordHandler` folds firehose commits into it, and the query
//! surface (see [`crate::query`]) reads out declarative conclusions
//! (matching, preference, sufficiency).
//!
//! The catalog stores *the most recent revision* of each record at its
//! at-uri — record updates rewrite the slot, deletes remove it. That
//! matches atproto's single-record-per-rkey semantics and is the
//! minimum needed for the reference queries below.
//!
//! Record kinds the catalog tracks:
//!
//! - [`Adapter`] — framework wrappers.
//! - [`Belief`] — doxastic claims attributing attitudes to third parties.
//! - [`Bounty`] — open / claimed / fulfilled requests.
//! - [`Community`] — self-constituted groups.
//! - [`ChangeProposal`] — governed definition changes and their evidence.
//! - [`CommunityRelease`] — signed community release indexes.
//! - [`Dialect`] — per-community preferred lens/schema bundles.
//! - [`Federation`] — declared inter-community relationships.
//! - [`MigrationRun`] — durable migration progress and failure state.
//! - [`Recommendation`] — community-declared lens paths.
//! - [`Verification`] — signed assertions about a lens's properties.
//! - [`Vocab`] — published action / purpose vocabularies; the
//!   theory-resolver registers these on ingest so subsumption
//!   queries work against catalog-live vocabularies.
//!
//! Encounters, corrections, retrospections, and observations do *not*
//! live in the orchestrator catalog: those are observer territory.
//! Keeping the split explicit means an orchestrator never grows an
//! opinion about encounter traffic beyond what an observer publishes
//! as a structured [`Observation`](idiolect_records::Observation).

use std::collections::HashMap;

use idiolect_records::{
    Adapter, AnyRecord, Belief, Bounty, ChangeProposal, Community, CommunityRelease, Deliberation,
    DeliberationOutcome, DeliberationStatement, DeliberationVote, Dialect, Federation,
    MigrationRun, Recommendation, Verification, Vocab,
};

/// Cataloged record, keyed by at-uri in the parent maps.
///
/// We keep the decoded record and the lightweight metadata callers
/// want (who wrote it, at what rev) so query results can cite
/// provenance without a second lookup.
#[derive(Debug, Clone)]
pub struct Entry<R> {
    /// AT-URI at which the record lives.
    pub uri: String,
    /// DID of the repo that published the record.
    pub author: String,
    /// Repository revision of the commit that most recently wrote this
    /// record.
    pub rev: String,
    /// The record itself.
    pub record: R,
}

/// In-memory catalog.
///
/// Thread-safety is left to the caller: use `Arc<Mutex<Catalog>>` or
/// `Arc<RwLock<Catalog>>` to share across tasks. The handler module
/// wraps the catalog in a `Mutex` for the reference pipeline.
#[derive(Debug, Default)]
pub struct Catalog {
    adapters: HashMap<String, Entry<Adapter>>,
    beliefs: HashMap<String, Entry<Belief>>,
    bounties: HashMap<String, Entry<Bounty>>,
    change_proposals: HashMap<String, Entry<ChangeProposal>>,
    communities: HashMap<String, Entry<Community>>,
    community_releases: HashMap<String, Entry<CommunityRelease>>,
    deliberations: HashMap<String, Entry<Deliberation>>,
    deliberation_statements: HashMap<String, Entry<DeliberationStatement>>,
    deliberation_votes: HashMap<String, Entry<DeliberationVote>>,
    deliberation_outcomes: HashMap<String, Entry<DeliberationOutcome>>,
    dialects: HashMap<String, Entry<Dialect>>,
    federations: HashMap<String, Entry<Federation>>,
    migration_runs: HashMap<String, Entry<MigrationRun>>,
    recommendations: HashMap<String, Entry<Recommendation>>,
    verifications: HashMap<String, Entry<Verification>>,
    vocabularies: HashMap<String, Entry<Vocab>>,
}

impl Catalog {
    /// Construct an empty catalog.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ingest one decoded record. Unknown or unsupported record kinds
    /// (encounters, corrections, retrospections, observations,
    /// vendored panproto types) are silently ignored — orchestrators
    /// and observers both see the same firehose, so the catalog has to
    /// tolerate records it does not care about.
    #[allow(clippy::too_many_lines)] // Mechanical match-and-insert per record kind; not factorable without obscuring the wire mapping.
    pub fn upsert(&mut self, uri: String, author: String, rev: String, record: AnyRecord) {
        // In atproto, record at-uris are already scoped by collection
        // (`at://<did>/<collection>/<rkey>`), so two distinct kinds
        // sharing a URI is formally impossible on-the-wire. Defend
        // against buggy upstreams and test fixtures anyway: removing
        // from every kind's slot before the insert keeps the
        // invariant "one at-uri, at most one cataloged record" even
        // if someone hands us a URI that was previously stored under
        // a different kind.
        self.remove(&uri);

        match record {
            AnyRecord::Adapter(a) => {
                self.adapters.insert(
                    uri.clone(),
                    Entry {
                        uri,
                        author,
                        rev,
                        record: a,
                    },
                );
            }
            AnyRecord::Bounty(b) => {
                self.bounties.insert(
                    uri.clone(),
                    Entry {
                        uri,
                        author,
                        rev,
                        record: b,
                    },
                );
            }
            AnyRecord::Community(c) => {
                self.communities.insert(
                    uri.clone(),
                    Entry {
                        uri,
                        author,
                        rev,
                        record: c,
                    },
                );
            }
            AnyRecord::ChangeProposal(c) => {
                self.change_proposals.insert(
                    uri.clone(),
                    Entry {
                        uri,
                        author,
                        rev,
                        record: c,
                    },
                );
            }
            AnyRecord::CommunityRelease(r) => {
                self.community_releases.insert(
                    uri.clone(),
                    Entry {
                        uri,
                        author,
                        rev,
                        record: r,
                    },
                );
            }
            AnyRecord::Dialect(d) => {
                self.dialects.insert(
                    uri.clone(),
                    Entry {
                        uri,
                        author,
                        rev,
                        record: d,
                    },
                );
            }
            AnyRecord::Federation(f) => {
                self.federations.insert(
                    uri.clone(),
                    Entry {
                        uri,
                        author,
                        rev,
                        record: f,
                    },
                );
            }
            AnyRecord::MigrationRun(m) => {
                self.migration_runs.insert(
                    uri.clone(),
                    Entry {
                        uri,
                        author,
                        rev,
                        record: m,
                    },
                );
            }
            AnyRecord::Recommendation(r) => {
                self.recommendations.insert(
                    uri.clone(),
                    Entry {
                        uri,
                        author,
                        rev,
                        record: r,
                    },
                );
            }
            AnyRecord::Verification(v) => {
                self.verifications.insert(
                    uri.clone(),
                    Entry {
                        uri,
                        author,
                        rev,
                        record: v,
                    },
                );
            }
            AnyRecord::Belief(b) => {
                self.beliefs.insert(
                    uri.clone(),
                    Entry {
                        uri,
                        author,
                        rev,
                        record: b,
                    },
                );
            }
            AnyRecord::Vocab(v) => {
                self.vocabularies.insert(
                    uri.clone(),
                    Entry {
                        uri,
                        author,
                        rev,
                        record: v,
                    },
                );
            }
            AnyRecord::Deliberation(d) => {
                self.deliberations.insert(
                    uri.clone(),
                    Entry {
                        uri,
                        author,
                        rev,
                        record: d,
                    },
                );
            }
            AnyRecord::DeliberationStatement(s) => {
                self.deliberation_statements.insert(
                    uri.clone(),
                    Entry {
                        uri,
                        author,
                        rev,
                        record: s,
                    },
                );
            }
            AnyRecord::DeliberationVote(v) => {
                self.deliberation_votes.insert(
                    uri.clone(),
                    Entry {
                        uri,
                        author,
                        rev,
                        record: v,
                    },
                );
            }
            AnyRecord::DeliberationOutcome(o) => {
                self.deliberation_outcomes.insert(
                    uri.clone(),
                    Entry {
                        uri,
                        author,
                        rev,
                        record: o,
                    },
                );
            }
            _ => {}
        }
    }

    /// Remove a record from the catalog by at-uri. The kind is not
    /// known at delete time, so every kind's slot is checked; a
    /// single-key lookup per map is cheap.
    pub fn remove(&mut self, uri: &str) {
        self.adapters.remove(uri);
        self.beliefs.remove(uri);
        self.bounties.remove(uri);
        self.change_proposals.remove(uri);
        self.communities.remove(uri);
        self.community_releases.remove(uri);
        self.deliberations.remove(uri);
        self.deliberation_statements.remove(uri);
        self.deliberation_votes.remove(uri);
        self.deliberation_outcomes.remove(uri);
        self.dialects.remove(uri);
        self.federations.remove(uri);
        self.migration_runs.remove(uri);
        self.recommendations.remove(uri);
        self.verifications.remove(uri);
        self.vocabularies.remove(uri);
    }

    /// Immutable view of all cataloged beliefs.
    pub fn beliefs(&self) -> impl Iterator<Item = &Entry<Belief>> {
        self.beliefs.values()
    }

    /// Immutable view of all cataloged vocabularies.
    pub fn vocabularies(&self) -> impl Iterator<Item = &Entry<Vocab>> {
        self.vocabularies.values()
    }

    /// Immutable view of all cataloged adapters, in arbitrary order.
    pub fn adapters(&self) -> impl Iterator<Item = &Entry<Adapter>> {
        self.adapters.values()
    }

    /// Immutable view of all cataloged bounties.
    pub fn bounties(&self) -> impl Iterator<Item = &Entry<Bounty>> {
        self.bounties.values()
    }

    /// Immutable view of all cataloged communities.
    pub fn communities(&self) -> impl Iterator<Item = &Entry<Community>> {
        self.communities.values()
    }

    /// Immutable view of all governed change proposals.
    pub fn change_proposals(&self) -> impl Iterator<Item = &Entry<ChangeProposal>> {
        self.change_proposals.values()
    }

    /// Immutable view of all signed community releases.
    pub fn community_releases(&self) -> impl Iterator<Item = &Entry<CommunityRelease>> {
        self.community_releases.values()
    }

    /// Immutable view of all cataloged dialects.
    pub fn dialects(&self) -> impl Iterator<Item = &Entry<Dialect>> {
        self.dialects.values()
    }

    /// Immutable view of all declared federation relationships.
    pub fn federations(&self) -> impl Iterator<Item = &Entry<Federation>> {
        self.federations.values()
    }

    /// Immutable view of all durable migration runs.
    pub fn migration_runs(&self) -> impl Iterator<Item = &Entry<MigrationRun>> {
        self.migration_runs.values()
    }

    /// Immutable view of all cataloged recommendations.
    pub fn recommendations(&self) -> impl Iterator<Item = &Entry<Recommendation>> {
        self.recommendations.values()
    }

    /// Immutable view of all cataloged verifications.
    pub fn verifications(&self) -> impl Iterator<Item = &Entry<Verification>> {
        self.verifications.values()
    }

    /// Immutable view of all cataloged deliberations.
    pub fn deliberations(&self) -> impl Iterator<Item = &Entry<Deliberation>> {
        self.deliberations.values()
    }

    /// Immutable view of all cataloged deliberation statements.
    pub fn deliberation_statements(&self) -> impl Iterator<Item = &Entry<DeliberationStatement>> {
        self.deliberation_statements.values()
    }

    /// Immutable view of all cataloged deliberation votes.
    pub fn deliberation_votes(&self) -> impl Iterator<Item = &Entry<DeliberationVote>> {
        self.deliberation_votes.values()
    }

    /// Immutable view of all cataloged deliberation outcomes.
    pub fn deliberation_outcomes(&self) -> impl Iterator<Item = &Entry<DeliberationOutcome>> {
        self.deliberation_outcomes.values()
    }

    /// Look up a deliberation by at-uri.
    #[must_use]
    pub fn deliberation(&self, uri: &str) -> Option<&Entry<Deliberation>> {
        self.deliberations.get(uri)
    }

    /// Look up a single record by at-uri. Scans every kind; `None`
    /// when no kind matches.
    #[must_use]
    pub fn find(&self, uri: &str) -> Option<CatalogRef<'_>> {
        if let Some(e) = self.adapters.get(uri) {
            return Some(CatalogRef::Adapter(e));
        }
        if let Some(e) = self.beliefs.get(uri) {
            return Some(CatalogRef::Belief(e));
        }
        if let Some(e) = self.bounties.get(uri) {
            return Some(CatalogRef::Bounty(e));
        }
        if let Some(e) = self.communities.get(uri) {
            return Some(CatalogRef::Community(e));
        }
        if let Some(e) = self.change_proposals.get(uri) {
            return Some(CatalogRef::ChangeProposal(e));
        }
        if let Some(e) = self.community_releases.get(uri) {
            return Some(CatalogRef::CommunityRelease(e));
        }
        if let Some(e) = self.dialects.get(uri) {
            return Some(CatalogRef::Dialect(e));
        }
        if let Some(e) = self.federations.get(uri) {
            return Some(CatalogRef::Federation(e));
        }
        if let Some(e) = self.migration_runs.get(uri) {
            return Some(CatalogRef::MigrationRun(e));
        }
        if let Some(e) = self.recommendations.get(uri) {
            return Some(CatalogRef::Recommendation(e));
        }
        if let Some(e) = self.verifications.get(uri) {
            return Some(CatalogRef::Verification(e));
        }
        if let Some(e) = self.vocabularies.get(uri) {
            return Some(CatalogRef::Vocab(e));
        }
        None
    }

    /// Total number of records across all kinds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.adapters.len()
            + self.beliefs.len()
            + self.bounties.len()
            + self.change_proposals.len()
            + self.communities.len()
            + self.community_releases.len()
            + self.dialects.len()
            + self.federations.len()
            + self.migration_runs.len()
            + self.recommendations.len()
            + self.verifications.len()
            + self.vocabularies.len()
    }

    /// Whether the catalog has no records.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Polymorphic borrow over the catalog's per-kind entries. Returned
/// by [`Catalog::find`] so callers can pattern-match on the kind.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum CatalogRef<'a> {
    /// An [`Adapter`] record.
    Adapter(&'a Entry<Adapter>),
    /// A [`Belief`] record.
    Belief(&'a Entry<Belief>),
    /// A [`Bounty`] record.
    Bounty(&'a Entry<Bounty>),
    /// A [`ChangeProposal`] record.
    ChangeProposal(&'a Entry<ChangeProposal>),
    /// A [`Community`] record.
    Community(&'a Entry<Community>),
    /// A [`CommunityRelease`] record.
    CommunityRelease(&'a Entry<CommunityRelease>),
    /// A [`Dialect`] record.
    Dialect(&'a Entry<Dialect>),
    /// A [`Federation`] record.
    Federation(&'a Entry<Federation>),
    /// A [`MigrationRun`] record.
    MigrationRun(&'a Entry<MigrationRun>),
    /// A [`Recommendation`] record.
    Recommendation(&'a Entry<Recommendation>),
    /// A [`Verification`] record.
    Verification(&'a Entry<Verification>),
    /// A [`Vocab`] record.
    Vocab(&'a Entry<Vocab>),
}
