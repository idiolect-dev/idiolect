//! Reference [`ObservationMethod`]: verification coverage per lens.
//!
//! For every lens referenced by a verification record, tallies
//! verifications by [`VerificationKind`](idiolect_records::generated::dev::idiolect::verification::VerificationKind)
//! and [`VerificationResult`](idiolect_records::generated::dev::idiolect::verification::VerificationResult).
//! Output enables a downstream orchestrator to decide whether a lens
//! has adequate formal-channel evidence before recommending it (P3 —
//! observation precedes declaration, where the formal channel is one
//! of the observation inputs).
//!
//! As with every reference method, this is intentionally lightweight:
//! it does not dedupe by verifier, cross-check proof artifacts, or
//! score trustworthiness. A production observer would layer that on.

use std::collections::BTreeMap;

use idiolect_indexer::IndexerEvent;
use idiolect_records::AnyRecord;
use idiolect_records::generated::dev::idiolect::observation::{
    ObservationMethod as ObservationMethodDescriptor, ObservationScope,
};
use idiolect_records::generated::dev::idiolect::verification::{
    VerificationKind, VerificationResult,
};

use crate::error::ObserverResult;
use crate::method::ObservationMethod;

/// Canonical method name.
pub const METHOD_NAME: &str = "verification-coverage";

/// Method version.
pub const METHOD_VERSION: &str = "1.0.0";

/// Per-lens verification histograms.
#[derive(Debug, Default, Clone)]
struct LensCoverage {
    /// Total verifications against this lens.
    total: u64,
    /// Counts by verification kind.
    by_kind: BTreeMap<&'static str, u64>,
    /// Counts by result.
    by_result: BTreeMap<&'static str, u64>,
    /// Distinct verifier DIDs, for a coarse "independent attestations"
    /// signal. `BTreeSet` would be heavier; we use a map-as-set so the
    /// value can be the attestation count per verifier if a richer
    /// method ever wants it.
    verifiers: BTreeMap<String, u64>,
}

/// Aggregator: counts verifications per lens by kind, result,
/// and (roughly) independence.
#[derive(Debug, Default, Clone)]
pub struct VerificationCoverageMethod {
    /// Per-lens counters, keyed by lens at-uri (or cid fallback).
    lenses: BTreeMap<String, LensCoverage>,
}

impl VerificationCoverageMethod {
    /// Construct an empty aggregator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Serialize [`VerificationKind`] into its kebab-case wire form.
    const fn kind_key(kind: VerificationKind) -> &'static str {
        match kind {
            VerificationKind::RoundtripTest => "roundtrip-test",
            VerificationKind::PropertyTest => "property-test",
            VerificationKind::FormalProof => "formal-proof",
            VerificationKind::ConformanceTest => "conformance-test",
            VerificationKind::StaticCheck => "static-check",
            VerificationKind::ConvergencePreserving => "convergence-preserving",
            VerificationKind::CoercionLaw => "coercion-law",
        }
    }

    /// Serialize [`VerificationResult`] into its kebab-case wire form.
    const fn result_key(result: VerificationResult) -> &'static str {
        match result {
            VerificationResult::Holds => "holds",
            VerificationResult::Falsified => "falsified",
            VerificationResult::Inconclusive => "inconclusive",
        }
    }
}

impl ObservationMethod for VerificationCoverageMethod {
    fn name(&self) -> &str {
        METHOD_NAME
    }

    fn version(&self) -> &str {
        METHOD_VERSION
    }

    fn descriptor(&self) -> ObservationMethodDescriptor {
        ObservationMethodDescriptor {
            code_ref: None,
            description: Some(
                "Per-lens verification counts partitioned by kind and result, \
                 plus the set of distinct verifiers. Feeds the orchestrator's \
                 decision about whether a lens has sufficient formal evidence."
                    .to_owned(),
            ),
            name: METHOD_NAME.to_owned(),
            parameters: None,
        }
    }

    fn scope(&self) -> ObservationScope {
        ObservationScope {
            communities: None,
            encounter_kinds: None,
            lenses: None,
            window: None,
        }
    }

    fn observe(&mut self, event: &IndexerEvent) -> ObserverResult<()> {
        let Some(record) = &event.record else {
            return Ok(());
        };
        let AnyRecord::Verification(verification) = record else {
            return Ok(());
        };

        let lens_key = verification
            .lens
            .uri
            .as_ref()
            .map(|u| u.as_str().to_owned())
            .or_else(|| {
                verification
                    .lens
                    .cid
                    .as_ref()
                    .map(|c| c.as_str().to_owned())
            })
            .unwrap_or_else(|| "<unidentified-lens>".to_owned());
        let entry = self.lenses.entry(lens_key).or_default();
        entry.total = entry.total.saturating_add(1);
        *entry
            .by_kind
            .entry(Self::kind_key(verification.kind))
            .or_insert(0) += 1;
        *entry
            .by_result
            .entry(Self::result_key(verification.result))
            .or_insert(0) += 1;
        *entry
            .verifiers
            .entry(verification.verifier.as_str().to_owned())
            .or_insert(0) += 1;

        Ok(())
    }

    fn snapshot(&self) -> ObserverResult<Option<serde_json::Value>> {
        if self.lenses.is_empty() {
            return Ok(None);
        }

        let mut lenses_map = serde_json::Map::new();
        for (key, coverage) in &self.lenses {
            let by_kind: serde_json::Map<String, serde_json::Value> = coverage
                .by_kind
                .iter()
                .map(|(k, v)| ((*k).to_owned(), serde_json::Value::Number((*v).into())))
                .collect();
            let by_result: serde_json::Map<String, serde_json::Value> = coverage
                .by_result
                .iter()
                .map(|(k, v)| ((*k).to_owned(), serde_json::Value::Number((*v).into())))
                .collect();
            let distinct_verifiers = coverage.verifiers.len();
            lenses_map.insert(
                key.clone(),
                serde_json::json!({
                    "total": coverage.total,
                    "byKind": by_kind,
                    "byResult": by_result,
                    "distinctVerifiers": distinct_verifiers,
                }),
            );
        }

        Ok(Some(serde_json::json!({ "lenses": lenses_map })))
    }
}
