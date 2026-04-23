//! Theory-resolver: validate a record's structured content against
//! its composed theory, and resolve theory-typed queries over the
//! catalog.
//!
//! The theory-resolver is the runtime counterpart to the content
//! theories in `theories/idiolect/`. For each record the catalog
//! ingests, the resolver checks whether the record's structured
//! fields are well-formed with respect to the theory that the
//! composition file merges for that record kind. Records whose
//! structure parses but whose references (vocabularies, schemas,
//! lenses) cannot yet be dereferenced return
//! [`Resolution::Unresolved`]; records whose structure is malformed
//! return [`Resolution::Invalid`] with the offending reason.
//!
//! Consumers use [`Resolver::resolution_of`] as a first-pass gate
//! before running theory-typed matchers, and
//! [`Resolver::purpose_subsumed_by`] as the canonical subsumption
//! predicate over a resolved vocabulary. Subsumption semantics are
//! parameterized by the vocabulary's declared `world`:
//!
//! - `closed-with-default`: undeclared actions are subsumed by
//!   `top` and nothing else.
//! - `hierarchy-closed`: only declared edges exist; undeclared
//!   actions subsume nothing.
//! - `open`: undeclared actions are incomparable to every declared
//!   action; callers must fall back to equality.

use std::collections::{BTreeMap, BTreeSet};

use idiolect_records::AnyRecord;
use idiolect_records::generated::defs::Purpose;
use idiolect_records::generated::vocab::{Vocab, VocabWorld};

/// Outcome of a theory-resolution pass over a record's content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Content is structurally well-formed and every cross-reference
    /// resolves in the supplied vocabulary set.
    WellFormed,
    /// Content parsed, but at least one referenced vocabulary / lens
    /// / schema is not yet in the resolver's catalog. A consumer may
    /// retry after ingesting the missing references.
    Unresolved {
        /// Short tag naming the unresolved reference.
        missing: String,
    },
    /// Content does not satisfy the theory. No amount of additional
    /// catalog data will make it valid.
    Invalid {
        /// Human-readable reason.
        reason: String,
    },
}

/// Vocabulary resolution state keyed by the vocabulary's at-uri
/// (or by the reference-vocabulary id when no uri is provided).
#[derive(Debug, Clone, Default)]
pub struct Resolver {
    vocabularies: BTreeMap<String, ResolvedVocabulary>,
}

/// Precomputed transitive-ancestor lookup for a vocabulary.
#[derive(Debug, Clone)]
struct ResolvedVocabulary {
    world: VocabWorld,
    top: String,
    /// `ancestors[id]` is the full transitive closure of actions
    /// that subsume `id`, including `top` for non-`top` ids under
    /// `closed-with-default`.
    ancestors: BTreeMap<String, BTreeSet<String>>,
    /// Set of all declared action ids (for fast membership checks).
    declared: BTreeSet<String>,
}

impl Resolver {
    /// Empty resolver — no vocabularies, no cached ancestors.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a vocabulary record at a canonical URI. Computes the
    /// transitive ancestor closure for every declared action once,
    /// so subsequent subsumption checks are O(|ancestors(id)|).
    pub fn insert_vocab(&mut self, uri: impl Into<String>, vocab: &Vocab) {
        let mut parents: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut declared: BTreeSet<String> = BTreeSet::new();
        for entry in &vocab.actions {
            declared.insert(entry.id.clone());
            parents
                .entry(entry.id.clone())
                .or_default()
                .extend(entry.parents.iter().cloned());
        }

        // Transitive closure: BFS from each id through its parents.
        let mut ancestors: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for id in &declared {
            let mut seen: BTreeSet<String> = BTreeSet::new();
            let mut frontier: Vec<String> =
                parents.get(id).into_iter().flatten().cloned().collect();
            while let Some(p) = frontier.pop() {
                if seen.insert(p.clone())
                    && let Some(grandparents) = parents.get(&p)
                {
                    frontier.extend(grandparents.iter().cloned());
                }
            }
            ancestors.insert(id.clone(), seen);
        }

        self.vocabularies.insert(
            uri.into(),
            ResolvedVocabulary {
                world: vocab.world,
                top: vocab.top.clone(),
                ancestors,
                declared,
            },
        );
    }

    /// Number of registered vocabularies.
    #[must_use]
    pub fn vocabulary_count(&self) -> usize {
        self.vocabularies.len()
    }

    /// Whether `specific.action` is subsumed by `general.action`
    /// under the vocabulary `specific.vocabulary` (or `general`'s,
    /// or the default) references.
    ///
    /// Returns `None` when the vocabulary isn't in the resolver yet
    /// (i.e. resolution is `Unresolved`). Returns `Some(true)` if
    /// the subsumption holds, `Some(false)` otherwise.
    ///
    /// Subsumption rules by world:
    /// - `closed-with-default`: every action is subsumed by `top`;
    ///   otherwise ancestry follows the declared edges.
    /// - `hierarchy-closed`: only declared edges count; undeclared
    ///   actions subsume nothing.
    /// - `open`: only equality holds; ancestry is not well-defined
    ///   for undeclared actions.
    #[must_use]
    pub fn purpose_subsumed_by(&self, specific: &Purpose, general: &Purpose) -> Option<bool> {
        // Equality is always decidable and world-independent.
        if specific.action == general.action {
            return Some(true);
        }

        let vocab_uri = specific
            .vocabulary
            .as_ref()
            .and_then(|v| v.uri.clone())
            .or_else(|| general.vocabulary.as_ref().and_then(|v| v.uri.clone()))
            .or_else(|| {
                // Fall back to whatever single vocabulary is registered
                // — convenient for tests and single-vocab deployments.
                if self.vocabularies.len() == 1 {
                    self.vocabularies.keys().next().cloned()
                } else {
                    None
                }
            })?;
        let resolved = self.vocabularies.get(&vocab_uri)?;

        Some(resolved.subsumed_by(&specific.action, &general.action))
    }

    /// Resolve the structural + cross-reference health of a record.
    ///
    /// Today this checks:
    /// - encounter: `purpose.vocabulary`, if present, must resolve
    ///   to a registered vocabulary, and `purpose.action` must be
    ///   admissible under that vocabulary's world.
    ///
    /// Other record kinds return [`Resolution::WellFormed`]
    /// because their structured content does not yet reference
    /// vocabularies. Extended as additional theory-typed cross-
    /// references land.
    #[must_use]
    pub fn resolution_of(&self, record: &AnyRecord) -> Resolution {
        match record {
            AnyRecord::Encounter(e) => self.resolve_purpose(&e.purpose),
            _ => Resolution::WellFormed,
        }
    }

    fn resolve_purpose(&self, purpose: &Purpose) -> Resolution {
        let Some(vocab_ref) = purpose.vocabulary.as_ref() else {
            // No vocabulary declared — purpose.action is opaque but
            // valid. Consumers that need subsumption will fall back
            // to equality.
            return Resolution::WellFormed;
        };
        let Some(uri) = vocab_ref.uri.as_ref() else {
            return Resolution::Invalid {
                reason: "purpose.vocabulary present but has no uri".into(),
            };
        };
        let Some(resolved) = self.vocabularies.get(uri) else {
            return Resolution::Unresolved {
                missing: format!("vocabulary:{uri}"),
            };
        };
        match resolved.world {
            VocabWorld::Open | VocabWorld::ClosedWithDefault => Resolution::WellFormed,
            VocabWorld::HierarchyClosed => {
                if resolved.declared.contains(&purpose.action) {
                    Resolution::WellFormed
                } else {
                    Resolution::Invalid {
                        reason: format!(
                            "action {:?} not declared in hierarchy-closed vocabulary {uri}",
                            purpose.action,
                        ),
                    }
                }
            }
        }
    }
}

impl ResolvedVocabulary {
    fn subsumed_by(&self, specific: &str, general: &str) -> bool {
        match self.world {
            VocabWorld::Open => false,
            VocabWorld::HierarchyClosed => self
                .ancestors
                .get(specific)
                .is_some_and(|a| a.contains(general)),
            VocabWorld::ClosedWithDefault => {
                // `top` subsumes every action transitively.
                if general == self.top {
                    return true;
                }
                // Otherwise consult declared ancestors.
                self.ancestors
                    .get(specific)
                    .is_some_and(|a| a.contains(general))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use idiolect_records::Vocab;
    use idiolect_records::generated::defs::{Purpose, VocabRef};
    use idiolect_records::generated::vocab::ActionEntry;

    fn entry(id: &str, parents: &[&str]) -> ActionEntry {
        ActionEntry {
            id: id.to_owned(),
            parents: parents.iter().map(|s| (*s).to_owned()).collect(),
            description: None,
        }
    }

    fn ref_vocab(world: VocabWorld) -> Vocab {
        Vocab {
            name: "test-v1".to_owned(),
            description: None,
            world,
            top: "any_purpose".to_owned(),
            actions: vec![
                entry("any_purpose", &[]),
                entry("pedagogy", &["any_purpose"]),
                entry("annotate", &["pedagogy"]),
                entry("machine_learning", &["any_purpose"]),
                entry("train_model", &["machine_learning"]),
                entry("fine_tune", &["train_model"]),
            ],
            supersedes: None,
            occurred_at: "2026-04-23T00:00:00Z".to_owned(),
        }
    }

    fn purpose(action: &str, vocab_uri: Option<&str>) -> Purpose {
        Purpose {
            action: action.to_owned(),
            material: None,
            actor: None,
            vocabulary: vocab_uri.map(|u| VocabRef {
                uri: Some(u.to_owned()),
                cid: None,
            }),
        }
    }

    #[test]
    fn transitive_ancestry_under_closed_with_default() {
        let mut r = Resolver::new();
        r.insert_vocab(
            "at://did:plc:x/dev.idiolect.vocab/ref",
            &ref_vocab(VocabWorld::ClosedWithDefault),
        );

        // fine_tune ⊑ train_model ⊑ machine_learning ⊑ any_purpose
        assert_eq!(
            r.purpose_subsumed_by(
                &purpose("fine_tune", Some("at://did:plc:x/dev.idiolect.vocab/ref")),
                &purpose("train_model", None),
            ),
            Some(true)
        );
        assert_eq!(
            r.purpose_subsumed_by(
                &purpose("fine_tune", Some("at://did:plc:x/dev.idiolect.vocab/ref")),
                &purpose("machine_learning", None),
            ),
            Some(true)
        );
        // annotate is not subsumed by machine_learning.
        assert_eq!(
            r.purpose_subsumed_by(
                &purpose("annotate", Some("at://did:plc:x/dev.idiolect.vocab/ref")),
                &purpose("machine_learning", None),
            ),
            Some(false)
        );
        // top subsumes everything.
        assert_eq!(
            r.purpose_subsumed_by(
                &purpose("fine_tune", Some("at://did:plc:x/dev.idiolect.vocab/ref")),
                &purpose("any_purpose", None),
            ),
            Some(true)
        );
    }

    #[test]
    fn equality_is_world_independent() {
        let mut r = Resolver::new();
        r.insert_vocab(
            "at://did:plc:x/dev.idiolect.vocab/ref",
            &ref_vocab(VocabWorld::Open),
        );
        assert_eq!(
            r.purpose_subsumed_by(&purpose("annotate", None), &purpose("annotate", None)),
            Some(true)
        );
    }

    #[test]
    fn open_world_rejects_non_equal_subsumption() {
        let mut r = Resolver::new();
        r.insert_vocab(
            "at://did:plc:x/dev.idiolect.vocab/ref",
            &ref_vocab(VocabWorld::Open),
        );
        assert_eq!(
            r.purpose_subsumed_by(
                &purpose("fine_tune", Some("at://did:plc:x/dev.idiolect.vocab/ref")),
                &purpose("train_model", None),
            ),
            Some(false)
        );
    }

    #[test]
    fn missing_vocab_returns_none() {
        let r = Resolver::new();
        assert_eq!(
            r.purpose_subsumed_by(
                &purpose(
                    "fine_tune",
                    Some("at://did:plc:x/dev.idiolect.vocab/missing")
                ),
                &purpose("train_model", None),
            ),
            None
        );
    }

    #[test]
    fn hierarchy_closed_rejects_undeclared_action() {
        let mut r = Resolver::new();
        r.insert_vocab(
            "at://did:plc:x/dev.idiolect.vocab/ref",
            &ref_vocab(VocabWorld::HierarchyClosed),
        );

        let p = Purpose {
            action: "ungrounded_purpose".to_owned(),
            material: None,
            actor: None,
            vocabulary: Some(VocabRef {
                uri: Some("at://did:plc:x/dev.idiolect.vocab/ref".to_owned()),
                cid: None,
            }),
        };
        let encounter = idiolect_records::Encounter {
            annotations: None,
            downstream_result: None,
            kind: idiolect_records::encounter::EncounterKind::InvocationLog,
            lens: idiolect_records::generated::defs::LensRef {
                cid: None,
                direction: None,
                uri: Some("at://x/dev.panproto.schema.lens/l".to_owned()),
            },
            occurred_at: "2026-04-23T00:00:00Z".to_owned(),
            produced_output: None,
            purpose: p,
            source_instance: None,
            source_schema: idiolect_records::generated::defs::SchemaRef {
                cid: None,
                language: None,
                uri: Some("at://x/dev.panproto.schema.schema/s".to_owned()),
            },
            target_schema: None,
            visibility: idiolect_records::generated::defs::Visibility::PublicDetailed,
        };
        let record = idiolect_records::AnyRecord::Encounter(encounter);
        match r.resolution_of(&record) {
            Resolution::Invalid { reason } => assert!(reason.contains("ungrounded_purpose")),
            other => panic!("expected Invalid, got {other:?}"),
        }
    }
}
