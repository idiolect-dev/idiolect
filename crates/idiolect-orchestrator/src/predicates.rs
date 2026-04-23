//! Filter semantics referenced by `orchestrator-spec/queries.json`.
//!
//! Every predicate named in the spec resolves to a `pub fn` here; the
//! generated query fns and HTTP handlers call into this module and
//! contain no business logic of their own.
//!
//! Also houses:
//!
//! - Shared reference-comparison helpers ([`schema_refs_match`],
//!   [`lens_refs_match`]) used across multiple predicates.
//! - Enum parsers ([`parse_verification_kind`],
//!   [`parse_adapter_invocation_protocol_kind`], [`parse_vocab_world`])
//!   that the generated HTTP handlers invoke to convert query-string
//!   tokens into typed parameters. Signature:
//!   `fn(&str) -> Result<T, String>`.
//!
//! Conventions:
//!
//! - One predicate fn per spec `predicate:` name.
//! - Predicates take `(&Record, …params)` and return `bool`.
//! - Predicates are `pub fn` so downstream tests and custom
//!   orchestrator frontends can call them directly.

use idiolect_records::generated::adapter::AdapterInvocationProtocolKind;
use idiolect_records::generated::bounty::{BountyStatus, BountyWants};
use idiolect_records::generated::defs::{LensRef, SchemaRef};
use idiolect_records::generated::verification::VerificationKind;
use idiolect_records::generated::vocab::VocabWorld;
use idiolect_records::{
    Adapter, Belief, Bounty, Community, Dialect, Recommendation, Verification, Vocab,
};

// -----------------------------------------------------------------
// Reference-matching fragments.
// -----------------------------------------------------------------

/// Decide whether two [`SchemaRef`]s designate the same schema.
///
/// Preference order: content-addressed cid match beats at-uri match.
/// A cid match is definitive (content-addressed equality); an at-uri
/// match is a weaker identity but still what callers usually want
/// when a cid is absent.
#[must_use]
pub fn schema_refs_match(a: &SchemaRef, b: &SchemaRef) -> bool {
    if let (Some(ca), Some(cb)) = (&a.cid, &b.cid) {
        return ca == cb;
    }
    if let (Some(ua), Some(ub)) = (&a.uri, &b.uri) {
        return ua == ub;
    }
    false
}

/// Compare two [`LensRef`]s for identity. Content-addressed cid match
/// beats at-uri match, same policy as [`schema_refs_match`].
#[must_use]
pub fn lens_refs_match(a: &LensRef, b: &LensRef) -> bool {
    if let (Some(ca), Some(cb)) = (&a.cid, &b.cid) {
        return ca == cb;
    }
    if let (Some(ua), Some(ub)) = (&a.uri, &b.uri) {
        return ua == ub;
    }
    false
}

// -----------------------------------------------------------------
// Bounty predicates.
// -----------------------------------------------------------------

/// Status is open, claimed, or unset (the lexicon allows absence, in
/// which case the requester has not yet declared a lifecycle state).
#[must_use]
pub const fn bounty_is_open(b: &Bounty) -> bool {
    matches!(
        b.status,
        None | Some(BountyStatus::Open | BountyStatus::Claimed)
    )
}

/// Bounty is a `WantLens` whose source/target match `source` and
/// `target`. Also requires the bounty to be open — matches the
/// original hand-written query's semantics.
#[must_use]
pub fn bounty_for_want_lens(b: &Bounty, source: &SchemaRef, target: &SchemaRef) -> bool {
    if !bounty_is_open(b) {
        return false;
    }
    matches!(
        &b.wants,
        BountyWants::WantLens(want)
            if schema_refs_match(&want.source, source)
                && schema_refs_match(&want.target, target)
    )
}

/// Bounty authored by the given requester DID.
#[must_use]
pub fn bounty_by_requester(b: &Bounty, requester_did: &str) -> bool {
    b.requester == requester_did
}

// -----------------------------------------------------------------
// Adapter predicates.
// -----------------------------------------------------------------

/// Adapter declared for the given framework. Case-insensitive.
#[must_use]
pub fn adapter_for_framework(a: &Adapter, framework: &str) -> bool {
    a.framework.eq_ignore_ascii_case(framework)
}

/// Adapter whose invocation-protocol kind matches.
#[must_use]
pub fn adapter_by_invocation_protocol(a: &Adapter, kind: &AdapterInvocationProtocolKind) -> bool {
    a.invocation_protocol.kind == *kind
}

// -----------------------------------------------------------------
// Community predicates.
// -----------------------------------------------------------------

/// Community whose `members` list includes `member_did`. Communities
/// using `membershipRoll` (external reference) are not resolved.
#[must_use]
pub fn community_has_member(c: &Community, member_did: &str) -> bool {
    c.members
        .as_ref()
        .is_some_and(|list| list.iter().any(|m| m == member_did))
}

/// Community whose name matches (case-insensitive).
#[must_use]
pub fn community_by_name(c: &Community, name: &str) -> bool {
    c.name.eq_ignore_ascii_case(name)
}

// -----------------------------------------------------------------
// Dialect predicates.
// -----------------------------------------------------------------

/// Dialect whose `owning_community` at-uri matches.
#[must_use]
pub fn dialect_for_community(d: &Dialect, community_uri: &str) -> bool {
    d.owning_community == community_uri
}

// -----------------------------------------------------------------
// Recommendation predicates.
// -----------------------------------------------------------------

/// Recommendation whose `lens_path` is non-empty — i.e. actually
/// recommends anything. See the query spec's
/// `recommendations_starting_from` description for the source-schema
/// filtering caveat (requires lens-record resolution, out of scope
/// for the reference orchestrator).
#[must_use]
pub const fn recommendation_has_path(r: &Recommendation) -> bool {
    !r.lens_path.is_empty()
}

// -----------------------------------------------------------------
// Verification predicates.
// -----------------------------------------------------------------

/// Verification targets the given lens.
#[must_use]
pub fn verification_for_lens(v: &Verification, lens: &LensRef) -> bool {
    lens_refs_match(&v.lens, lens)
}

/// Verification whose `kind` matches.
#[must_use]
pub fn verification_by_kind(v: &Verification, kind: &VerificationKind) -> bool {
    v.kind == *kind
}

// -----------------------------------------------------------------
// Parsers for HTTP query-string → typed param.
// -----------------------------------------------------------------

/// Parse an at-proto verification-kind string into the enum.
///
/// # Errors
///
/// Returns an operator-friendly message when the string is not one of
/// the known kinds; the generated HTTP handler wraps this into a 400
/// `invalid_request` response.
pub fn parse_verification_kind(s: &str) -> Result<VerificationKind, String> {
    Ok(match s {
        "roundtrip-test" => VerificationKind::RoundtripTest,
        "property-test" => VerificationKind::PropertyTest,
        "formal-proof" => VerificationKind::FormalProof,
        "conformance-test" => VerificationKind::ConformanceTest,
        "static-check" => VerificationKind::StaticCheck,
        "convergence-preserving" => VerificationKind::ConvergencePreserving,
        other => return Err(format!("unknown verification kind: {other}")),
    })
}

/// Parse an at-proto adapter-invocation-protocol-kind string into
/// the enum.
///
/// # Errors
///
/// Same shape as [`parse_verification_kind`].
pub fn parse_adapter_invocation_protocol_kind(
    s: &str,
) -> Result<AdapterInvocationProtocolKind, String> {
    Ok(match s {
        "subprocess" => AdapterInvocationProtocolKind::Subprocess,
        "http" => AdapterInvocationProtocolKind::Http,
        "wasm" => AdapterInvocationProtocolKind::Wasm,
        other => return Err(format!("unknown adapter invocation-protocol kind: {other}")),
    })
}

/// Build a [`SchemaRef`] from a plain at-uri. The cid and language
/// fields are left unset — the orchestrator's match semantics
/// (`schema_refs_match`) accept uri-only refs as a weaker identity.
#[must_use]
pub const fn schema_ref_from_uri(uri: String) -> SchemaRef {
    SchemaRef {
        uri: Some(uri),
        cid: None,
        language: None,
    }
}

/// Build a [`LensRef`] from a plain at-uri. Cid and direction left
/// unset.
#[must_use]
pub const fn lens_ref_from_uri(uri: String) -> LensRef {
    LensRef {
        uri: Some(uri),
        cid: None,
        direction: None,
    }
}

// -----------------------------------------------------------------
// Belief predicates.
// -----------------------------------------------------------------

/// Belief records whose subject's at-uri matches `subject_uri`.
#[must_use]
pub fn belief_about_record(b: &Belief, subject_uri: &str) -> bool {
    b.subject.uri == subject_uri
}

/// Belief records whose effective holder (explicit `holder` or, if
/// absent, the repo owner) matches `holder_did`. The repo-owner
/// fallback isn't visible from a `&Belief` alone, so this predicate
/// tests the explicit holder only; callers needing the repo-owner
/// fallback filter over `Entry<Belief>` directly.
#[must_use]
pub fn belief_by_explicit_holder(b: &Belief, holder_did: &str) -> bool {
    b.holder.as_deref() == Some(holder_did)
}

// -----------------------------------------------------------------
// Vocabulary predicates.
// -----------------------------------------------------------------

/// Vocabulary records whose declared `world` matches the given name
/// (`closed-with-default`, `open`, or `hierarchy-closed`).
#[must_use]
pub fn vocab_with_world(v: &Vocab, world: &str) -> bool {
    let rendered = match v.world {
        VocabWorld::ClosedWithDefault => "closed-with-default",
        VocabWorld::Open => "open",
        VocabWorld::HierarchyClosed => "hierarchy-closed",
    };
    rendered == world
}

/// Vocabulary records whose `name` matches.
#[must_use]
pub fn vocab_by_name(v: &Vocab, name: &str) -> bool {
    v.name == name
}

/// Parse a `world` query-string token into the typed
/// [`VocabWorld`]. Unknown tokens surface the raw string.
///
/// # Errors
/// Returns the raw token in an `Err` when it does not match one of
/// the three declared world disciplines.
pub fn parse_vocab_world(s: &str) -> Result<String, String> {
    match s {
        "closed-with-default" | "open" | "hierarchy-closed" => Ok(s.to_owned()),
        other => Err(format!("unknown world discipline: {other}")),
    }
}
