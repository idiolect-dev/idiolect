//! Reference [`ObservationMethod`]: encounter throughput by kind.
//!
//! Counts encounters observed, partitioned by
//! [`EncounterKind`](idiolect_records::generated::encounter::EncounterKind)
//! and [`EncounterDownstreamResult`](idiolect_records::generated::encounter::EncounterDownstreamResult).
//! Output is a small json object with total-seen and the two histograms.
//!
//! Purpose: the simplest possible "is the firehose healthy and are
//! encounters flowing" signal. Shipped alongside the correction-rate
//! method so operators can tell "no corrections observed" from
//! "no encounters at all observed".

use std::collections::BTreeMap;

use idiolect_indexer::IndexerEvent;
use idiolect_records::AnyRecord;
use idiolect_records::generated::encounter::{EncounterDownstreamResult, EncounterKind};
use idiolect_records::generated::observation::{
    ObservationMethod as ObservationMethodDescriptor, ObservationScope,
};

use crate::error::ObserverResult;
use crate::method::ObservationMethod;

/// Canonical method name.
pub const METHOD_NAME: &str = "encounter-throughput";

/// Method version.
pub const METHOD_VERSION: &str = "1.0.0";

/// Histogram aggregator for encounter-family traffic.
#[derive(Debug, Default, Clone)]
pub struct EncounterThroughputMethod {
    /// Total encounter records observed.
    total: u64,
    /// Per-kind counts, in stable (serialized) order.
    by_kind: BTreeMap<&'static str, u64>,
    /// Per-downstream-result counts. `None` means the encounter
    /// record did not include a result.
    by_downstream_result: BTreeMap<&'static str, u64>,
}

impl EncounterThroughputMethod {
    /// Construct an empty aggregator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Serialize [`EncounterKind`] into its kebab-case wire form.
    const fn kind_key(kind: EncounterKind) -> &'static str {
        match kind {
            EncounterKind::InvocationLog => "invocation-log",
            EncounterKind::Curated => "curated",
            EncounterKind::RoundtripVerified => "roundtrip-verified",
            EncounterKind::Production => "production",
            EncounterKind::Adversarial => "adversarial",
        }
    }

    /// Serialize [`EncounterDownstreamResult`] into its kebab-case wire form.
    const fn result_key(result: EncounterDownstreamResult) -> &'static str {
        match result {
            EncounterDownstreamResult::Success => "success",
            EncounterDownstreamResult::Corrected => "corrected",
            EncounterDownstreamResult::Rejected => "rejected",
            EncounterDownstreamResult::Unknown => "unknown",
        }
    }

    /// Total encounters observed so far.
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.total
    }
}

impl ObservationMethod for EncounterThroughputMethod {
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
                "Encounter traffic partitioned by kind and downstream result. \
                 A liveness signal for the firehose more than an analytical product."
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
        let AnyRecord::Encounter(encounter) = record else {
            return Ok(());
        };

        self.total = self.total.saturating_add(1);
        *self
            .by_kind
            .entry(Self::kind_key(encounter.kind))
            .or_insert(0) += 1;
        let result_key = encounter.downstream_result.map_or("none", Self::result_key);
        *self.by_downstream_result.entry(result_key).or_insert(0) += 1;

        Ok(())
    }

    fn snapshot(&self) -> ObserverResult<Option<serde_json::Value>> {
        if self.total == 0 {
            return Ok(None);
        }

        let by_kind: serde_json::Map<String, serde_json::Value> = self
            .by_kind
            .iter()
            .map(|(k, v)| ((*k).to_owned(), serde_json::Value::Number((*v).into())))
            .collect();
        let by_result: serde_json::Map<String, serde_json::Value> = self
            .by_downstream_result
            .iter()
            .map(|(k, v)| ((*k).to_owned(), serde_json::Value::Number((*v).into())))
            .collect();

        Ok(Some(serde_json::json!({
            "total": self.total,
            "byKind": by_kind,
            "byDownstreamResult": by_result,
        })))
    }
}
