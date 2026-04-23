//! Reference [`ObservationMethod`]: encounter counts grouped by
//! structured `use.purpose`, with optional purpose-vocabulary-rooted
//! rollup.
//!
//! Parallel to `ActionDistributionMethod` but dispatching on the
//! Use's purpose dimension. The two axes aggregate independently —
//! consumers register this method to answer "how many encounters
//! served a non-commercial purpose?" in the same way they use the
//! action counterpart to answer "how many encounters invoked an
//! action subsumed by `train_model`?"
//!
//! Encounters whose `use.purpose` is absent contribute to a
//! dedicated `missing` bucket so consumers can tell "no purpose
//! declared" from "no encounters at all."

use std::collections::BTreeMap;

use idiolect_indexer::IndexerEvent;
use idiolect_records::AnyRecord;
use idiolect_records::generated::observation::{
    ObservationMethod as ObservationMethodDescriptor, ObservationScope,
};
use idiolect_records::generated::vocab::{Vocab, VocabWorld};

use crate::error::ObserverResult;
use crate::method::ObservationMethod;

/// Canonical method name.
pub const METHOD_NAME: &str = "purpose-distribution";

/// Method version.
pub const METHOD_VERSION: &str = "1.0.0";

/// Histogram of encounter counts by structured `use.purpose`, with
/// optional purpose-vocabulary-rooted rollup.
#[derive(Debug, Clone, Default)]
pub struct PurposeDistributionMethod {
    /// Total encounters observed.
    total: u64,
    /// Encounters with no declared purpose.
    missing: u64,
    /// Leaf counts keyed by `use.purpose`.
    leaves: BTreeMap<String, u64>,
    /// Encounters whose `use.purpose_vocabulary` referenced a
    /// vocabulary not in the registered resolver.
    unresolved: u64,
    /// Registered purpose vocabularies keyed by canonical at-uri.
    vocabularies: BTreeMap<String, InternalVocab>,
}

#[derive(Debug, Clone)]
struct InternalVocab {
    world: VocabWorld,
    top: String,
    ancestors: BTreeMap<String, Vec<String>>,
}

impl PurposeDistributionMethod {
    /// Construct an empty aggregator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a purpose vocabulary at its canonical at-uri.
    pub fn register_vocabulary(&mut self, uri: impl Into<String>, vocab: &Vocab) {
        let mut parents: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for entry in &vocab.actions {
            parents
                .entry(entry.id.clone())
                .or_default()
                .extend(entry.parents.iter().cloned());
        }
        let mut ancestors: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for id in parents.keys() {
            let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            let mut frontier: Vec<String> =
                parents.get(id).into_iter().flatten().cloned().collect();
            while let Some(p) = frontier.pop() {
                if seen.insert(p.clone())
                    && let Some(gp) = parents.get(&p)
                {
                    frontier.extend(gp.iter().cloned());
                }
            }
            ancestors.insert(id.clone(), seen.into_iter().collect());
        }
        self.vocabularies.insert(
            uri.into(),
            InternalVocab {
                world: vocab.world,
                top: vocab.top.clone(),
                ancestors,
            },
        );
    }

    /// Total encounters observed.
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.total
    }

    /// Resolve a purpose-vocabulary by explicit URI, or fall back to
    /// the sole registered vocabulary when exactly one is known.
    /// Returns `None` if zero or ≥2 are registered and no URI is
    /// provided, or if the provided URI isn't registered.
    fn lookup_vocab(&self, vocab_uri: Option<&str>) -> Option<&InternalVocab> {
        if let Some(uri) = vocab_uri {
            return self.vocabularies.get(uri);
        }
        if self.vocabularies.len() == 1 {
            return self.vocabularies.values().next();
        }
        None
    }
}

impl ObservationMethod for PurposeDistributionMethod {
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
                "Encounter counts grouped by structured use.purpose, with optional \
                 vocabulary-rooted rollup via the ThUse purpose subsumption hierarchy."
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
        let Some(AnyRecord::Encounter(encounter)) = &event.record else {
            return Ok(());
        };
        self.total = self.total.saturating_add(1);
        let Some(purpose) = encounter.r#use.purpose.as_ref() else {
            self.missing = self.missing.saturating_add(1);
            return Ok(());
        };
        *self.leaves.entry(purpose.clone()).or_insert(0) += 1;
        if let Some(vref) = encounter.r#use.purpose_vocabulary.as_ref()
            && let Some(uri) = vref.uri.as_ref()
            && !self.vocabularies.contains_key(uri)
        {
            self.unresolved = self.unresolved.saturating_add(1);
        }
        Ok(())
    }

    fn snapshot(&self) -> ObserverResult<Option<serde_json::Value>> {
        if self.total == 0 {
            return Ok(None);
        }
        let leaves_json: serde_json::Map<String, serde_json::Value> = self
            .leaves
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::Number((*v).into())))
            .collect();

        let mut rollup: BTreeMap<String, u64> = BTreeMap::new();
        for (purpose, count) in &self.leaves {
            // No per-encounter vocabulary is retained past `observe`
            // — the leaf map is keyed by purpose string alone. Roll
            // up under the single registered vocabulary when that's
            // unambiguous; otherwise skip this leaf's rollup.
            let Some(vocab) = self.lookup_vocab(None) else {
                continue;
            };
            match vocab.world {
                VocabWorld::Open => {}
                VocabWorld::HierarchyClosed | VocabWorld::ClosedWithDefault => {
                    let declared = vocab.ancestors.get(purpose);
                    if let Some(ancestors) = declared {
                        for a in ancestors {
                            *rollup.entry(a.clone()).or_insert(0) += *count;
                        }
                    }
                    if matches!(vocab.world, VocabWorld::ClosedWithDefault)
                        && *purpose != vocab.top
                        && declared.is_none_or(|a| !a.iter().any(|x| x == &vocab.top))
                    {
                        *rollup.entry(vocab.top.clone()).or_insert(0) += *count;
                    }
                }
            }
        }
        let rollup_json: serde_json::Map<String, serde_json::Value> = rollup
            .into_iter()
            .map(|(k, v)| (k, serde_json::Value::Number(v.into())))
            .collect();

        Ok(Some(serde_json::json!({
            "total": self.total,
            "missing": self.missing,
            "leaves": leaves_json,
            "rollup": rollup_json,
            "unresolved": self.unresolved,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use idiolect_indexer::{IndexerAction, IndexerEvent};
    use idiolect_records::Encounter;
    use idiolect_records::generated::defs::{LensRef, SchemaRef, Use, VocabRef};
    use idiolect_records::generated::encounter::EncounterKind;
    use idiolect_records::generated::vocab::ActionEntry;

    fn encounter(purpose: Option<&str>, vocab_uri: Option<&str>) -> IndexerEvent {
        IndexerEvent {
            seq: 1,
            live: true,
            did: "did:plc:a".into(),
            rev: "r1".into(),
            collection: "dev.idiolect.encounter".into(),
            rkey: "e1".into(),
            action: IndexerAction::Create,
            cid: None,
            record: Some(AnyRecord::Encounter(Encounter {
                annotations: None,
                basis: None,
                downstream_result: None,
                holder: None,
                kind: EncounterKind::Production,
                lens: LensRef {
                    cid: None,
                    direction: None,
                    uri: Some("at://did:plc:x/dev.panproto.schema.lens/l".into()),
                },
                occurred_at: "2026-04-23T00:00:00Z".into(),
                produced_output: None,
                r#use: Use {
                    action: "act".into(),
                    material: None,
                    actor: None,
                    purpose: purpose.map(ToOwned::to_owned),
                    action_vocabulary: None,
                    purpose_vocabulary: vocab_uri.map(|u| VocabRef {
                        uri: Some(u.to_owned()),
                        cid: None,
                    }),
                },
                source_instance: None,
                source_schema: SchemaRef {
                    cid: None,
                    language: None,
                    uri: Some("at://did:plc:x/dev.panproto.schema.schema/s".into()),
                },
                target_schema: None,
                visibility: idiolect_records::generated::defs::Visibility::PublicDetailed,
            })),
        }
    }

    fn ref_vocab() -> Vocab {
        Vocab {
            name: "p".into(),
            description: None,
            world: VocabWorld::ClosedWithDefault,
            top: "any_purpose".into(),
            actions: vec![
                ActionEntry {
                    id: "any_purpose".into(),
                    parents: vec![],
                    description: None,
                    class: None,
                },
                ActionEntry {
                    id: "non_commercial".into(),
                    parents: vec!["any_purpose".into()],
                    description: None,
                    class: None,
                },
                ActionEntry {
                    id: "academic".into(),
                    parents: vec!["non_commercial".into()],
                    description: None,
                    class: None,
                },
            ],
            supersedes: None,
            occurred_at: "2026-04-23T00:00:00Z".into(),
        }
    }

    #[test]
    fn rolls_up_purposes_under_closed_with_default() {
        let mut m = PurposeDistributionMethod::new();
        let uri = "at://did:plc:x/dev.idiolect.vocab/p";
        m.register_vocabulary(uri, &ref_vocab());

        m.observe(&encounter(Some("academic"), Some(uri))).unwrap();
        m.observe(&encounter(Some("academic"), Some(uri))).unwrap();
        m.observe(&encounter(Some("non_commercial"), Some(uri)))
            .unwrap();

        let snap = m.snapshot().unwrap().unwrap();
        assert_eq!(snap["total"], 3);
        assert_eq!(snap["leaves"]["academic"], 2);
        assert_eq!(snap["leaves"]["non_commercial"], 1);
        assert_eq!(snap["rollup"]["non_commercial"], 2);
        assert_eq!(snap["rollup"]["any_purpose"], 3);
    }

    #[test]
    fn missing_purpose_counted_separately() {
        let mut m = PurposeDistributionMethod::new();
        m.observe(&encounter(None, None)).unwrap();
        m.observe(&encounter(Some("academic"), None)).unwrap();
        let snap = m.snapshot().unwrap().unwrap();
        assert_eq!(snap["total"], 2);
        assert_eq!(snap["missing"], 1);
    }
}
