//! Reference [`ObservationMethod`]: lens adoption breadth.
//!
//! Counts, per lens at-uri, the number of distinct DIDs that have
//! produced an encounter referencing that lens. Output is a
//! `lens -> { encounters, distinctInvokers }` mapping, sorted by lens
//! key for deterministic snapshots.
//!
//! Rationale: "correction rate" and "verification coverage" measure
//! quality; adoption measures breadth — how widely a lens is actually
//! being invoked, across how many independent parties. A recommender
//! that prefers well-adopted lenses over well-verified-but-unused ones
//! (or vice versa) needs both signals.
//!
//! Record kinds observed: only `Encounter` contributes. Corrections,
//! retrospections, and verifications are silently ignored because they
//! are downstream of adoption; counting them here would inflate the
//! distinct-invoker count with non-invoker parties.

use std::collections::{BTreeMap, BTreeSet};

use idiolect_indexer::IndexerEvent;
use idiolect_records::AnyRecord;
use idiolect_records::generated::dev::idiolect::observation::{
    ObservationMethod as ObservationMethodDescriptor, ObservationScope,
};

use crate::error::ObserverResult;
use crate::method::ObservationMethod;

/// Canonical method name.
pub const METHOD_NAME: &str = "lens-adoption";

/// Method version.
pub const METHOD_VERSION: &str = "1.0.0";

/// Per-lens adoption counters.
#[derive(Debug, Default, Clone)]
struct AdoptionStats {
    /// Total encounters observed for the lens (duplicates by invoker
    /// allowed; counts every invocation).
    encounters: u64,
    /// Set of distinct invoker DIDs.
    invokers: BTreeSet<String>,
}

/// Aggregator: encounter count + distinct-invoker count per lens.
#[derive(Debug, Default, Clone)]
pub struct LensAdoptionMethod {
    /// Per-lens counters keyed by lens at-uri (or cid fallback).
    lenses: BTreeMap<String, AdoptionStats>,
}

impl LensAdoptionMethod {
    /// Construct an empty aggregator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl ObservationMethod for LensAdoptionMethod {
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
                "Per-lens adoption breadth: total encounters and count of distinct \
                 invoker DIDs. Pairs with correction-rate and verification-coverage \
                 to give an orchestrator both quality and breadth signals."
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

        let lens_key = encounter
            .lens
            .uri
            .clone()
            .or_else(|| encounter.lens.cid.clone())
            .unwrap_or_else(|| "<unidentified-lens>".to_owned());
        let entry = self.lenses.entry(lens_key).or_default();
        entry.encounters = entry.encounters.saturating_add(1);
        entry.invokers.insert(event.did.clone());

        Ok(())
    }

    fn snapshot(&self) -> ObserverResult<Option<serde_json::Value>> {
        if self.lenses.is_empty() {
            return Ok(None);
        }

        let mut lenses_map = serde_json::Map::new();
        for (key, stats) in &self.lenses {
            lenses_map.insert(
                key.clone(),
                serde_json::json!({
                    "encounters": stats.encounters,
                    "distinctInvokers": stats.invokers.len(),
                }),
            );
        }

        Ok(Some(serde_json::json!({ "lenses": lenses_map })))
    }
}
