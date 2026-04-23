//! Declarative read surface over a [`Catalog`].
//!
//! The taxonomy of list queries (`open_bounties`,
//! `adapters_for_framework`, …) is re-exported from
//! [`crate::generated::queries`], which is emitted from
//! `orchestrator-spec/queries.json`.
//!
//! This module keeps:
//!
//! - re-exports of the generated list queries (kept under the
//!   `query::` path for callers);
//! - the non-list queries ([`catalog_stats`],
//!   [`sufficient_verifications_for`]) and the
//!   [`preferred_lenses_for_dialect`] accessor.
//!
//! To add a list query: edit `orchestrator-spec/queries.json`, add a
//! matching predicate in `crate::predicates`, and run
//! `cargo run -p idiolect-codegen -- generate`.

use idiolect_records::Dialect;
use idiolect_records::generated::defs::LensRef;
use idiolect_records::generated::recommendation::RecommendationRequiredVerifications;
use idiolect_records::generated::verification::{VerificationKind, VerificationResult};

use crate::catalog::Catalog;

// -----------------------------------------------------------------
// Re-exports: the spec-driven taxonomy.
// -----------------------------------------------------------------

pub use crate::generated::queries::{
    adapters_by_invocation_protocol, adapters_for_framework, bounties_by_requester,
    bounties_for_want_lens, communities_by_name, communities_for_member, dialects_for_community,
    open_bounties, recommendations_starting_from, verifications_by_kind, verifications_for_lens,
};

// Reference-matching helpers stay public on the query module too so
// existing callers that imported them here keep compiling.
pub use crate::predicates::{lens_refs_match, schema_refs_match};

// -----------------------------------------------------------------
// Hand-written queries: non-list shapes that don't fit the spec.
// -----------------------------------------------------------------

/// Preferred lenses declared on a dialect, in declaration order.
/// Returns an empty slice if the dialect has no `preferred_lenses`
/// set.
#[must_use]
pub fn preferred_lenses_for_dialect(dialect: &Dialect) -> &[LensRef] {
    dialect.preferred_lenses.as_deref().unwrap_or(&[])
}

/// Map a [`RecommendationRequiredVerifications`] tagged-union variant
/// to the [`VerificationKind`] that satisfies it.
///
/// Under v0.2.0, required verifications are `LensProperty` values, not
/// raw kinds. For this coverage check we still reduce each requirement
/// to its coarser kind — a future extension can also demand that the
/// verification's `property` field matches structurally.
#[must_use]
pub const fn required_kind_to_verification_kind(
    kind: &RecommendationRequiredVerifications,
) -> VerificationKind {
    match kind {
        RecommendationRequiredVerifications::LpRoundtrip(_) => VerificationKind::RoundtripTest,
        RecommendationRequiredVerifications::LpGenerator(_) => VerificationKind::PropertyTest,
        RecommendationRequiredVerifications::LpTheorem(_) => VerificationKind::FormalProof,
        RecommendationRequiredVerifications::LpConformance(_) => VerificationKind::ConformanceTest,
        RecommendationRequiredVerifications::LpChecker(_) => VerificationKind::StaticCheck,
        RecommendationRequiredVerifications::LpConvergence(_) => {
            VerificationKind::ConvergencePreserving
        }
    }
}

/// Decide whether `lens` has verifications covering every kind in
/// `required`, optionally requiring `result=holds` to count.
///
/// When `require_holds` is `true`, verifications whose result is
/// `falsified` or `inconclusive` do not count toward coverage even if
/// the kind matches — the recommendation's precondition is that a
/// holding verification exists, not that someone checked.
#[must_use]
pub fn sufficient_verifications_for(
    catalog: &Catalog,
    lens: &LensRef,
    required: &[RecommendationRequiredVerifications],
    require_holds: bool,
) -> bool {
    if required.is_empty() {
        return true;
    }
    let verifications = verifications_for_lens(catalog, lens);
    required.iter().all(|r| {
        let want = required_kind_to_verification_kind(r);
        verifications.iter().any(|entry| {
            entry.record.kind == want
                && (!require_holds || entry.record.result == VerificationResult::Holds)
        })
    })
}

/// Aggregate counts of every catalog kind. Useful for a `/stats`
/// endpoint or for a dashboard; no filtering, no sort.
#[must_use]
pub fn catalog_stats(catalog: &Catalog) -> CatalogStats {
    CatalogStats {
        adapters: catalog.adapters().count(),
        bounties: catalog.bounties().count(),
        communities: catalog.communities().count(),
        dialects: catalog.dialects().count(),
        recommendations: catalog.recommendations().count(),
        verifications: catalog.verifications().count(),
    }
}

/// Return value of [`catalog_stats`]. Each field is the count of
/// records of that kind currently in the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct CatalogStats {
    /// Count of `Adapter` records.
    pub adapters: usize,
    /// Count of `Bounty` records.
    pub bounties: usize,
    /// Count of `Community` records.
    pub communities: usize,
    /// Count of `Dialect` records.
    pub dialects: usize,
    /// Count of `Recommendation` records.
    pub recommendations: usize,
    /// Count of `Verification` records.
    pub verifications: usize,
}

impl CatalogStats {
    /// Sum of all kinds.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.adapters
            + self.bounties
            + self.communities
            + self.dialects
            + self.recommendations
            + self.verifications
    }
}
