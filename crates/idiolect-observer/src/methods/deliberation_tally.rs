//! Reference [`ObservationMethod`]: deliberation outcome tally.
//!
//! Watches `dev.idiolect.deliberationVote` records and aggregates
//! per-statement counts grouped by stance slug. The snapshot output
//! shape mirrors the `dev.idiolect.deliberationOutcome.statementTallies`
//! field, so a downstream publisher can lift the snapshot into a
//! deliberationOutcome record without re-shaping.
//!
//! Stance slugs are taken verbatim from the vote's `stance` field
//! when no canonical vocabulary is configured. When the consumer
//! wires a [`VocabRegistry`] plus a canonical-vocab AT-URI via
//! [`DeliberationTallyMethod::with_canonical_stance_vocab`], every
//! incoming vote's stance is translated via `equivalent_to` edges
//! into the canonical slug before counting. That keeps tallies
//! comparable across dialects: a vote authored as `endorse` against
//! a community vocab that declares `endorse equivalent_to agree`
//! lands in the same `agree` bucket as a canonical-vocab vote.
//! Untranslated slugs (no path in the registry) pass through
//! verbatim, so the bucket label still tells consumers what the
//! original wire form was.

use std::collections::BTreeMap;

use idiolect_indexer::IndexerEvent;
use idiolect_records::AnyRecord;
use idiolect_records::generated::dev::idiolect::observation::{
    ObservationMethod as ObservationMethodDescriptor, ObservationScope,
};
use idiolect_records::vocab::VocabRegistry;

use crate::error::ObserverResult;
use crate::method::ObservationMethod;

/// Canonical method name.
pub const METHOD_NAME: &str = "deliberation-tally";

/// Method version. Bump when the output shape changes in a way
/// that invalidates comparison with older snapshots.
pub const METHOD_VERSION: &str = "1.0.0";

/// Per-statement tally bucket. Keyed by the at-uri of the statement
/// being voted on.
#[derive(Debug, Default, Clone)]
struct StatementTally {
    /// Cid of the statement, when the vote carried it. Pinned to
    /// the latest cid the aggregator has seen so the published
    /// outcome record can carry a strong ref.
    cid: Option<String>,
    /// Stance slug → count. `BTreeMap` so serialized output is in
    /// stable key order.
    counts: BTreeMap<String, u64>,
    /// Stance slug → weighted-count sum (scaled by 1000 per the
    /// `deliberationVote.weight` convention). Populated only for
    /// votes that carried `weight`.
    weighted_counts: BTreeMap<String, u64>,
}

/// Aggregator: tallies votes per (statement at-uri, stance slug).
#[derive(Debug, Default, Clone)]
pub struct DeliberationTallyMethod {
    /// Per-statement counters keyed by statement at-uri.
    statements: BTreeMap<String, StatementTally>,
    /// Total votes observed.
    total_votes: u64,
    /// Optional canonical vocabulary AT-URI. When set together with
    /// [`Self::registry`], every incoming vote's stance is
    /// translated into the canonical slug before counting; otherwise
    /// stances pass through verbatim.
    canonical_stance_vocab: Option<String>,
    /// Vocab registry used for stance translation. Present together
    /// with [`Self::canonical_stance_vocab`].
    registry: Option<VocabRegistry>,
}

impl DeliberationTallyMethod {
    /// Construct an empty aggregator. Stance slugs are kept
    /// verbatim; consumers that want cross-dialect canonicalisation
    /// chain [`Self::with_canonical_stance_vocab`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure cross-dialect canonicalisation. Every incoming
    /// vote's stance slug is run through
    /// [`VocabRegistry::translate`] from the vote's stance vocab
    /// into the supplied `canonical_uri` before counting. The
    /// registry must already contain both the source and target
    /// vocabs at the moment a vote is observed; missing vocabs
    /// degrade gracefully (the verbatim slug is used).
    ///
    /// The vote's source vocab is taken from the vote's
    /// `stanceVocab` field when set, otherwise defaulting to the
    /// canonical AT-URI itself (i.e. the vote is assumed to already
    /// be authored in the canonical vocab).
    #[must_use]
    pub fn with_canonical_stance_vocab(
        mut self,
        canonical_uri: impl Into<String>,
        registry: VocabRegistry,
    ) -> Self {
        self.canonical_stance_vocab = Some(canonical_uri.into());
        self.registry = Some(registry);
        self
    }

    /// Total votes observed since construction.
    #[must_use]
    pub const fn total_votes(&self) -> u64 {
        self.total_votes
    }

    /// Map a wire-form stance slug to its canonical form when a
    /// registry + canonical vocab are configured. Falls through to
    /// the verbatim slug when no canonicalisation is configured, the
    /// source vocab is unknown to the registry, or no `equivalent_to`
    /// path resolves the slug into the canonical vocab.
    fn canonicalize_stance(&self, source_vocab: Option<&str>, slug: &str) -> String {
        let (Some(registry), Some(canonical_uri)) =
            (self.registry.as_ref(), self.canonical_stance_vocab.as_deref())
        else {
            return slug.to_owned();
        };
        let from_uri = source_vocab.unwrap_or(canonical_uri);
        registry
            .translate(from_uri, canonical_uri, slug)
            .unwrap_or_else(|| slug.to_owned())
    }
}

impl ObservationMethod for DeliberationTallyMethod {
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
                "Per-statement vote tallies for community deliberations. \
                 Output mirrors dev.idiolect.deliberationOutcome.statementTallies \
                 so a publisher can lift snapshots into outcome records directly."
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
            encounter_kinds_vocab: None,
            lenses: None,
            window: None,
        }
    }

    fn observe(&mut self, event: &IndexerEvent) -> ObserverResult<()> {
        let Some(AnyRecord::DeliberationVote(vote)) = &event.record else {
            return Ok(());
        };
        self.total_votes = self.total_votes.saturating_add(1);
        let statement_uri = vote.subject.uri.as_str().to_owned();
        let raw_stance = vote.stance.as_str();
        let source_vocab_uri = vote
            .stance_vocab
            .as_ref()
            .and_then(|v| v.uri.as_ref())
            .map(idiolect_records::AtUri::as_str);
        let stance = self.canonicalize_stance(source_vocab_uri, raw_stance);
        let bucket = self.statements.entry(statement_uri).or_default();
        if bucket.cid.is_none() {
            bucket.cid = Some(vote.subject.cid.as_str().to_owned());
        }
        *bucket.counts.entry(stance.clone()).or_insert(0) += 1;
        if let Some(weight) = vote.weight {
            // Lexicon constrains weight to 0..=1000; saturate
            // defensively in case a malformed record sneaks through.
            let w: u64 = u64::try_from(weight.max(0)).unwrap_or(0);
            let slot = bucket.weighted_counts.entry(stance).or_insert(0);
            *slot = slot.saturating_add(w);
        }
        Ok(())
    }

    fn snapshot(&self) -> ObserverResult<Option<serde_json::Value>> {
        if self.total_votes == 0 {
            return Ok(None);
        }
        let statement_tallies: Vec<serde_json::Value> = self
            .statements
            .iter()
            .map(|(uri, tally)| {
                let counts: Vec<serde_json::Value> = tally
                    .counts
                    .iter()
                    .map(|(stance, count)| {
                        serde_json::json!({
                            "stance": stance,
                            "count": count,
                        })
                    })
                    .collect();
                let weighted_counts: Vec<serde_json::Value> = tally
                    .weighted_counts
                    .iter()
                    .map(|(stance, count)| {
                        serde_json::json!({
                            "stance": stance,
                            "count": count,
                        })
                    })
                    .collect();
                let mut entry = serde_json::json!({
                    "statement": {
                        "uri": uri,
                        "cid": tally.cid.as_deref().unwrap_or(""),
                    },
                    "counts": counts,
                });
                if !weighted_counts.is_empty() {
                    entry["weightedCounts"] = serde_json::Value::Array(weighted_counts);
                }
                entry
            })
            .collect();
        Ok(Some(serde_json::json!({
            "totalVotes": self.total_votes,
            "statementTallies": statement_tallies,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use idiolect_indexer::IndexerAction;
    use idiolect_records::DeliberationVote;
    use idiolect_records::generated::dev::idiolect::defs::StrongRecordRef;
    use idiolect_records::generated::dev::idiolect::deliberation_vote::DeliberationVoteStance;

    fn vote(statement_uri: &str, statement_cid: &str, stance: DeliberationVoteStance, weight: Option<i64>) -> AnyRecord {
        AnyRecord::DeliberationVote(DeliberationVote {
            subject: StrongRecordRef {
                uri: idiolect_records::AtUri::parse(statement_uri).expect("valid at-uri"),
                cid: idiolect_records::Cid::parse(statement_cid).expect("valid cid"),
            },
            stance,
            stance_vocab: None,
            weight,
            rationale: None,
            created_at: idiolect_records::Datetime::parse("2026-04-29T00:00:00Z")
                .expect("valid datetime"),
        })
    }

    fn event(seq: u64, record: AnyRecord) -> IndexerEvent {
        IndexerEvent {
            seq,
            live: true,
            did: "did:plc:voter".to_owned(),
            rev: format!("rev{seq}"),
            collection: idiolect_records::Nsid::parse("dev.idiolect.deliberationVote")
                .expect("valid nsid"),
            rkey: format!("v{seq}"),
            action: IndexerAction::Create,
            cid: Some(format!("bafyrei{seq}")),
            record: Some(record),
        }
    }

    #[test]
    fn folds_votes_into_per_statement_per_stance_counts() {
        let s1 = "at://did:plc:c/dev.idiolect.deliberationStatement/s1";
        let s2 = "at://did:plc:c/dev.idiolect.deliberationStatement/s2";
        let cid = "bafyreidfcm4u3vnuph5ltwdpssiz3a4xfbm2otjrdisftwnbfmnxd6lsxm";
        let mut m = DeliberationTallyMethod::new();
        for (i, ev_record) in [
            vote(s1, cid, DeliberationVoteStance::Agree, None),
            vote(s1, cid, DeliberationVoteStance::Agree, None),
            vote(s1, cid, DeliberationVoteStance::Disagree, None),
            vote(s2, cid, DeliberationVoteStance::Pass, None),
        ]
        .into_iter()
        .enumerate()
        {
            m.observe(&event(i as u64, ev_record)).expect("observe");
        }
        assert_eq!(m.total_votes(), 4);
        let snap = m.snapshot().expect("snapshot").expect("non-empty");
        let tallies = snap["statementTallies"].as_array().expect("array");
        assert_eq!(tallies.len(), 2);
        // Stable ordering — s1 sorts before s2.
        let s1_counts = &tallies[0]["counts"];
        let agree = s1_counts
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["stance"] == "agree")
            .expect("agree entry");
        assert_eq!(agree["count"], 2);
    }

    #[test]
    fn empty_observer_returns_none_on_snapshot() {
        let m = DeliberationTallyMethod::new();
        let snap = m.snapshot().expect("snapshot");
        assert!(snap.is_none());
    }

    #[test]
    fn weighted_counts_appear_only_when_votes_carry_weight() {
        let s1 = "at://did:plc:c/dev.idiolect.deliberationStatement/s1";
        let cid = "bafyreidfcm4u3vnuph5ltwdpssiz3a4xfbm2otjrdisftwnbfmnxd6lsxm";
        let mut m = DeliberationTallyMethod::new();
        m.observe(&event(0, vote(s1, cid, DeliberationVoteStance::Agree, Some(750))))
            .expect("observe");
        m.observe(&event(1, vote(s1, cid, DeliberationVoteStance::Agree, Some(250))))
            .expect("observe");
        let snap = m.snapshot().expect("snapshot").expect("non-empty");
        let weighted = &snap["statementTallies"][0]["weightedCounts"];
        let agree = weighted
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["stance"] == "agree")
            .expect("agree weighted entry");
        assert_eq!(agree["count"], 1000);
    }

    #[test]
    fn other_record_kinds_are_ignored() {
        // Encountering a non-vote record must not crash or count.
        let mut m = DeliberationTallyMethod::new();
        let ev = IndexerEvent {
            seq: 0,
            live: true,
            did: "did:plc:x".to_owned(),
            rev: "rev0".to_owned(),
            collection: idiolect_records::Nsid::parse("dev.idiolect.encounter")
                .expect("valid nsid"),
            rkey: "x".to_owned(),
            action: IndexerAction::Create,
            cid: None,
            record: None,
        };
        m.observe(&ev).expect("observe ignores non-vote events");
        assert_eq!(m.total_votes(), 0);
    }

    #[test]
    fn canonicalises_stance_via_registry_when_configured() {
        use idiolect_records::Vocab;
        use idiolect_records::generated::dev::idiolect::defs::VocabRef;
        use idiolect_records::vocab::VocabRegistry;

        let bridge: Vocab = serde_json::from_value(serde_json::json!({
            "name":  "blacksky",
            "world": "open",
            "nodes": [
                { "id": "endorse", "kind": "concept" },
                { "id": "agree",   "kind": "concept" },
                { "id": "equivalent_to", "kind": "relation",
                  "relationMetadata": { "symmetric": true, "transitive": true } },
            ],
            "edges": [
                { "source": "endorse", "target": "agree", "relationSlug": "equivalent_to" },
            ],
            "occurredAt": "2026-04-29T00:00:00.000Z",
        }))
        .expect("bridge parses");
        let canonical: Vocab = serde_json::from_value(serde_json::json!({
            "name":  "canonical",
            "world": "open",
            "nodes": [{ "id": "agree", "kind": "concept" }],
            "occurredAt": "2026-04-29T00:00:00.000Z",
        }))
        .expect("canonical parses");

        let bridge_uri = "at://did:plc:x/dev.idiolect.vocab/bridge";
        let canonical_uri = "at://did:plc:x/dev.idiolect.vocab/canonical";
        let mut reg = VocabRegistry::new();
        reg.insert(bridge_uri, &bridge);
        reg.insert(canonical_uri, &canonical);

        let mut m = DeliberationTallyMethod::new()
            .with_canonical_stance_vocab(canonical_uri, reg);

        // Vote authored against the bridge vocab using the
        // community-extended `endorse` slug. The observer must
        // canonicalise it to `agree` before counting.
        let s1 = "at://did:plc:c/dev.idiolect.deliberationStatement/s1";
        let cid = "bafyreidfcm4u3vnuph5ltwdpssiz3a4xfbm2otjrdisftwnbfmnxd6lsxm";
        let vote_record = AnyRecord::DeliberationVote(DeliberationVote {
            subject: StrongRecordRef {
                uri: idiolect_records::AtUri::parse(s1).expect("valid at-uri"),
                cid: idiolect_records::Cid::parse(cid).expect("valid cid"),
            },
            stance: DeliberationVoteStance::Other("endorse".to_owned()),
            stance_vocab: Some(VocabRef {
                uri: Some(idiolect_records::AtUri::parse(bridge_uri).expect("valid at-uri")),
                cid: None,
            }),
            weight: None,
            rationale: None,
            created_at: idiolect_records::Datetime::parse("2026-04-29T00:00:00Z")
                .expect("valid datetime"),
        });

        m.observe(&event(0, vote_record)).expect("observe");
        let snap = m.snapshot().expect("snapshot").expect("non-empty");
        // The tally bucket should now be `agree`, not `endorse`.
        let counts = snap["statementTallies"][0]["counts"]
            .as_array()
            .expect("array");
        let agree = counts
            .iter()
            .find(|c| c["stance"] == "agree")
            .expect("agree bucket present");
        assert_eq!(agree["count"], 1);
        let endorse = counts.iter().find(|c| c["stance"] == "endorse");
        assert!(endorse.is_none(), "endorse should have been canonicalised away");
    }

    #[test]
    fn unknown_source_vocab_falls_through_with_verbatim_slug() {
        use idiolect_records::vocab::VocabRegistry;
        let canonical_uri = "at://did:plc:x/dev.idiolect.vocab/canonical";
        // Empty registry: no vocabs known.
        let reg = VocabRegistry::new();
        let mut m = DeliberationTallyMethod::new()
            .with_canonical_stance_vocab(canonical_uri, reg);

        let s1 = "at://did:plc:c/dev.idiolect.deliberationStatement/s1";
        let cid = "bafyreidfcm4u3vnuph5ltwdpssiz3a4xfbm2otjrdisftwnbfmnxd6lsxm";
        m.observe(&event(
            0,
            vote(s1, cid, DeliberationVoteStance::Agree, None),
        ))
        .expect("observe");
        let snap = m.snapshot().expect("snapshot").expect("non-empty");
        // Slug passes through verbatim because the registry knows
        // nothing.
        let counts = snap["statementTallies"][0]["counts"]
            .as_array()
            .expect("array");
        assert!(counts.iter().any(|c| c["stance"] == "agree"));
    }
}
