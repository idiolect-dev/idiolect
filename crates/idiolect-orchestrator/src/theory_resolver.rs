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
//! [`Resolver::action_subsumed_by`] / [`Resolver::purpose_subsumed_by`]
//! as the canonical subsumption predicates over a resolved
//! vocabulary. Subsumption semantics are parameterized by the
//! vocabulary's declared `world`:
//!
//! - `closed-with-default`: undeclared ids are subsumed by `top` and
//!   nothing else.
//! - `hierarchy-closed`: only declared edges exist; undeclared ids
//!   subsume nothing.
//! - `open`: undeclared ids are incomparable to every declared id;
//!   callers must fall back to equality.

use std::collections::{BTreeMap, BTreeSet};

use idiolect_records::AnyRecord;
use idiolect_records::generated::dev::idiolect::defs::Use;
use idiolect_records::generated::dev::idiolect::vocab::{Vocab, VocabWorld};

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

/// Vocabulary resolution state keyed by the vocabulary's at-uri.
#[derive(Debug, Clone, Default)]
pub struct Resolver {
    vocabularies: BTreeMap<String, ResolvedVocabulary>,
}

/// Precomputed transitive-ancestor lookup for a vocabulary.
#[derive(Debug, Clone)]
struct ResolvedVocabulary {
    world: VocabWorld,
    top: String,
    /// `ancestors[id]` is the full transitive closure of ids that
    /// subsume `id`, including `top` for non-`top` ids under
    /// `closed-with-default`.
    ancestors: BTreeMap<String, BTreeSet<String>>,
    /// Set of all declared ids (for fast membership checks).
    declared: BTreeSet<String>,
    /// `class[id]` is the attitudinal composition the entry declares
    /// itself an instance of, when set (e.g.
    /// `"dev.idiolect.asserted_use"`). Consumers dispatch on this
    /// when they want to respect both the subsumption hierarchy AND
    /// the attitudinal shape.
    class: BTreeMap<String, String>,
}

impl Resolver {
    /// Empty resolver — no vocabularies, no cached ancestors.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a vocabulary at its canonical at-uri. Computes the
    /// transitive ancestor closure for every declared id once, so
    /// subsequent subsumption checks are O(|ancestors(id)|).
    pub fn insert_vocab(&mut self, uri: impl Into<String>, vocab: &Vocab) {
        let mut parents: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut declared: BTreeSet<String> = BTreeSet::new();
        let mut class: BTreeMap<String, String> = BTreeMap::new();
        for entry in &vocab.actions {
            declared.insert(entry.id.clone());
            parents
                .entry(entry.id.clone())
                .or_default()
                .extend(entry.parents.iter().cloned());
            if let Some(cls) = entry.class.clone() {
                class.insert(entry.id.clone(), cls);
            }
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
                class,
            },
        );
    }

    /// Register every Vocab record currently in a [`Catalog`]. Call
    /// this after catalog ingest so subsumption + class queries
    /// immediately honour the catalog-live vocabularies. Overwrites
    /// any existing registration at the same URI.
    pub fn sync_from_catalog(&mut self, catalog: &crate::Catalog) {
        for entry in catalog.vocabularies() {
            self.insert_vocab(entry.uri.clone(), &entry.record);
        }
    }

    /// Look up the attitudinal class declared for `id` in the named
    /// vocabulary (or the single registered vocabulary when
    /// `vocab_uri` is `None`). Returns `None` when the vocabulary
    /// isn't registered, the id isn't declared, or no class was set.
    #[must_use]
    pub fn class_of(&self, vocab_uri: Option<&str>, id: &str) -> Option<&str> {
        let uri = vocab_uri.map(ToOwned::to_owned).or_else(|| {
            if self.vocabularies.len() == 1 {
                self.vocabularies.keys().next().cloned()
            } else {
                None
            }
        })?;
        let resolved = self.vocabularies.get(&uri)?;
        resolved.class.get(id).map(String::as_str)
    }

    /// Number of registered vocabularies.
    #[must_use]
    pub fn vocabulary_count(&self) -> usize {
        self.vocabularies.len()
    }

    /// Whether `specific` is subsumed by `general` under the named
    /// vocabulary, or — when `vocab_uri` is `None` and exactly one
    /// vocabulary is registered — under that one.
    ///
    /// Returns `None` when the vocabulary isn't in the resolver yet
    /// (treat as `Unresolved`). Returns `Some(true)` / `Some(false)`
    /// otherwise.
    #[must_use]
    pub fn subsumed_by(
        &self,
        vocab_uri: Option<&str>,
        specific: &str,
        general: &str,
    ) -> Option<bool> {
        if specific == general {
            return Some(true);
        }
        let uri = vocab_uri.map(ToOwned::to_owned).or_else(|| {
            if self.vocabularies.len() == 1 {
                self.vocabularies.keys().next().cloned()
            } else {
                None
            }
        })?;
        let resolved = self.vocabularies.get(&uri)?;
        Some(resolved.subsumes(specific, general))
    }

    /// Convenience over [`Self::subsumed_by`] for a `Use`'s action
    /// field. Dispatches against `use.action_vocabulary` when set,
    /// otherwise against `vocab_override`, otherwise against the
    /// single registered vocabulary when exactly one is present.
    #[must_use]
    pub fn action_subsumed_by(
        &self,
        u: &Use,
        general_action: &str,
        vocab_override: Option<&str>,
    ) -> Option<bool> {
        let uri = u
            .action_vocabulary
            .as_ref()
            .and_then(|v| v.uri.as_deref())
            .or(vocab_override);
        self.subsumed_by(uri, &u.action, general_action)
    }

    /// Convenience over [`Self::subsumed_by`] for a `Use`'s purpose
    /// field. Returns `None` when `use.purpose` is absent.
    #[must_use]
    pub fn purpose_subsumed_by(
        &self,
        u: &Use,
        general_purpose: &str,
        vocab_override: Option<&str>,
    ) -> Option<bool> {
        let purpose = u.purpose.as_deref()?;
        let uri = u
            .purpose_vocabulary
            .as_ref()
            .and_then(|v| v.uri.as_deref())
            .or(vocab_override);
        self.subsumed_by(uri, purpose, general_purpose)
    }

    /// Resolve the structural + cross-reference health of a record.
    ///
    /// Today this checks:
    /// - encounter: `use.action_vocabulary` and `use.purpose_vocabulary`,
    ///   if present, must resolve to registered vocabularies; and
    ///   the respective ids must be admissible under each
    ///   vocabulary's world.
    ///
    /// Other record kinds return [`Resolution::WellFormed`] because
    /// their structured content does not yet reference vocabularies.
    #[must_use]
    pub fn resolution_of(&self, record: &AnyRecord) -> Resolution {
        match record {
            AnyRecord::Encounter(e) => self.resolve_use(&e.r#use),
            _ => Resolution::WellFormed,
        }
    }

    fn resolve_use(&self, u: &Use) -> Resolution {
        if let Some(r) = self.check_dimension(
            u.action_vocabulary.as_ref().and_then(|v| v.uri.as_deref()),
            &u.action,
            "use.action_vocabulary",
        ) {
            return r;
        }
        if let Some(purpose) = u.purpose.as_deref()
            && let Some(r) = self.check_dimension(
                u.purpose_vocabulary.as_ref().and_then(|v| v.uri.as_deref()),
                purpose,
                "use.purpose_vocabulary",
            )
        {
            return r;
        }
        Resolution::WellFormed
    }

    /// Returns `Some(Unresolved)` when the vocabulary uri is set but
    /// unregistered, `Some(Invalid)` when an `id` is not declared in
    /// a hierarchy-closed vocabulary, or `None` when the dimension
    /// is well-formed and nothing more to check.
    fn check_dimension(&self, vocab_uri: Option<&str>, id: &str, tag: &str) -> Option<Resolution> {
        let uri = vocab_uri?;
        let Some(resolved) = self.vocabularies.get(uri) else {
            return Some(Resolution::Unresolved {
                missing: format!("{tag}:{uri}"),
            });
        };
        if matches!(resolved.world, VocabWorld::HierarchyClosed) && !resolved.declared.contains(id)
        {
            return Some(Resolution::Invalid {
                reason: format!("{id:?} not declared in hierarchy-closed vocabulary {uri}"),
            });
        }
        None
    }
}

impl ResolvedVocabulary {
    fn subsumes(&self, specific: &str, general: &str) -> bool {
        match self.world {
            VocabWorld::Open => false,
            VocabWorld::HierarchyClosed => self
                .ancestors
                .get(specific)
                .is_some_and(|a| a.contains(general)),
            VocabWorld::ClosedWithDefault => {
                // `top` subsumes every id transitively.
                if general == self.top {
                    return true;
                }
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
    use idiolect_records::generated::dev::idiolect::defs::{Use, VocabRef};
    use idiolect_records::generated::dev::idiolect::vocab::ActionEntry;

    fn entry(id: &str, parents: &[&str]) -> ActionEntry {
        ActionEntry {
            class: None,
            id: id.to_owned(),
            parents: parents.iter().map(|s| (*s).to_owned()).collect(),
            description: None,
        }
    }

    fn ref_action_vocab(world: VocabWorld) -> Vocab {
        Vocab {
            name: "actions-test".to_owned(),
            description: None,
            world,
            top: "any_action".to_owned(),
            actions: vec![
                entry("any_action", &[]),
                entry("train_model", &["any_action"]),
                entry("fine_tune", &["train_model"]),
                entry("annotate", &["any_action"]),
            ],
            supersedes: None,
            occurred_at: idiolect_records::Datetime::parse("2026-04-23T00:00:00Z")
                .expect("valid datetime"),
        }
    }

    fn ref_purpose_vocab() -> Vocab {
        Vocab {
            name: "purposes-test".to_owned(),
            description: None,
            world: VocabWorld::ClosedWithDefault,
            top: "any_purpose".to_owned(),
            actions: vec![
                entry("any_purpose", &[]),
                entry("commercial", &["any_purpose"]),
                entry("non_commercial", &["any_purpose"]),
                entry("academic", &["non_commercial"]),
            ],
            supersedes: None,
            occurred_at: idiolect_records::Datetime::parse("2026-04-23T00:00:00Z")
                .expect("valid datetime"),
        }
    }

    fn mk_use(
        action: &str,
        action_vocab: Option<&str>,
        purpose: Option<&str>,
        purpose_vocab: Option<&str>,
    ) -> Use {
        Use {
            action: action.to_owned(),
            material: None,
            actor: None,
            purpose: purpose.map(ToOwned::to_owned),
            action_vocabulary: action_vocab.map(|u| VocabRef {
                uri: Some(idiolect_records::AtUri::parse(u).expect("valid at-uri")),
                cid: None,
            }),
            purpose_vocabulary: purpose_vocab.map(|u| VocabRef {
                uri: Some(idiolect_records::AtUri::parse(u).expect("valid at-uri")),
                cid: None,
            }),
        }
    }

    #[test]
    fn transitive_ancestry_under_closed_with_default() {
        let mut r = Resolver::new();
        let av_uri = "at://did:plc:x/dev.idiolect.vocab/a";
        r.insert_vocab(av_uri, &ref_action_vocab(VocabWorld::ClosedWithDefault));

        let u = mk_use("fine_tune", Some(av_uri), None, None);
        assert_eq!(r.action_subsumed_by(&u, "train_model", None), Some(true));
        assert_eq!(r.action_subsumed_by(&u, "any_action", None), Some(true));
        assert_eq!(r.action_subsumed_by(&u, "annotate", None), Some(false));
    }

    #[test]
    fn purpose_subsumption_uses_its_own_vocabulary() {
        let mut r = Resolver::new();
        let av_uri = "at://did:plc:x/dev.idiolect.vocab/a";
        let pv_uri = "at://did:plc:x/dev.idiolect.vocab/p";
        r.insert_vocab(av_uri, &ref_action_vocab(VocabWorld::ClosedWithDefault));
        r.insert_vocab(pv_uri, &ref_purpose_vocab());

        let u = mk_use("annotate", Some(av_uri), Some("academic"), Some(pv_uri));
        assert_eq!(
            r.purpose_subsumed_by(&u, "non_commercial", None),
            Some(true)
        );
        assert_eq!(r.purpose_subsumed_by(&u, "commercial", None), Some(false));
    }

    #[test]
    fn open_world_rejects_non_equal_subsumption() {
        let mut r = Resolver::new();
        let av_uri = "at://did:plc:x/dev.idiolect.vocab/a";
        r.insert_vocab(av_uri, &ref_action_vocab(VocabWorld::Open));

        let u = mk_use("fine_tune", Some(av_uri), None, None);
        assert_eq!(r.action_subsumed_by(&u, "train_model", None), Some(false));
        assert_eq!(r.action_subsumed_by(&u, "fine_tune", None), Some(true));
    }

    #[test]
    fn missing_vocab_returns_none() {
        let r = Resolver::new();
        let u = mk_use(
            "fine_tune",
            Some("at://did:plc:x/dev.idiolect.vocab/missing"),
            None,
            None,
        );
        assert_eq!(r.action_subsumed_by(&u, "train_model", None), None);
    }

    #[test]
    fn hierarchy_closed_rejects_undeclared_action_in_resolution() {
        let mut r = Resolver::new();
        let av_uri = "at://did:plc:x/dev.idiolect.vocab/a";
        r.insert_vocab(av_uri, &ref_action_vocab(VocabWorld::HierarchyClosed));

        let encounter = idiolect_records::Encounter {
            annotations: None,
            basis: None,
            downstream_result: None,
            holder: None,
            kind:
                idiolect_records::generated::dev::idiolect::encounter::EncounterKind::InvocationLog,
            lens: idiolect_records::generated::dev::idiolect::defs::LensRef {
                cid: None,
                direction: None,
                uri: Some(
                    idiolect_records::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/l")
                        .expect("valid at-uri"),
                ),
            },
            occurred_at: idiolect_records::Datetime::parse("2026-04-23T00:00:00Z")
                .expect("valid datetime"),
            produced_output: None,
            r#use: mk_use("ungrounded_action", Some(av_uri), None, None),
            source_instance: None,
            source_schema: idiolect_records::generated::dev::idiolect::defs::SchemaRef {
                cid: None,
                language: None,
                uri: Some(
                    idiolect_records::AtUri::parse("at://did:plc:x/dev.panproto.schema.schema/s")
                        .expect("valid at-uri"),
                ),
            },
            target_schema: None,
            visibility:
                idiolect_records::generated::dev::idiolect::defs::Visibility::PublicDetailed,
        };
        let record = idiolect_records::AnyRecord::Encounter(encounter);
        match r.resolution_of(&record) {
            Resolution::Invalid { reason } => assert!(reason.contains("ungrounded_action")),
            other => panic!("expected Invalid, got {other:?}"),
        }
    }
}
