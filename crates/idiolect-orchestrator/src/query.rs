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

use idiolect_records::generated::dev::idiolect::defs::LensRef;
use idiolect_records::generated::dev::idiolect::recommendation::RecommendationRequiredVerifications;
use idiolect_records::generated::dev::idiolect::verification::VerificationResult;
use idiolect_records::{
    Deliberation, DeliberationOutcome, DeliberationStatement, DeliberationVote, Dialect,
};

use crate::catalog::{Catalog, Entry};

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
        (RecommendationRequiredVerifications::LpCoercionLaw(req), V::LpCoercionLaw(p)) => {
            str_wildcard_eq(&req.standard, &p.standard)
                && opt_str_wildcard_eq(req.version.as_deref(), p.version.as_deref())
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
        deliberations: catalog.deliberations().count(),
        deliberation_statements: catalog.deliberation_statements().count(),
        deliberation_votes: catalog.deliberation_votes().count(),
        deliberation_outcomes: catalog.deliberation_outcomes().count(),
        dialects: catalog.dialects().count(),
        recommendations: catalog.recommendations().count(),
        verifications: catalog.verifications().count(),
        vocabularies: catalog.vocabularies().count(),
    }
}

/// Resolve a deliberation by AT-URI to the full deliberation view
/// — the deliberation record plus every statement that strong-refs
/// it, plus every vote whose subject is one of those statements,
/// plus the latest published outcome (when one exists).
///
/// Statements and votes are returned in catalog-iteration order
/// (`BTreeMap` order on at-uri); consumers wanting a different order
/// (e.g. by vote count, by createdAt) sort downstream.
///
/// Returns `None` when the deliberation at `uri` is not in the
/// catalog. Returns `Some(view)` with empty statement / vote lists
/// when the deliberation exists but has no associated records yet.
#[must_use]
pub fn get_deliberation<'c>(catalog: &'c Catalog, uri: &str) -> Option<DeliberationView<'c>> {
    let deliberation = catalog.deliberation(uri)?;
    let statements: Vec<&Entry<DeliberationStatement>> = catalog
        .deliberation_statements()
        .filter(|s| s.record.deliberation.uri.as_str() == uri)
        .collect();
    let statement_uris: std::collections::BTreeSet<&str> =
        statements.iter().map(|s| s.uri.as_str()).collect();
    let votes: Vec<&Entry<DeliberationVote>> = catalog
        .deliberation_votes()
        .filter(|v| statement_uris.contains(v.record.subject.uri.as_str()))
        .collect();
    // Outcomes that strong-ref this deliberation. Multiple outcomes
    // are allowed; consumers pick by computedAt.
    let outcomes: Vec<&Entry<DeliberationOutcome>> = catalog
        .deliberation_outcomes()
        .filter(|o| o.record.deliberation.uri.as_str() == uri)
        .collect();
    Some(DeliberationView {
        deliberation,
        statements,
        votes,
        outcomes,
    })
}

/// Composed view returned by [`get_deliberation`].
#[derive(Debug)]
pub struct DeliberationView<'c> {
    /// The deliberation record itself.
    pub deliberation: &'c Entry<Deliberation>,
    /// Every statement strong-refing this deliberation.
    pub statements: Vec<&'c Entry<DeliberationStatement>>,
    /// Every vote whose subject is one of those statements.
    pub votes: Vec<&'c Entry<DeliberationVote>>,
    /// Outcome records for this deliberation. Multiple are
    /// permitted; consumers select by `computedAt`.
    pub outcomes: Vec<&'c Entry<DeliberationOutcome>>,
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
    /// Count of `Deliberation` records.
    pub deliberations: usize,
    /// Count of `DeliberationStatement` records.
    pub deliberation_statements: usize,
    /// Count of `DeliberationVote` records.
    pub deliberation_votes: usize,
    /// Count of `DeliberationOutcome` records.
    pub deliberation_outcomes: usize,
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
            + self.deliberations
            + self.deliberation_statements
            + self.deliberation_votes
            + self.deliberation_outcomes
            + self.dialects
            + self.recommendations
            + self.verifications
            + self.vocabularies
    }
}

#[cfg(test)]
mod get_deliberation_tests {
    use super::*;
    use idiolect_records::generated::dev::idiolect::defs::StrongRecordRef;
    use idiolect_records::generated::dev::idiolect::deliberation_vote::DeliberationVoteStance;
    use idiolect_records::{
        AnyRecord, Datetime, Deliberation, DeliberationStatement, DeliberationVote,
    };

    fn datetime() -> Datetime {
        Datetime::parse("2026-04-29T00:00:00Z").expect("valid datetime")
    }

    fn cid() -> idiolect_records::Cid {
        idiolect_records::Cid::parse("bafyreidfcm4u3vnuph5ltwdpssiz3a4xfbm2otjrdisftwnbfmnxd6lsxm")
            .expect("valid cid")
    }

    fn at_uri(s: &str) -> idiolect_records::AtUri {
        idiolect_records::AtUri::parse(s).expect("valid at-uri")
    }

    fn deliberation_record() -> Deliberation {
        Deliberation {
            owning_community: at_uri("at://did:plc:c/dev.idiolect.community/main"),
            topic: "Adopt verification policy v2?".to_owned(),
            description: None,
            auth_required: None,
            classification: None,
            classification_vocab: None,
            status: None,
            status_vocab: None,
            closed_at: None,
            outcome: None,
            created_at: datetime(),
        }
    }

    fn statement(deliberation_uri: &str, text: &str) -> DeliberationStatement {
        DeliberationStatement {
            deliberation: StrongRecordRef {
                uri: at_uri(deliberation_uri),
                cid: cid(),
            },
            text: text.to_owned(),
            classification: None,
            classification_vocab: None,
            anonymous: None,
            created_at: datetime(),
        }
    }

    fn vote(statement_uri: &str, stance: DeliberationVoteStance) -> DeliberationVote {
        DeliberationVote {
            subject: StrongRecordRef {
                uri: at_uri(statement_uri),
                cid: cid(),
            },
            stance,
            stance_vocab: None,
            weight: None,
            rationale: None,
            created_at: datetime(),
        }
    }

    #[test]
    fn returns_none_for_unknown_deliberation() {
        let cat = Catalog::new();
        assert!(get_deliberation(&cat, "at://did:plc:c/dev.idiolect.deliberation/nope").is_none());
    }

    #[test]
    fn assembles_deliberation_with_statements_and_votes() {
        let mut cat = Catalog::new();
        let d_uri = "at://did:plc:c/dev.idiolect.deliberation/d1";
        let s1_uri = "at://did:plc:c/dev.idiolect.deliberationStatement/s1";
        let s2_uri = "at://did:plc:c/dev.idiolect.deliberationStatement/s2";
        let v1_uri = "at://did:plc:voter/dev.idiolect.deliberationVote/v1";
        let v2_uri = "at://did:plc:voter/dev.idiolect.deliberationVote/v2";
        cat.upsert(
            d_uri.to_owned(),
            "did:plc:c".to_owned(),
            "rev1".to_owned(),
            AnyRecord::Deliberation(deliberation_record()),
        );
        cat.upsert(
            s1_uri.to_owned(),
            "did:plc:c".to_owned(),
            "rev2".to_owned(),
            AnyRecord::DeliberationStatement(statement(d_uri, "Require formal proofs.")),
        );
        cat.upsert(
            s2_uri.to_owned(),
            "did:plc:c".to_owned(),
            "rev3".to_owned(),
            AnyRecord::DeliberationStatement(statement(d_uri, "Property tests are sufficient.")),
        );
        cat.upsert(
            v1_uri.to_owned(),
            "did:plc:voter".to_owned(),
            "rev4".to_owned(),
            AnyRecord::DeliberationVote(vote(s1_uri, DeliberationVoteStance::Agree)),
        );
        cat.upsert(
            v2_uri.to_owned(),
            "did:plc:voter".to_owned(),
            "rev5".to_owned(),
            AnyRecord::DeliberationVote(vote(s2_uri, DeliberationVoteStance::Disagree)),
        );
        // Add a stray vote on an unrelated statement; must NOT
        // appear in the view.
        let unrelated_s = "at://did:plc:other/dev.idiolect.deliberationStatement/x";
        cat.upsert(
            unrelated_s.to_owned(),
            "did:plc:other".to_owned(),
            "revx".to_owned(),
            AnyRecord::DeliberationStatement(statement(
                "at://did:plc:other/dev.idiolect.deliberation/other",
                "Unrelated.",
            )),
        );
        cat.upsert(
            "at://did:plc:other/dev.idiolect.deliberationVote/x".to_owned(),
            "did:plc:voter".to_owned(),
            "revy".to_owned(),
            AnyRecord::DeliberationVote(vote(unrelated_s, DeliberationVoteStance::Pass)),
        );

        let view = get_deliberation(&cat, d_uri).expect("deliberation present");
        assert_eq!(
            view.deliberation.record.topic,
            "Adopt verification policy v2?"
        );
        assert_eq!(view.statements.len(), 2);
        assert_eq!(view.votes.len(), 2);
        let stances: Vec<_> = view
            .votes
            .iter()
            .map(|v| v.record.stance.as_str())
            .collect();
        assert!(stances.contains(&"agree"));
        assert!(stances.contains(&"disagree"));
        assert!(view.outcomes.is_empty());
    }
}
