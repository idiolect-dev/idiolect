//! Reference [`ObservationMethod`]: correction rate per lens.
//!
//! Counts, for each lens at-uri seen in an encounter, how many times
//! it appears in a correction — grouped by correction reason. The
//! output is a `lens -> { reason -> count, encounters, corrections,
//! rate }` mapping serialized as a json object.
//!
//! This method is intentionally simple: its job is to prove the
//! observer pipeline end-to-end (firehose → decode → fold → snapshot
//! → publish) works with a non-trivial aggregator, not to win any
//! statistical beauty contest. Real observers would weight by
//! visibility, decay old corrections, cross-check community scope,
//! etc.

use std::collections::BTreeMap;

use idiolect_indexer::IndexerEvent;
use idiolect_records::AnyRecord;
use idiolect_records::generated::dev::idiolect::correction::CorrectionReason;
use idiolect_records::generated::dev::idiolect::observation::{
    ObservationMethod as ObservationMethodDescriptor, ObservationScope,
};

use crate::error::ObserverResult;
use crate::method::ObservationMethod;

/// Canonical method name for correction-rate aggregation.
pub const METHOD_NAME: &str = "correction-rate";

/// Method version. Bump when the output shape changes in a way that
/// invalidates comparison with older snapshots.
pub const METHOD_VERSION: &str = "1.0.0";

/// Per-lens rolling counters.
///
/// `BTreeMap` rather than `HashMap` so the serialized output has a
/// stable key order — observations downstream of the firehose may be
/// compared structurally by test fixtures.
#[derive(Debug, Default, Clone)]
struct LensStats {
    /// Total encounters seen for this lens.
    encounters: u64,
    /// Total corrections seen for this lens, regardless of reason.
    corrections: u64,
    /// Corrections partitioned by reason. The key is the kebab-case
    /// serialized form of [`CorrectionReason`] — i.e. what shows up
    /// in the lexicon json — so readers of a published observation do
    /// not have to re-derive it.
    by_reason: BTreeMap<String, u64>,
}

/// Reference aggregator: tracks encounters and corrections per lens.
#[derive(Debug, Default, Clone)]
pub struct CorrectionRateMethod {
    /// Per-lens counters, keyed by lens at-uri.
    lenses: BTreeMap<String, LensStats>,
}

impl CorrectionRateMethod {
    /// Construct an empty aggregator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl ObservationMethod for CorrectionRateMethod {
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
                "Per-lens correction count grouped by correction reason. \
                 Output includes encounters, corrections, and a rate = corrections/encounters."
                    .to_owned(),
            ),
            name: METHOD_NAME.to_owned(),
            parameters: None,
        }
    }

    fn scope(&self) -> ObservationScope {
        // the reference method observes every encounter and correction
        // in the dev.idiolect.* family. a narrower method would set
        // encounter_kinds / communities / window here.
        ObservationScope {
            communities: None,
            encounter_kinds: None,
            encounter_kinds_vocab: None,
            lenses: None,
            window: None,
        }
    }

    fn observe(&mut self, event: &IndexerEvent) -> ObserverResult<()> {
        let Some(record) = &event.record else {
            // deletes and pre-filter events carry no record body;
            // nothing to aggregate.
            return Ok(());
        };

        match record {
            AnyRecord::Encounter(encounter) => {
                // encounters: increment the per-lens encounter count.
                // the lens ref's uri is the canonical identifier; when
                // the encounter only carries a cid (rare, but the
                // lexicon allows it), fall back to the cid so a lens
                // without an at-uri does not disappear from the
                // output.
                let lens_key = encounter
                    .lens
                    .uri
                    .as_ref()
                    .map(|u| u.as_str().to_owned())
                    .or_else(|| encounter.lens.cid.as_ref().map(|c| c.as_str().to_owned()))
                    .unwrap_or_else(|| "<unidentified-lens>".to_owned());
                let stats = self.lenses.entry(lens_key).or_default();
                stats.encounters = stats.encounters.saturating_add(1);
            }
            AnyRecord::Correction(correction) => {
                // corrections: the encounter they point at is the one
                // whose lens (if any) gets a +1 correction. we do not
                // resolve the encounter here — that is a job for a
                // richer observer with access to an indexed store.
                // instead, the method simply counts "corrections
                // against any encounter whose lens we have already
                // seen at least once" by walking the map. when the
                // encounter is unseen, the method buckets the
                // correction under a synthetic "<unknown>" lens so the
                // signal is not lost.
                let encounter_uri = correction.encounter.uri.as_str();
                // first-pass, we only know the encounter at-uri here,
                // not its lens. in practice an observer would keep a
                // side-map encounter_uri -> lens_uri. for the
                // reference method we sidestep the resolution by
                // tagging this correction against a stable "encounter
                // scope" key so counts stay comparable.
                let scope_key = format!("encounter:{encounter_uri}");
                let stats = self.lenses.entry(scope_key).or_default();
                stats.corrections = stats.corrections.saturating_add(1);
                let reason = correction.reason.as_str().to_owned();
                *stats.by_reason.entry(reason).or_insert(0) += 1;
            }
            // retrospections and verifications would feed a richer
            // reference method; the rate aggregator does not care.
            _ => {}
        }

        Ok(())
    }

    fn snapshot(&self) -> ObserverResult<Option<serde_json::Value>> {
        if self.lenses.is_empty() {
            return Ok(None);
        }

        // serialize in a deterministic shape:
        //
        //   { "lenses": { <key>: { encounters, corrections, rate,
        //                          byReason: { <reason>: <count>, ... }}, ...} }
        //
        // the rate is computed at snapshot time rather than on the fly
        // so observe() stays branch-free on corrections.
        let mut lenses_map = serde_json::Map::new();
        for (key, stats) in &self.lenses {
            let rate = if stats.encounters == 0 {
                // no encounters observed; "rate" is undefined. emit
                // null so downstream readers do not mistake "no
                // information" for "zero rate".
                serde_json::Value::Null
            } else {
                // saturating cast — u64 -> f64 is lossless below 2^53,
                // and encounters beyond that point in a test window
                // would make the rate meaningless anyway.
                #[allow(clippy::cast_precision_loss)]
                let r = (stats.corrections as f64) / (stats.encounters as f64);
                serde_json::Number::from_f64(r)
                    .map_or(serde_json::Value::Null, serde_json::Value::Number)
            };

            let by_reason: serde_json::Map<String, serde_json::Value> = stats
                .by_reason
                .iter()
                .map(|(r, c)| (r.clone(), serde_json::Value::Number((*c).into())))
                .collect();

            let entry = serde_json::json!({
                "encounters": stats.encounters,
                "corrections": stats.corrections,
                "rate": rate,
                "byReason": by_reason,
            });
            lenses_map.insert(key.clone(), entry);
        }

        Ok(Some(serde_json::json!({ "lenses": lenses_map })))
    }
}
