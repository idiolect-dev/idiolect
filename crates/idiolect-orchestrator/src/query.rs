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
use idiolect_records::generated::dev::idiolect::defs::LensRef;
use idiolect_records::generated::dev::idiolect::recommendation::RecommendationRequiredVerifications;
use idiolect_records::generated::dev::idiolect::verification::VerificationResult;

use crate::catalog::Catalog;

// -----------------------------------------------------------------
// Re-exports: the spec-driven taxonomy.
// -----------------------------------------------------------------

pub use crate::generated::queries::{
    adapters_by_invocation_protocol, adapters_for_framework, beliefs_about_record,
    beliefs_by_holder, bounties_by_requester, bounties_for_want_lens, communities_by_name,
    communities_for_member, dialects_for_community, open_bounties, recommendations_starting_from,
    verifications_by_kind, verifications_for_lens, vocabularies_by_name, vocabularies_with_world,
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

/// Decide whether `lens` has verifications covering every requirement
/// in `required`, optionally requiring `result=holds` to count.
///
/// A requirement is matched by a verification when the verification's
/// kind corresponds to the requirement's `LensProperty` variant AND the
/// variant's *discriminating* fields agree where the requirement
/// pinned specifics. Empty-string / empty-collection fields on the
/// requirement are treated as "any", so callers get coarse kind-only
/// matching for free when they haven't pinned a specific
/// domain/theorem/standard.
///
/// When `require_holds` is `true`, verifications whose result is
/// `falsified` or `inconclusive` do not count toward coverage even if
/// the structural match succeeds — the recommendation's precondition
/// is that a holding verification exists, not that someone checked.
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
        verifications.iter().any(|entry| {
            requirement_matches(r, &entry.record.property)
                && (!require_holds || entry.record.result == VerificationResult::Holds)
        })
    })
}

/// Whether a verification's `property` value satisfies the given
/// `required` specification. Exposed for downstream filters that
/// need the same structural match without running the full coverage
/// check.
#[must_use]
pub fn requirement_matches(
    required: &RecommendationRequiredVerifications,
    property: &idiolect_records::generated::dev::idiolect::verification::VerificationProperty,
) -> bool {
    use idiolect_records::generated::dev::idiolect::verification::VerificationProperty as V;
    match (required, property) {
        (RecommendationRequiredVerifications::LpRoundtrip(req), V::LpRoundtrip(p)) => {
            str_wildcard_eq(&req.domain, &p.domain)
        }
        (RecommendationRequiredVerifications::LpGenerator(req), V::LpGenerator(p)) => {
            str_wildcard_eq(&req.spec, &p.spec)
                && opt_str_wildcard_eq(req.runner.as_deref(), p.runner.as_deref())
        }
        (RecommendationRequiredVerifications::LpTheorem(req), V::LpTheorem(p)) => {
            str_wildcard_eq(&req.statement, &p.statement)
                && opt_str_wildcard_eq(req.system.as_deref(), p.system.as_deref())
        }
        (RecommendationRequiredVerifications::LpConformance(req), V::LpConformance(p)) => {
            str_wildcard_eq(&req.standard, &p.standard)
                && str_wildcard_eq(&req.version, &p.version)
                && clauses_covered(req.clauses.as_deref(), p.clauses.as_deref())
        }
        (RecommendationRequiredVerifications::LpChecker(req), V::LpChecker(p)) => {
            str_wildcard_eq(&req.checker, &p.checker)
                && opt_str_wildcard_eq(req.ruleset.as_deref(), p.ruleset.as_deref())
        }
        (RecommendationRequiredVerifications::LpConvergence(req), V::LpConvergence(p)) => {
            str_wildcard_eq(&req.property, &p.property)
        }
        _ => false,
    }
}

/// Empty required-value matches any published value; otherwise exact
/// equality. Exposed `pub(crate)` so the eligibility evaluator in
/// [`crate::predicate_eval`] can reuse the same wildcard semantics.
pub(crate) fn str_wildcard_eq(required: &str, published: &str) -> bool {
    required.is_empty() || required == published
}

pub(crate) fn opt_str_wildcard_eq(required: Option<&str>, published: Option<&str>) -> bool {
    match required {
        None | Some("") => true,
        Some(r) => published == Some(r),
    }
}

/// Every clause named in the requirement must appear in the
/// published clause list (when published clauses are declared).
/// Empty / absent required clauses matches any published set.
fn clauses_covered(required: Option<&[String]>, published: Option<&[String]>) -> bool {
    let Some(req) = required else { return true };
    if req.is_empty() {
        return true;
    }
    let Some(pub_clauses) = published else {
        return false;
    };
    req.iter().all(|r| pub_clauses.iter().any(|p| p == r))
}

/// Aggregate counts of every catalog kind. Useful for a `/stats`
/// endpoint or for a dashboard; no filtering, no sort.
#[must_use]
pub fn catalog_stats(catalog: &Catalog) -> CatalogStats {
    CatalogStats {
        adapters: catalog.adapters().count(),
        beliefs: catalog.beliefs().count(),
        bounties: catalog.bounties().count(),
        communities: catalog.communities().count(),
        dialects: catalog.dialects().count(),
        recommendations: catalog.recommendations().count(),
        verifications: catalog.verifications().count(),
        vocabularies: catalog.vocabularies().count(),
    }
}

/// Return value of [`catalog_stats`]. Each field is the count of
/// records of that kind currently in the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogStats {
    /// Count of `Adapter` records.
    pub adapters: usize,
    /// Count of `Belief` records.
    pub beliefs: usize,
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
    /// Count of `Vocab` records.
    pub vocabularies: usize,
}

impl CatalogStats {
    /// Sum of all kinds.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.adapters
            + self.beliefs
            + self.bounties
            + self.communities
            + self.dialects
            + self.recommendations
            + self.verifications
            + self.vocabularies
    }
}
