//! Evaluator for condition and eligibility predicate trees.
//!
//! Condition and eligibility trees are stored in postfix-operator
//! form on records: a list whose items are either atomic predicates
//! or combinators (and/or/not) that pop operands from a running
//! stack. `eval_condition_tree` interprets that stack against a
//! caller-supplied [`ConditionContext`] using the theory-resolver
//! for subsumption lookups; `eval_eligibility_tree` does the same
//! for bounty eligibility against a claimer DID.
//!
//! Trees that fail to parse (malformed stack, dangling operands) are
//! reported as [`PredicateError::Malformed`]; subsumption lookups
//! against unregistered vocabularies are reported as
//! [`PredicateError::UnresolvedVocabulary`] so callers can
//! distinguish "the predicate doesn't hold" from "we don't know
//! enough yet to say."

use idiolect_records::generated::dev::idiolect::bounty::{
    BountyEligibility, EligibilityDid, EligibilityMember, EligibilityVerificationFor,
};
use idiolect_records::generated::dev::idiolect::defs::{LensProperty, VocabRef};
use idiolect_records::generated::dev::idiolect::recommendation::{
    ConditionActionSubsumedBy, ConditionDataHas, ConditionPurposeSubsumedBy, ConditionSourceIs,
    ConditionTargetIs, RecommendationConditions, RecommendationPreconditions,
};

use crate::theory_resolver::Resolver;

/// Context against which a condition tree evaluates. Fields are all
/// optional — callers supply whatever context they have and the
/// evaluator returns `false` / `UnresolvedVocabulary` when a
/// predicate needs something the context is missing.
#[derive(Debug, Clone, Default)]
pub struct ConditionContext {
    /// Source schema of the invocation.
    pub source_schema: Option<String>,
    /// Target schema of the invocation.
    pub target_schema: Option<String>,
    /// Invocation's `use.action`, if any.
    pub action: Option<String>,
    /// Invocation's `use.purpose`, if any.
    pub purpose: Option<String>,
    /// Community in which the invocation occurred.
    pub community: Option<String>,
    /// Properties of the data being acted on (e.g. "contains-pii",
    /// "length>1024"). Used by `conditionDataHas`.
    pub data_properties: Vec<String>,
    /// Default action vocabulary for `action_subsumed_by`.
    pub action_vocabulary: Option<String>,
    /// Default purpose vocabulary for `purpose_subsumed_by`.
    pub purpose_vocabulary: Option<String>,
}

/// Evaluation outcome for a predicate tree against a context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PredicateResult {
    /// Tree holds against the context.
    Holds,
    /// Tree does not hold against the context.
    DoesNotHold,
    /// Could not decide — a subsumption check referenced a
    /// vocabulary not registered with the resolver. Caller may
    /// retry after registering the missing vocabulary.
    Unresolved {
        /// Short tag identifying the missing input.
        reason: String,
    },
}

/// Errors surfaced by the predicate evaluator. These are structural
/// problems with the tree itself — the caller gave us malformed data
/// — and should be reported as 4xx rather than "doesn't hold."
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PredicateError {
    /// Stack imbalance: a combinator popped more operands than the
    /// running stack held, or the final stack has anything other
    /// than one predicate value on top.
    #[error("malformed predicate tree: {0}")]
    Malformed(String),
}

/// Claimant for eligibility evaluation. DIDs this claimant asserts
/// membership in and the typed lens properties they hold
/// verifications for are looked up against the catalog by the
/// caller; here we just take the resolved lists.
#[derive(Debug, Clone, Default)]
pub struct Claimer {
    /// DID of the claimant.
    pub did: String,
    /// Communities the claimant is currently a member of.
    pub memberships: Vec<String>,
    /// Typed `LensProperty` values the claimant has a verification
    /// for. The eligibility evaluator matches these against the
    /// eligibility node's `property` with the same structural
    /// semantics as [`crate::query::requirement_matches`] — variant
    /// must agree, and pinned fields must agree.
    pub verified_properties: Vec<LensProperty>,
}

/// Evaluate a `conditions` tree on a recommendation against a
/// [`ConditionContext`]. The resolver supplies subsumption
/// semantics for the action and purpose dimensions.
///
/// # Errors
///
/// Returns [`PredicateError::Malformed`] if the tree is not a valid
/// postfix expression (missing operands, unbalanced stack).
pub fn eval_condition_tree(
    tree: &[RecommendationConditions],
    ctx: &ConditionContext,
    resolver: &Resolver,
) -> Result<PredicateResult, PredicateError> {
    let mut stack: Vec<PredicateResult> = Vec::new();
    for node in tree {
        match node {
            RecommendationConditions::ConditionSourceIs(n) => {
                stack.push(source_is(n, ctx));
            }
            RecommendationConditions::ConditionTargetIs(n) => {
                stack.push(target_is(n, ctx));
            }
            RecommendationConditions::ConditionActionSubsumedBy(n) => {
                stack.push(action_subsumed_by(n, ctx, resolver));
            }
            RecommendationConditions::ConditionPurposeSubsumedBy(n) => {
                stack.push(purpose_subsumed_by(n, ctx, resolver));
            }
            RecommendationConditions::ConditionDataHas(n) => {
                stack.push(data_has(n, ctx));
            }
            RecommendationConditions::ConditionAnd(_) => {
                let (r, l) = pop_two(&mut stack, "and")?;
                stack.push(combine_and(l, r));
            }
            RecommendationConditions::ConditionOr(_) => {
                let (r, l) = pop_two(&mut stack, "or")?;
                stack.push(combine_or(l, r));
            }
            RecommendationConditions::ConditionNot(_) => {
                let v = pop_one(&mut stack, "not")?;
                stack.push(combine_not(v));
            }
        }
    }
    stack_result(stack)
}

/// Evaluate a `preconditions` tree. Same shape as
/// [`eval_condition_tree`] — preconditions is a separate union type
/// in the generated code but identical in structure.
///
/// # Errors
///
/// See [`eval_condition_tree`].
pub fn eval_precondition_tree(
    tree: &[RecommendationPreconditions],
    ctx: &ConditionContext,
    resolver: &Resolver,
) -> Result<PredicateResult, PredicateError> {
    // Reuse the same machinery by aliasing each variant.
    let mut stack: Vec<PredicateResult> = Vec::new();
    for node in tree {
        match node {
            RecommendationPreconditions::ConditionSourceIs(n) => {
                stack.push(source_is(n, ctx));
            }
            RecommendationPreconditions::ConditionTargetIs(n) => {
                stack.push(target_is(n, ctx));
            }
            RecommendationPreconditions::ConditionActionSubsumedBy(n) => {
                stack.push(action_subsumed_by(n, ctx, resolver));
            }
            RecommendationPreconditions::ConditionPurposeSubsumedBy(n) => {
                stack.push(purpose_subsumed_by(n, ctx, resolver));
            }
            RecommendationPreconditions::ConditionDataHas(n) => {
                stack.push(data_has(n, ctx));
            }
            RecommendationPreconditions::ConditionAnd(_) => {
                let (r, l) = pop_two(&mut stack, "and")?;
                stack.push(combine_and(l, r));
            }
            RecommendationPreconditions::ConditionOr(_) => {
                let (r, l) = pop_two(&mut stack, "or")?;
                stack.push(combine_or(l, r));
            }
            RecommendationPreconditions::ConditionNot(_) => {
                let v = pop_one(&mut stack, "not")?;
                stack.push(combine_not(v));
            }
        }
    }
    stack_result(stack)
}

/// Evaluate an eligibility tree against a claimer.
///
/// # Errors
///
/// See [`eval_condition_tree`].
pub fn eval_eligibility_tree(
    tree: &[BountyEligibility],
    claimer: &Claimer,
) -> Result<PredicateResult, PredicateError> {
    let mut stack: Vec<PredicateResult> = Vec::new();
    for node in tree {
        match node {
            BountyEligibility::EligibilityMember(n) => {
                stack.push(is_member_of(n, claimer));
            }
            BountyEligibility::EligibilityVerificationFor(n) => {
                stack.push(has_verification_for(n, claimer));
            }
            BountyEligibility::EligibilityDid(n) => {
                stack.push(is_did(n, claimer));
            }
            BountyEligibility::EligibilityAnd(_) => {
                let (r, l) = pop_two(&mut stack, "and")?;
                stack.push(combine_and(l, r));
            }
            BountyEligibility::EligibilityOr(_) => {
                let (r, l) = pop_two(&mut stack, "or")?;
                stack.push(combine_or(l, r));
            }
            BountyEligibility::EligibilityNot(_) => {
                let v = pop_one(&mut stack, "not")?;
                stack.push(combine_not(v));
            }
        }
    }
    stack_result(stack)
}

// -------------------------------------------------------------------
// Atomic evaluators
// -------------------------------------------------------------------

fn source_is(n: &ConditionSourceIs, ctx: &ConditionContext) -> PredicateResult {
    match (ctx.source_schema.as_ref(), n.schema.uri.as_ref()) {
        (Some(a), Some(b)) if a == b => PredicateResult::Holds,
        _ => PredicateResult::DoesNotHold,
    }
}

fn target_is(n: &ConditionTargetIs, ctx: &ConditionContext) -> PredicateResult {
    match (ctx.target_schema.as_ref(), n.schema.uri.as_ref()) {
        (Some(a), Some(b)) if a == b => PredicateResult::Holds,
        _ => PredicateResult::DoesNotHold,
    }
}

fn action_subsumed_by(
    n: &ConditionActionSubsumedBy,
    ctx: &ConditionContext,
    resolver: &Resolver,
) -> PredicateResult {
    let Some(ctx_action) = ctx.action.as_deref() else {
        return PredicateResult::DoesNotHold;
    };
    let vocab = vocab_uri_from(n.vocabulary.as_ref(), ctx.action_vocabulary.as_deref());
    match resolver.subsumed_by(vocab, ctx_action, &n.action) {
        Some(true) => PredicateResult::Holds,
        Some(false) => PredicateResult::DoesNotHold,
        None => PredicateResult::Unresolved {
            reason: format!(
                "action vocabulary not registered: {}",
                vocab.unwrap_or("<none>")
            ),
        },
    }
}

fn purpose_subsumed_by(
    n: &ConditionPurposeSubsumedBy,
    ctx: &ConditionContext,
    resolver: &Resolver,
) -> PredicateResult {
    let Some(ctx_purpose) = ctx.purpose.as_deref() else {
        return PredicateResult::DoesNotHold;
    };
    let vocab = vocab_uri_from(n.vocabulary.as_ref(), ctx.purpose_vocabulary.as_deref());
    match resolver.subsumed_by(vocab, ctx_purpose, &n.purpose) {
        Some(true) => PredicateResult::Holds,
        Some(false) => PredicateResult::DoesNotHold,
        None => PredicateResult::Unresolved {
            reason: format!(
                "purpose vocabulary not registered: {}",
                vocab.unwrap_or("<none>")
            ),
        },
    }
}

fn data_has(n: &ConditionDataHas, ctx: &ConditionContext) -> PredicateResult {
    if ctx.data_properties.contains(&n.property) {
        PredicateResult::Holds
    } else {
        PredicateResult::DoesNotHold
    }
}

fn is_member_of(n: &EligibilityMember, claimer: &Claimer) -> PredicateResult {
    if claimer.memberships.contains(&n.community) {
        PredicateResult::Holds
    } else {
        PredicateResult::DoesNotHold
    }
}

fn has_verification_for(n: &EligibilityVerificationFor, claimer: &Claimer) -> PredicateResult {
    // Structural match on LensProperty variant + pinned fields, same
    // rules as `query::requirement_matches` (empty string fields on
    // the requirement match any published value). This lets callers
    // hand us the claimant's verifications directly as typed values
    // — no string-tag encoding required.
    if claimer
        .verified_properties
        .iter()
        .any(|held| lens_property_matches(&n.property, held))
    {
        PredicateResult::Holds
    } else {
        PredicateResult::DoesNotHold
    }
}

/// Whether a claimant's held `LensProperty` satisfies the eligibility
/// node's required property. Mirrors the requirement/property pairing
/// in [`crate::query::requirement_matches`] but operates on two
/// [`LensProperty`] values so callers don't have to route through the
/// `RecommendationRequiredVerifications` union.
fn lens_property_matches(required: &LensProperty, held: &LensProperty) -> bool {
    use crate::query::{opt_str_wildcard_eq, str_wildcard_eq};
    match (required, held) {
        (LensProperty::LpRoundtrip(req), LensProperty::LpRoundtrip(h)) => {
            str_wildcard_eq(&req.domain, &h.domain)
        }
        (LensProperty::LpGenerator(req), LensProperty::LpGenerator(h)) => {
            str_wildcard_eq(&req.spec, &h.spec)
                && opt_str_wildcard_eq(req.runner.as_deref(), h.runner.as_deref())
        }
        (LensProperty::LpTheorem(req), LensProperty::LpTheorem(h)) => {
            str_wildcard_eq(&req.statement, &h.statement)
                && opt_str_wildcard_eq(req.system.as_deref(), h.system.as_deref())
        }
        (LensProperty::LpConformance(req), LensProperty::LpConformance(h)) => {
            str_wildcard_eq(&req.standard, &h.standard) && str_wildcard_eq(&req.version, &h.version)
        }
        (LensProperty::LpChecker(req), LensProperty::LpChecker(h)) => {
            str_wildcard_eq(&req.checker, &h.checker)
                && opt_str_wildcard_eq(req.ruleset.as_deref(), h.ruleset.as_deref())
        }
        (LensProperty::LpConvergence(req), LensProperty::LpConvergence(h)) => {
            str_wildcard_eq(&req.property, &h.property)
        }
        _ => false,
    }
}

fn is_did(n: &EligibilityDid, claimer: &Claimer) -> PredicateResult {
    if claimer.did == n.did {
        PredicateResult::Holds
    } else {
        PredicateResult::DoesNotHold
    }
}

// -------------------------------------------------------------------
// Combinators
// -------------------------------------------------------------------

fn combine_and(l: PredicateResult, r: PredicateResult) -> PredicateResult {
    use PredicateResult::{DoesNotHold, Holds, Unresolved};
    match (l, r) {
        (Holds, Holds) => Holds,
        (DoesNotHold, _) | (_, DoesNotHold) => DoesNotHold,
        (Unresolved { reason }, _) | (_, Unresolved { reason }) => Unresolved { reason },
    }
}

fn combine_or(l: PredicateResult, r: PredicateResult) -> PredicateResult {
    use PredicateResult::{DoesNotHold, Holds, Unresolved};
    match (l, r) {
        (Holds, _) | (_, Holds) => Holds,
        (DoesNotHold, DoesNotHold) => DoesNotHold,
        (Unresolved { reason }, _) | (_, Unresolved { reason }) => Unresolved { reason },
    }
}

fn combine_not(v: PredicateResult) -> PredicateResult {
    use PredicateResult::{DoesNotHold, Holds, Unresolved};
    match v {
        Holds => DoesNotHold,
        DoesNotHold => Holds,
        Unresolved { reason } => Unresolved { reason },
    }
}

// -------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------

fn pop_two(
    stack: &mut Vec<PredicateResult>,
    label: &str,
) -> Result<(PredicateResult, PredicateResult), PredicateError> {
    let right = stack
        .pop()
        .ok_or_else(|| PredicateError::Malformed(format!("`{label}` missing right operand")))?;
    let left = stack
        .pop()
        .ok_or_else(|| PredicateError::Malformed(format!("`{label}` missing left operand")))?;
    Ok((right, left))
}

fn pop_one(
    stack: &mut Vec<PredicateResult>,
    label: &str,
) -> Result<PredicateResult, PredicateError> {
    stack
        .pop()
        .ok_or_else(|| PredicateError::Malformed(format!("`{label}` missing operand")))
}

fn stack_result(mut stack: Vec<PredicateResult>) -> Result<PredicateResult, PredicateError> {
    match stack.len() {
        0 => Ok(PredicateResult::Holds), // empty tree: trivially applies
        1 => Ok(stack.pop().unwrap()),
        n => Err(PredicateError::Malformed(format!(
            "tree leaves {n} values on stack, expected exactly 1"
        ))),
    }
}

fn vocab_uri_from<'a>(
    node_vocab: Option<&'a VocabRef>,
    ctx_default: Option<&'a str>,
) -> Option<&'a str> {
    node_vocab.and_then(|v| v.uri.as_deref()).or(ctx_default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use idiolect_records::generated::dev::idiolect::defs::SchemaRef;
    use idiolect_records::generated::dev::idiolect::recommendation::{ConditionAnd, ConditionNot, ConditionOr};

    fn ctx_with_schemas(src: &str, tgt: &str) -> ConditionContext {
        ConditionContext {
            source_schema: Some(src.into()),
            target_schema: Some(tgt.into()),
            ..Default::default()
        }
    }

    fn source_is_node(uri: &str) -> RecommendationConditions {
        RecommendationConditions::ConditionSourceIs(ConditionSourceIs {
            schema: SchemaRef {
                cid: None,
                language: None,
                uri: Some(uri.into()),
            },
        })
    }

    fn target_is_node(uri: &str) -> RecommendationConditions {
        RecommendationConditions::ConditionTargetIs(ConditionTargetIs {
            schema: SchemaRef {
                cid: None,
                language: None,
                uri: Some(uri.into()),
            },
        })
    }

    #[test]
    fn empty_tree_trivially_holds() {
        let r = eval_condition_tree(&[], &ConditionContext::default(), &Resolver::new()).unwrap();
        assert_eq!(r, PredicateResult::Holds);
    }

    #[test]
    fn single_source_is_evaluates_against_ctx() {
        let ctx = ctx_with_schemas(
            "at://did:plc:x/dev.panproto.schema.schema/s",
            "at://did:plc:x/dev.panproto.schema.schema/t",
        );
        let tree = vec![source_is_node(
            "at://did:plc:x/dev.panproto.schema.schema/s",
        )];
        assert_eq!(
            eval_condition_tree(&tree, &ctx, &Resolver::new()).unwrap(),
            PredicateResult::Holds
        );
        let miss = vec![source_is_node(
            "at://did:plc:x/dev.panproto.schema.schema/y",
        )];
        assert_eq!(
            eval_condition_tree(&miss, &ctx, &Resolver::new()).unwrap(),
            PredicateResult::DoesNotHold
        );
    }

    #[test]
    fn and_combines_two_atoms() {
        let ctx = ctx_with_schemas("s", "t");
        let s = SchemaRef {
            cid: None,
            language: None,
            uri: Some("s".into()),
        };
        let t = SchemaRef {
            cid: None,
            language: None,
            uri: Some("t".into()),
        };
        let tree = vec![
            RecommendationConditions::ConditionSourceIs(ConditionSourceIs { schema: s.clone() }),
            RecommendationConditions::ConditionTargetIs(ConditionTargetIs { schema: t.clone() }),
            RecommendationConditions::ConditionAnd(ConditionAnd {}),
        ];
        assert_eq!(
            eval_condition_tree(&tree, &ctx, &Resolver::new()).unwrap(),
            PredicateResult::Holds
        );
        // Wrong target: the conjunction fails.
        let wrong = vec![
            RecommendationConditions::ConditionSourceIs(ConditionSourceIs { schema: s }),
            RecommendationConditions::ConditionTargetIs(ConditionTargetIs {
                schema: SchemaRef {
                    cid: None,
                    language: None,
                    uri: Some("other".into()),
                },
            }),
            RecommendationConditions::ConditionAnd(ConditionAnd {}),
        ];
        assert_eq!(
            eval_condition_tree(&wrong, &ctx, &Resolver::new()).unwrap(),
            PredicateResult::DoesNotHold
        );
    }

    #[test]
    fn not_flips_result() {
        let ctx = ctx_with_schemas("s", "t");
        let tree = vec![
            source_is_node("s"),
            RecommendationConditions::ConditionNot(ConditionNot {}),
        ];
        assert_eq!(
            eval_condition_tree(&tree, &ctx, &Resolver::new()).unwrap(),
            PredicateResult::DoesNotHold
        );
    }

    #[test]
    fn or_short_circuits_on_holds() {
        let ctx = ctx_with_schemas("s", "t");
        let tree = vec![
            source_is_node("wrong"),
            target_is_node("t"),
            RecommendationConditions::ConditionOr(ConditionOr {}),
        ];
        assert_eq!(
            eval_condition_tree(&tree, &ctx, &Resolver::new()).unwrap(),
            PredicateResult::Holds
        );
    }

    #[test]
    fn malformed_tree_errors() {
        let tree = vec![RecommendationConditions::ConditionAnd(ConditionAnd {})];
        assert!(matches!(
            eval_condition_tree(&tree, &ConditionContext::default(), &Resolver::new()),
            Err(PredicateError::Malformed(_))
        ));
    }
}
