//! Reference [`ObservationMethod`]: encounter counts grouped by
//! structured `purpose.action`, with optional vocabulary-rooted
//! rollup.
//!
//! Answers the "how many encounters had a pedagogical purpose?"
//! question as a bag-count over `purpose.action`, plus — when a
//! vocabulary is registered — a transitive rollup through the
//! `ThPurpose` subsumption hierarchy.
//!
//! # Operation
//!
//! - Leaf counts: every distinct `purpose.action` string seen across
//!   observed encounters, with its observation count.
//! - Rollup counts: for each ancestor action in the registered
//!   vocabulary, the sum of leaf counts for actions it subsumes.
//!   Under `closed-with-default`, the top action rolls up everything;
//!   under `hierarchy-closed`, only declared ancestors appear;
//!   under `open`, no rollup is performed.
//!
//! Encounters whose purpose references a vocabulary not in the
//! method's resolver contribute to the leaf-count bag but do not
//! fold into any rollup. The snapshot marks them as `unresolved`
//! so consumers can tell "unknown vocabulary" apart from "absent
//! purpose".

use std::collections::BTreeMap;

use idiolect_indexer::IndexerEvent;
use idiolect_records::AnyRecord;
use idiolect_records::generated::defs::Purpose;
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

/// Histogram of encounter counts by structured purpose.action, with
/// optional vocabulary-rooted rollup.
#[derive(Debug, Clone, Default)]
pub struct PurposeDistributionMethod {
    /// Total encounters observed.
    total: u64,
    /// Leaf counts keyed by `purpose.action`.
    leaves: BTreeMap<String, u64>,
    /// Encounters whose purpose referenced a vocabulary not in the
    /// registered resolver. Counted separately so consumers can
    /// distinguish "unknown vocabulary" from "no purpose at all".
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

impl PurposeDistributionMethod {
    /// Construct an empty aggregator with no registered vocabularies.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a vocabulary at its canonical at-uri. Rollups will
    /// attribute leaf counts to every declared subsumer under the
    /// vocabulary's world discipline.
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

    fn find_vocab(&self, purpose: &Purpose) -> Option<&InternalVocab> {
        purpose
            .vocabulary
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
                "Encounter counts grouped by structured purpose.action, with optional \
                 vocabulary-rooted rollup via the ThPurpose subsumption hierarchy."
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
            .entry(encounter.purpose.action.clone())
            .or_insert(0) += 1;
        // If the encounter declared a vocabulary we don't recognize,
        // bump the unresolved counter so the snapshot surfaces it.
        if let Some(vref) = encounter.purpose.vocabulary.as_ref()
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

        // Leaf counts.
        let leaves_json: serde_json::Map<String, serde_json::Value> = self
            .leaves
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::Number((*v).into())))
            .collect();

        // Rollup: for each leaf, add its count to every declared
        // ancestor in the resolvable vocabulary. Under
        // closed-with-default, the vocabulary's `top` also picks up
        // the total across every contributing leaf.
        let mut rollup: BTreeMap<String, u64> = BTreeMap::new();
        for (action, count) in &self.leaves {
            let temp_purpose = Purpose {
                action: action.clone(),
                material: None,
                actor: None,
                vocabulary: None,
            };
            let Some(vocab) = self.find_vocab(&temp_purpose) else {
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
                    // Under closed-with-default, `top` transitively
                    // subsumes every action — even undeclared ones
                    // or ones whose declared ancestry doesn't include
                    // `top`. Only credit top here when it wasn't
                    // already picked up by the declared ancestry.
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
    use idiolect_records::generated::defs::{LensRef, SchemaRef, VocabRef};
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
                downstream_result: None,
                kind: EncounterKind::Production,
                lens: LensRef {
                    cid: None,
                    direction: None,
                    uri: Some("at://did:plc:x/dev.panproto.schema.lens/l".into()),
                },
                occurred_at: "2026-04-23T00:00:00Z".into(),
                produced_output: None,
                purpose: Purpose {
                    action: action.to_owned(),
                    material: None,
                    actor: None,
                    vocabulary: vocab_uri.map(|u| VocabRef {
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
            name: "t".into(),
            description: None,
            world: VocabWorld::ClosedWithDefault,
            top: "any_purpose".into(),
            actions: vec![
                ActionEntry {
                    id: "any_purpose".into(),
                    parents: vec![],
                    description: None,
                },
                ActionEntry {
                    id: "pedagogy".into(),
                    parents: vec!["any_purpose".into()],
                    description: None,
                },
                ActionEntry {
                    id: "annotate".into(),
                    parents: vec!["pedagogy".into()],
                    description: None,
                },
            ],
            supersedes: None,
            occurred_at: "2026-04-23T00:00:00Z".into(),
        }
    }

    #[test]
    fn rolls_up_under_closed_with_default() {
        let mut m = PurposeDistributionMethod::new();
        let uri = "at://did:plc:x/dev.idiolect.vocab/t";
        m.register_vocabulary(uri, &ref_vocab());

        m.observe(&encounter("annotate", Some(uri))).unwrap();
        m.observe(&encounter("annotate", Some(uri))).unwrap();
        m.observe(&encounter("pedagogy", Some(uri))).unwrap();

        let snap = m.snapshot().unwrap().unwrap();
        assert_eq!(snap["total"], 3);
        assert_eq!(snap["leaves"]["annotate"], 2);
        assert_eq!(snap["leaves"]["pedagogy"], 1);
        // pedagogy rolls up into any_purpose (from 2 annotates + 1 pedagogy).
        assert_eq!(snap["rollup"]["pedagogy"], 2);
        assert_eq!(snap["rollup"]["any_purpose"], 3);
        assert_eq!(snap["unresolved"], 0);
    }

    #[test]
    fn unknown_vocabulary_is_counted_as_unresolved() {
        let mut m = PurposeDistributionMethod::new();
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
