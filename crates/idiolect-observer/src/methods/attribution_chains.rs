//! Reference [`ObservationMethod`]: aggregate `dev.idiolect.belief`
//! records by the DID that holds them and by the record they point
//! at.
//!
//! Belief records are the nested-attitude primitive: a labeler
//! publishes a record in its own repo attributing an attitude to
//! another party, then publishes a belief record whose subject is a
//! strong-ref to the attribution. This method surfaces how many
//! belief records each holder publishes and which records accumulate
//! the most beliefs — useful for evaluating labeler coverage and
//! spotting records that many third parties want to reason about.

use std::collections::BTreeMap;

use idiolect_indexer::IndexerEvent;
use idiolect_records::AnyRecord;
use idiolect_records::generated::dev::idiolect::observation::{
    ObservationMethod as ObservationMethodDescriptor, ObservationScope,
};

use crate::error::ObserverResult;
use crate::method::ObservationMethod;

/// Canonical method name.
pub const METHOD_NAME: &str = "attribution-chains";

/// Method version.
pub const METHOD_VERSION: &str = "1.0.0";

/// Counts of belief records by holder and by subject.
#[derive(Debug, Clone, Default)]
pub struct AttributionChainsMethod {
    total: u64,
    /// Number of belief records whose `holder` differs from the
    /// record's repo DID. Surfaces the rate of third-party
    /// attribution vs self-reporting.
    third_party: u64,
    /// Counts keyed by the belief's holder DID (where present) —
    /// otherwise by the repo DID.
    by_holder: BTreeMap<String, u64>,
    /// Counts keyed by the subject record's at-uri.
    by_subject: BTreeMap<String, u64>,
}

impl AttributionChainsMethod {
    /// Construct an empty aggregator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Total belief records observed.
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.total
    }
}

impl ObservationMethod for AttributionChainsMethod {
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
                "Counts of dev.idiolect.belief records by holder DID and by subject at-uri. \
                 Surfaces third-party attribution coverage."
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
        let Some(AnyRecord::Belief(belief)) = &event.record else {
            return Ok(());
        };
        self.total = self.total.saturating_add(1);
        // The belief carries a typed Did; the firehose event's `did`
        // is still the raw atproto string from the commit envelope.
        // Compare on the borrowed `&str` (Did derefs).
        let holder = belief
            .holder
            .as_ref()
            .map_or_else(|| event.did.clone(), |d| d.as_str().to_owned());
        if holder != event.did {
            self.third_party = self.third_party.saturating_add(1);
        }
        *self.by_holder.entry(holder).or_insert(0) += 1;
        *self
            .by_subject
            .entry(belief.subject.uri.as_str().to_owned())
            .or_insert(0) += 1;
        Ok(())
    }

    fn snapshot(&self) -> ObserverResult<Option<serde_json::Value>> {
        if self.total == 0 {
            return Ok(None);
        }
        let by_holder: serde_json::Map<String, serde_json::Value> = self
            .by_holder
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::Number((*v).into())))
            .collect();
        let by_subject: serde_json::Map<String, serde_json::Value> = self
            .by_subject
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::Number((*v).into())))
            .collect();
        Ok(Some(serde_json::json!({
            "total": self.total,
            "thirdParty": self.third_party,
            "distinctHolders": self.by_holder.len(),
            "distinctSubjects": self.by_subject.len(),
            "byHolder": by_holder,
            "bySubject": by_subject,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use idiolect_indexer::{IndexerAction, IndexerEvent};
    use idiolect_records::Belief;
    use idiolect_records::generated::dev::idiolect::defs::{StrongRecordRef, Visibility};

    fn belief_event(repo: &str, holder: Option<&str>, subject: &str) -> IndexerEvent {
        IndexerEvent {
            seq: 1,
            live: true,
            did: repo.to_owned(),
            rev: "r".into(),
            collection: idiolect_records::Nsid::parse("dev.idiolect.belief").expect("valid nsid"),
            rkey: "b1".into(),
            action: IndexerAction::Create,
            cid: None,
            record: Some(AnyRecord::Belief(Belief {
                annotations: None,
                basis: None,
                holder: holder.map(|h| idiolect_records::Did::parse(h).expect("valid did")),
                subject: StrongRecordRef {
                    uri: idiolect_records::AtUri::parse(subject).expect("valid at-uri"),
                    cid: "bafyxxxx".into(),
                },
                visibility: Some(Visibility::PublicDetailed),
                occurred_at: idiolect_records::Datetime::parse("2026-04-23T00:00:00Z")
                    .expect("valid datetime"),
            })),
        }
    }

    #[test]
    fn counts_third_party_attributions() {
        let mut m = AttributionChainsMethod::new();
        // first-party (repo == holder default): no third-party bump.
        m.observe(&belief_event(
            "did:plc:self",
            None,
            "at://did:plc:x/dev.idiolect.test/z",
        ))
        .unwrap();
        // third-party: holder differs from repo DID.
        m.observe(&belief_event(
            "did:plc:labeler",
            Some("did:plc:user"),
            "at://did:plc:x/dev.idiolect.test/z",
        ))
        .unwrap();
        m.observe(&belief_event(
            "did:plc:labeler",
            Some("did:plc:user"),
            "at://did:plc:x/dev.idiolect.test/w",
        ))
        .unwrap();

        let snap = m.snapshot().unwrap().unwrap();
        assert_eq!(snap["total"], 3);
        assert_eq!(snap["thirdParty"], 2);
        assert_eq!(snap["byHolder"]["did:plc:self"], 1);
        assert_eq!(snap["byHolder"]["did:plc:user"], 2);
        assert_eq!(snap["bySubject"]["at://did:plc:x/dev.idiolect.test/z"], 2);
    }
}
