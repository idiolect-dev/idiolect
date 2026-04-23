//! Reference [`ObservationMethod`]: encounter counts grouped by
//! structured `use.action`, with optional action-vocabulary-rooted
//! rollup.
//!
//! Answers questions like "how many encounters invoked an action
//! subsumed by `train_model`?" as a bag-count over `use.action`
//! plus — when a vocabulary is registered — a transitive rollup
//! through the `ThUse` action subsumption hierarchy. A parallel
//! method could aggregate over `use.purpose` against a purpose
//! vocabulary; the pattern generalizes.
//!
//! # Operation
//!
//! - Leaf counts: every distinct `use.action` string seen across
//!   observed encounters, with its observation count.
//! - Rollup counts: for each ancestor action in the registered
//!   vocabulary, the sum of leaf counts for actions it subsumes.
//!   Under `closed-with-default`, the top action rolls up
//!   everything; under `hierarchy-closed`, only declared ancestors
//!   appear; under `open`, no rollup is performed.
//!
//! Encounters whose `use.action_vocabulary` is set to a vocabulary
//! not in the method's resolver contribute to the leaf-count bag
//! but do not fold into any rollup. The snapshot exposes them as
//! `unresolved` so consumers can tell "unknown vocabulary" apart
//! from "absent vocabulary".

use std::collections::BTreeMap;

use idiolect_indexer::IndexerEvent;
use idiolect_records::AnyRecord;
use idiolect_records::generated::defs::Use;
use idiolect_records::generated::observation::{
    ObservationMethod as ObservationMethodDescriptor, ObservationScope,
};
use idiolect_records::generated::vocab::{Vocab, VocabWorld};

use crate::error::ObserverResult;
use crate::method::ObservationMethod;

/// Canonical method name.
pub const METHOD_NAME: &str = "action-distribution";

/// Method version.
pub const METHOD_VERSION: &str = "1.0.0";

/// Histogram of encounter counts by structured `use.action`, with
/// optional vocabulary-rooted rollup.
#[derive(Debug, Clone, Default)]
pub struct ActionDistributionMethod {
    /// Total encounters observed.
    total: u64,
    /// Leaf counts keyed by `use.action`.
    leaves: BTreeMap<String, u64>,
    /// Encounters whose `use.action_vocabulary` referenced a
    /// vocabulary not in the registered resolver. Counted
    /// separately so consumers can distinguish "unknown vocabulary"
    /// from "no vocabulary at all".
    unresolved: u64,
    /// Registered vocabularies keyed by canonical at-uri.
    vocabularies: BTreeMap<String, InternalVocab>,
}

#[derive(Debug, Clone)]
struct InternalVocab {
    world: VocabWorld,
    top: String,
    /// `ancestors[id]` is every declared subsumer of `id`, transitively.
    ancestors: BTreeMap<String, Vec<String>>,
}

impl ActionDistributionMethod {
    /// Construct an empty aggregator with no registered vocabularies.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an action vocabulary at its canonical at-uri.
    /// Rollups will attribute leaf counts to every declared
    /// subsumer under the vocabulary's world discipline.
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

    fn find_vocab(&self, u: &Use) -> Option<&InternalVocab> {
        u.action_vocabulary
            .as_ref()
            .and_then(|v| v.uri.as_ref())
            .and_then(|uri| self.vocabularies.get(uri))
            .or_else(|| {
                if self.vocabularies.len() == 1 {
                    self.vocabularies.values().next()
                } else {
                    None
                }
            })
    }
}

impl ObservationMethod for ActionDistributionMethod {
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
                "Encounter counts grouped by structured use.action, with optional \
                 vocabulary-rooted rollup via the ThUse action subsumption hierarchy."
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
        *self
            .leaves
            .entry(encounter.r#use.action.clone())
            .or_insert(0) += 1;
        if let Some(vref) = encounter.r#use.action_vocabulary.as_ref()
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
        for (action, count) in &self.leaves {
            let temp = Use {
                action: action.clone(),
                material: None,
                actor: None,
                purpose: None,
                action_vocabulary: None,
                purpose_vocabulary: None,
            };
            let Some(vocab) = self.find_vocab(&temp) else {
                continue;
            };
            match vocab.world {
                VocabWorld::Open => { /* no rollup under open world */ }
                VocabWorld::HierarchyClosed | VocabWorld::ClosedWithDefault => {
                    let declared = vocab.ancestors.get(action);
                    if let Some(ancestors) = declared {
                        for a in ancestors {
                            *rollup.entry(a.clone()).or_insert(0) += *count;
                        }
                    }
                    if matches!(vocab.world, VocabWorld::ClosedWithDefault)
                        && *action != vocab.top
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

    fn encounter(action: &str, vocab_uri: Option<&str>) -> IndexerEvent {
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
                    action: action.to_owned(),
                    material: None,
                    actor: None,
                    purpose: None,
                    action_vocabulary: vocab_uri.map(|u| VocabRef {
                        uri: Some(u.to_owned()),
                        cid: None,
                    }),
                    purpose_vocabulary: None,
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
            name: "t".into(),
            description: None,
            world: VocabWorld::ClosedWithDefault,
            top: "any_action".into(),
            actions: vec![
                ActionEntry {
                    class: None,
                    id: "any_action".into(),
                    parents: vec![],
                    description: None,
                },
                ActionEntry {
                    class: None,
                    id: "train_model".into(),
                    parents: vec!["any_action".into()],
                    description: None,
                },
                ActionEntry {
                    class: None,
                    id: "fine_tune".into(),
                    parents: vec!["train_model".into()],
                    description: None,
                },
            ],
            supersedes: None,
            occurred_at: "2026-04-23T00:00:00Z".into(),
        }
    }

    #[test]
    fn rolls_up_under_closed_with_default() {
        let mut m = ActionDistributionMethod::new();
        let uri = "at://did:plc:x/dev.idiolect.vocab/t";
        m.register_vocabulary(uri, &ref_vocab());

        m.observe(&encounter("fine_tune", Some(uri))).unwrap();
        m.observe(&encounter("fine_tune", Some(uri))).unwrap();
        m.observe(&encounter("train_model", Some(uri))).unwrap();

        let snap = m.snapshot().unwrap().unwrap();
        assert_eq!(snap["total"], 3);
        assert_eq!(snap["leaves"]["fine_tune"], 2);
        assert_eq!(snap["leaves"]["train_model"], 1);
        assert_eq!(snap["rollup"]["train_model"], 2);
        assert_eq!(snap["rollup"]["any_action"], 3);
        assert_eq!(snap["unresolved"], 0);
    }

    #[test]
    fn unknown_vocabulary_is_counted_as_unresolved() {
        let mut m = ActionDistributionMethod::new();
        m.observe(&encounter(
            "train_model",
            Some("at://did:plc:x/dev.idiolect.vocab/unknown"),
        ))
        .unwrap();
        let snap = m.snapshot().unwrap().unwrap();
        assert_eq!(snap["total"], 1);
        assert_eq!(snap["unresolved"], 1);
    }
}
