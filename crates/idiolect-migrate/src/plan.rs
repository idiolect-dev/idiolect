//! Migration planning: diff classification + lens auto-derivation.

use panproto_lens::auto_lens::{AutoLensConfig, auto_generate};
use panproto_lens::protolens::ProtolensChain;
use panproto_schema::{Protocol, Schema};

use crate::error::PlannerError;

pub use panproto_check::{
    CompatReport, SchemaDiff, classify as classify_diff, diff as compute_diff,
};

/// Classify the diff between two schemas through `panproto-check`.
///
/// Thin wrapper: callers that already hold two `Schema` values can
/// call this in one step rather than chaining `diff` + `classify`.
#[must_use]
pub fn classify(old: &Schema, new: &Schema, protocol: &Protocol) -> CompatReport {
    let d = compute_diff(old, new);
    classify_diff(&d, protocol)
}

/// Packaged migration ready for publication.
///
/// The caller publishes this as a `dev.panproto.schema.lens` record
/// whose `blob` field is [`protolens_chain`](Self::protolens_chain)
/// serialized to JSON. Callers that hash-address schemas compute
/// `source_schema_hash` and `target_schema_hash` at schema-emit time;
/// this crate does not hash, because the hashing rule lives in the
/// deployment's schema-storage layer.
#[derive(Debug, Clone)]
pub struct MigrationPlan {
    /// Content-addressed hash of the schema records are migrating
    /// *from*.
    pub source_schema_hash: String,
    /// Content-addressed hash of the schema records are migrating
    /// *to*.
    pub target_schema_hash: String,
    /// The protolens chain that translates records from source to
    /// target. Serialize into a `PanprotoLens.blob` field when
    /// publishing.
    pub protolens_chain: ProtolensChain,
    /// Alignment quality reported by `panproto_lens::auto_generate`
    /// (0.0 to 1.0). Low scores — below ~0.6 — suggest the caller
    /// should review and possibly hand-author a lens rather than
    /// publish the auto-derived one.
    pub alignment_quality: f64,
}

/// Attempt to auto-derive a [`MigrationPlan`] translating records
/// from `old` to `new`.
///
/// Delegates to `panproto_lens::auto_generate`. On compatibility,
/// returns an `Err` indicating no plan is needed.
///
/// # Errors
///
/// - [`PlannerError::NoChange`] — schemas are structurally identical
///   after diffing.
/// - [`PlannerError::OnlyNonBreaking`] — diff exists but no breaking
///   changes; readers of the old schema keep working.
/// - [`PlannerError::NotAutoDerivable`] — breaking changes exist and
///   `auto_generate` could not produce a lens; the returned list
///   names each change the caller must address manually.
pub fn plan_auto(
    old: &Schema,
    new: &Schema,
    protocol: &Protocol,
    source_schema_hash: impl Into<String>,
    target_schema_hash: impl Into<String>,
) -> Result<MigrationPlan, PlannerError> {
    let diff = compute_diff(old, new);
    let report = classify_diff(&diff, protocol);
    if report.breaking.is_empty() && report.non_breaking.is_empty() {
        return Err(PlannerError::NoChange);
    }
    if report.breaking.is_empty() {
        return Err(PlannerError::OnlyNonBreaking);
    }

    let config = AutoLensConfig::default();
    if let Ok(result) = auto_generate(old, new, protocol, &config) {
        Ok(MigrationPlan {
            source_schema_hash: source_schema_hash.into(),
            target_schema_hash: target_schema_hash.into(),
            protolens_chain: result.chain,
            alignment_quality: result.alignment_quality,
        })
    } else {
        // auto_generate's failure modes are opaque by design —
        // translate into an actionable list of breaking changes
        // the caller must resolve by hand.
        let reasons: Vec<String> = report.breaking.iter().map(|b| format!("{b:?}")).collect();
        Err(PlannerError::NotAutoDerivable(reasons))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use panproto_schema::SchemaBuilder;

    fn test_protocol() -> Protocol {
        Protocol::default()
    }

    fn schema_with_field(kind: &str) -> Schema {
        SchemaBuilder::new(&test_protocol())
            .entry("body")
            .vertex("body", "object", None)
            .unwrap()
            .vertex("body.text", kind, None)
            .unwrap()
            .edge("body", "body.text", "prop", Some("text"))
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    fn classify_reports_no_change_when_identical() {
        let s = schema_with_field("string");
        let report = classify(&s, &s, &test_protocol());
        assert!(report.compatible);
        assert!(report.breaking.is_empty());
        assert!(report.non_breaking.is_empty());
    }

    #[test]
    fn plan_auto_rejects_identical_schemas() {
        let s = schema_with_field("string");
        let err = plan_auto(&s, &s, &test_protocol(), "h1", "h1").unwrap_err();
        assert!(matches!(err, PlannerError::NoChange));
    }
}
