//! Evaluate `panproto-expr` predicates against catalog records.
//!
//! Expression-form queries declared in `orchestrator-spec/queries.json`
//! carry a string like `r.reward != null` instead of a Rust predicate
//! fn name. At codegen time the emitted query fn calls
//! [`eval_bool_against_record`] for each entry with the expression
//! source and the record's JSON body; the record is bound to `r` in
//! the evaluation environment.
//!
//! Runtime cost: one serde serialize + one expression parse + one
//! evaluation per record per query. The expression parse is cached
//! via [`std::sync::OnceLock`] keyed on the source string so each
//! source is tokenized and parsed at most once per process.
//!
//! A parse failure at first use is a programmer error (the spec
//! declared an expression that doesn't compile); it propagates as a
//! `false` return and a warn-level trace. Runtime eval failures
//! (type mismatch, field-not-found, step-limit exceeded) are also
//! logged and return `false`.
//!
//! # Absent-field caveat
//!
//! `panproto-expr`'s field access errors with `FieldNotFound` when a
//! record lacks the requested field. Record types in
//! `idiolect-records` use `#[serde(skip_serializing_if = "Option::is_none")]`
//! on optional fields, so `None` values are absent from the
//! serialized JSON. Write expressions against fields that are
//! always present (non-optional fields, or optional fields the
//! record always sets in the path of interest). The predicate form
//! remains available for queries that need null-checking on
//! optional fields.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use panproto_expr::{Env, EvalConfig, Expr, Literal, eval};
use panproto_expr_parser::{parse as parse_expr, tokenize};

/// Convert a `serde_json::Value` into a `panproto_expr::Literal`.
///
/// JSON null maps to `Literal::Null`; absent-field access in
/// `panproto-expr` also yields `Null`, so the two are
/// interchangeable from the expression's point of view.
fn json_to_literal(v: &serde_json::Value) -> Literal {
    match v {
        serde_json::Value::Null => Literal::Null,
        serde_json::Value::Bool(b) => Literal::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Literal::Int(i)
            } else if let Some(f) = n.as_f64() {
                Literal::Float(f)
            } else {
                Literal::Str(n.to_string())
            }
        }
        serde_json::Value::String(s) => Literal::Str(s.clone()),
        serde_json::Value::Array(items) => {
            Literal::List(items.iter().map(json_to_literal).collect())
        }
        serde_json::Value::Object(obj) => Literal::Record(
            obj.iter()
                .map(|(k, v)| (std::sync::Arc::from(k.as_str()), json_to_literal(v)))
                .collect(),
        ),
    }
}

/// Per-process cache of parsed expressions. Keyed by source string.
fn expr_cache() -> &'static RwLock<HashMap<String, Result<Expr, String>>> {
    static CACHE: OnceLock<RwLock<HashMap<String, Result<Expr, String>>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn parse_cached(source: &str) -> Result<Expr, String> {
    // Fast path: read-lock lookup.
    {
        let cache = expr_cache().read().expect("expr cache poisoned");
        if let Some(entry) = cache.get(source) {
            return entry.clone();
        }
    }

    // Slow path: tokenize + parse + store.
    let tokens = tokenize(source).map_err(|e| format!("tokenize: {e:?}"))?;
    let parsed = parse_expr(&tokens).map_err(|errs| format!("parse: {errs:?}"))?;
    let result = Ok(parsed);
    {
        let mut cache = expr_cache().write().expect("expr cache poisoned");
        cache.insert(source.to_owned(), result.clone());
    }
    result
}

/// Evaluate `expression` against `record`, returning its truthiness
/// as a `bool`.
///
/// Bindings: `r` is the record, serialized via `serde_json::to_value`
/// and converted into a `Literal::Record`. The expression source can
/// traverse fields with `r.field.sub`, call the ~50 builtin ops, and
/// compare against literals.
///
/// # Error handling
///
/// Parse failures and eval failures log a `warn` with the source and
/// return `false` — expression-form queries filter conservatively on
/// error rather than halting the request. Callers that want hard
/// failure should surface the error by converting to a predicate-form
/// query and raising in Rust.
#[must_use]
pub fn eval_bool_against_record<R: serde::Serialize>(expression: &str, record: &R) -> bool {
    let value = match serde_json::to_value(record) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(expression, error = %e, "expression-form filter: record serialize failed");
            return false;
        }
    };
    let expr = match parse_cached(expression) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(expression, error = %e, "expression-form filter: parse failed");
            return false;
        }
    };
    let env = Env::new().extend(std::sync::Arc::from("r"), json_to_literal(&value));
    match eval(&expr, &env, &EvalConfig::default()) {
        Ok(Literal::Bool(b)) => b,
        Ok(other) => {
            tracing::warn!(
                expression,
                result_kind = other.type_name(),
                "expression-form filter: non-bool result"
            );
            false
        }
        Err(e) => {
            tracing::warn!(expression, error = ?e, "expression-form filter: eval failed");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use idiolect_records::Bounty;
    use idiolect_records::generated::dev::idiolect::bounty::{
        BountyReward, BountyStatus, BountyWants, WantAdapter,
    };

    fn bounty_with_reward(summary: Option<&str>) -> Bounty {
        Bounty {
            basis: None,
            constraints: None,
            eligibility: None,
            fulfillment: None,
            occurred_at: idiolect_records::Datetime::parse("2026-04-21T00:00:00Z")
                .expect("valid datetime"),
            requester: idiolect_records::Did::parse("did:plc:alice").expect("valid DID"),
            reward: summary.map(|s| BountyReward {
                external_ref: None,
                summary: Some(s.to_owned()),
            }),
            status: Some(BountyStatus::Open),
            status_vocab: None,
            wants: BountyWants::WantAdapter(WantAdapter {
                framework: "hasura".into(),
                version_range: None,
            }),
        }
    }

    #[test]
    fn equality_against_string_literal() {
        let b = bounty_with_reward(Some("grant-xyz"));
        assert!(eval_bool_against_record(
            "r.reward.summary == \"grant-xyz\"",
            &b
        ));
        assert!(!eval_bool_against_record(
            "r.reward.summary == \"other\"",
            &b
        ));
    }

    #[test]
    fn equality_against_always_present_field() {
        let b = bounty_with_reward(Some("irrelevant"));
        assert!(eval_bool_against_record(
            "r.requester == \"did:plc:alice\"",
            &b
        ));
        assert!(!eval_bool_against_record(
            "r.requester == \"did:plc:other\"",
            &b
        ));
    }

    #[test]
    fn field_not_found_returns_false() {
        // Accessing an optional field that's `None` hits `FieldNotFound`
        // at eval time; the helper returns `false` rather than panicking.
        let b = bounty_with_reward(None);
        assert!(!eval_bool_against_record("r.reward.summary == \"x\"", &b));
    }

    #[test]
    fn bogus_expression_returns_false() {
        let b = bounty_with_reward(None);
        assert!(!eval_bool_against_record(
            "this is not a valid expression",
            &b
        ));
    }

    #[test]
    fn non_bool_result_returns_false() {
        let b = bounty_with_reward(Some("x"));
        // r.requester is a string; returning it directly is non-bool.
        assert!(!eval_bool_against_record("r.requester", &b));
    }
}
