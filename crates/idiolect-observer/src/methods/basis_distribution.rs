//! Reference [`ObservationMethod`]: record counts grouped by the
//! variant of their `basis` field.
//!
//! Surfaces first-party vs third-party attribution rates across the
//! firehose: how many records ground themselves on community
//! policy, external signals, derived records, or assert directly.
//! Operators use this to spot when an indexer is dominated by
//! labelers rather than first-party publishers, or when an external
//! signal source suddenly stops producing records.
//!
//! Applies to encounter, correction, bounty, recommendation,
//! verification, observation, and retrospection — the seven
//! attitudinal records that carry a basis field.

use std::collections::BTreeMap;

use idiolect_indexer::IndexerEvent;
use idiolect_records::AnyRecord;
use idiolect_records::generated::defs::Basis;
use idiolect_records::generated::observation::{
    ObservationMethod as ObservationMethodDescriptor, ObservationScope,
};

use crate::error::ObserverResult;
use crate::method::ObservationMethod;

/// Canonical method name.
pub const METHOD_NAME: &str = "basis-distribution";

/// Method version.
pub const METHOD_VERSION: &str = "1.0.0";

/// Counts by basis variant, broken down by record kind.
#[derive(Debug, Clone, Default)]
pub struct BasisDistributionMethod {
    /// Total records observed across the supported kinds.
    total: u64,
    /// Counts keyed by `<record-kind>/<basis-variant>`, e.g.
    /// `"encounter/self_asserted"`. Records without a basis increment
    /// `<kind>/absent`.
    by_kind_and_basis: BTreeMap<String, u64>,
    /// Counts keyed by basis variant alone, across all record kinds.
    by_basis: BTreeMap<String, u64>,
}

impl BasisDistributionMethod {
    /// Construct an empty aggregator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Total records observed.
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.total
    }

    fn record_bucket(&mut self, kind: &str, basis: &str) {
        self.total = self.total.saturating_add(1);
        *self
            .by_kind_and_basis
            .entry(format!("{kind}/{basis}"))
            .or_insert(0) += 1;
        *self.by_basis.entry(basis.to_owned()).or_insert(0) += 1;
    }
}

impl ObservationMethod for BasisDistributionMethod {
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
                "Record counts grouped by basis variant (self-asserted, community-policy, \
                 external-signal, derived-from-record, absent), broken out by record kind. \
                 Surfaces first-party vs third-party attribution rates."
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
        let (kind, basis_tag) = match record {
            AnyRecord::Encounter(r) => ("encounter", tag_of(r.basis.as_ref())),
            AnyRecord::Correction(r) => ("correction", tag_of(r.basis.as_ref())),
            AnyRecord::Bounty(r) => ("bounty", tag_of(r.basis.as_ref())),
            AnyRecord::Recommendation(r) => ("recommendation", tag_of(r.basis.as_ref())),
            AnyRecord::Verification(r) => ("verification", tag_of(r.basis.as_ref())),
            AnyRecord::Observation(r) => ("observation", tag_of(r.basis.as_ref())),
            AnyRecord::Retrospection(r) => ("retrospection", tag_of(r.basis.as_ref())),
            // Other record kinds don't carry a basis today.
            _ => return Ok(()),
        };
        self.record_bucket(kind, basis_tag);
        Ok(())
    }

    fn snapshot(&self) -> ObserverResult<Option<serde_json::Value>> {
        if self.total == 0 {
            return Ok(None);
        }
        let by_basis: serde_json::Map<String, serde_json::Value> = self
            .by_basis
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::Number((*v).into())))
            .collect();
        let by_kind_and_basis: serde_json::Map<String, serde_json::Value> = self
            .by_kind_and_basis
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::Number((*v).into())))
            .collect();
        Ok(Some(serde_json::json!({
            "total": self.total,
            "byBasis": by_basis,
            "byKindAndBasis": by_kind_and_basis,
        })))
    }
}

fn tag_of(basis: Option<&Basis>) -> &'static str {
    match basis {
        None => "absent",
        Some(Basis::BasisSelfAsserted(_)) => "self_asserted",
        Some(Basis::BasisCommunityPolicy(_)) => "community_policy",
        Some(Basis::BasisExternalSignal(_)) => "external_signal",
        Some(Basis::BasisDerivedFromRecord(_)) => "derived_from_record",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use idiolect_indexer::{IndexerAction, IndexerEvent};
    use idiolect_records::Encounter;
    use idiolect_records::generated::defs::{
        BasisExternalSignal, BasisSelfAsserted, LensRef, SchemaRef, Use,
    };
    use idiolect_records::generated::encounter::EncounterKind;

    fn encounter_with_basis(basis: Option<Basis>) -> IndexerEvent {
        IndexerEvent {
            seq: 1,
            live: true,
            did: "did:plc:a".into(),
            rev: "r".into(),
            collection: "dev.idiolect.encounter".into(),
            rkey: "e1".into(),
            action: IndexerAction::Create,
            cid: None,
            record: Some(AnyRecord::Encounter(Encounter {
                annotations: None,
                basis,
                downstream_result: None,
                holder: None,
                kind: EncounterKind::InvocationLog,
                lens: LensRef {
                    cid: None,
                    direction: None,
                    uri: Some("at://did:plc:x/dev.panproto.schema.lens/l".into()),
                },
                occurred_at: "2026-04-23T00:00:00Z".into(),
                produced_output: None,
                r#use: Use {
                    action: "x".into(),
                    material: None,
                    actor: None,
                    purpose: None,
                    action_vocabulary: None,
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

    #[test]
    fn buckets_basis_variants() {
        let mut m = BasisDistributionMethod::new();
        m.observe(&encounter_with_basis(None)).unwrap();
        m.observe(&encounter_with_basis(Some(Basis::BasisSelfAsserted(
            BasisSelfAsserted {},
        ))))
        .unwrap();
        m.observe(&encounter_with_basis(Some(Basis::BasisExternalSignal(
            BasisExternalSignal {
                url: "https://x".into(),
                signal_type: None,
                description: None,
            },
        ))))
        .unwrap();

        let snap = m.snapshot().unwrap().unwrap();
        assert_eq!(snap["total"], 3);
        assert_eq!(snap["byBasis"]["absent"], 1);
        assert_eq!(snap["byBasis"]["self_asserted"], 1);
        assert_eq!(snap["byBasis"]["external_signal"], 1);
        assert_eq!(snap["byKindAndBasis"]["encounter/external_signal"], 1);
    }
}
