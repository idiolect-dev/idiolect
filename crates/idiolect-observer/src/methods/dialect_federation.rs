//! Reference [`ObservationMethod`]: federated-dialect watcher.
//!
//! Tracks a caller-configured set of communities (by DID). Whenever a
//! `Dialect` record authored by one of those DIDs passes through the
//! firehose, the method updates its snapshot of that community's
//! current `idiolects` + `preferred_lenses` and computes the delta
//! against the prior snapshot. On flush, the snapshot carries:
//!
//! - the watched community DIDs,
//! - each community's current dialect at-uri (highest-`createdAt`),
//! - the set of lens URIs newly added, updated, or removed since
//!   the previous snapshot.
//!
//! Purpose: the lexicon-federation primitive in hyperdeclarative is
//! records-not-registries (P2). A subscriber that wants to follow
//! another community's lens choices reads their dialect records off
//! the firehose; this method is the reference "how to read them"
//! shape, so downstream orchestrators can consume a stable
//! observation body rather than each re-walking dialect history.

use std::collections::{BTreeMap, BTreeSet};

use idiolect_indexer::IndexerEvent;
use idiolect_records::AnyRecord;
use idiolect_records::generated::observation::{
    ObservationMethod as ObservationMethodDescriptor, ObservationScope,
};

use crate::error::ObserverResult;
use crate::method::ObservationMethod;

/// Canonical method name.
pub const METHOD_NAME: &str = "dialect-federation";

/// Method version. Bump when the output shape changes in a way that
/// invalidates comparison with older snapshots.
pub const METHOD_VERSION: &str = "1.0.0";

/// Per-community current-state snapshot.
#[derive(Debug, Default, Clone)]
struct CommunityState {
    /// At-uri of the most-recent dialect record the method saw for
    /// this community. `None` before the first dialect from that
    /// community flows through.
    current_dialect_uri: Option<String>,
    /// Timestamp of the current dialect (for recency comparisons).
    current_created_at: Option<String>,
    /// Lens URIs the current dialect marks as preferred.
    current_lenses: BTreeSet<String>,
    /// Snapshot emitted at the previous flush, so the next snapshot
    /// can compute added/removed deltas.
    previous_lenses_at_last_flush: BTreeSet<String>,
}

/// Reference method that watches dialect records from a fixed set
/// of community-owning DIDs.
pub struct DialectFederationMethod {
    watched: BTreeSet<String>,
    state: BTreeMap<String, CommunityState>,
}

impl DialectFederationMethod {
    /// Construct with an empty watch list. Use
    /// [`watch`](Self::watch) to add DIDs before registering the
    /// method with a driver.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            watched: BTreeSet::new(),
            state: BTreeMap::new(),
        }
    }

    /// Add a DID to the watch list. Dialects authored by DIDs not in
    /// this set are ignored.
    pub fn watch(&mut self, did: impl Into<String>) -> &mut Self {
        let did = did.into();
        self.state.entry(did.clone()).or_default();
        self.watched.insert(did);
        self
    }

    /// How many DIDs are being watched.
    #[must_use]
    pub fn watch_count(&self) -> usize {
        self.watched.len()
    }
}

impl Default for DialectFederationMethod {
    fn default() -> Self {
        Self::new()
    }
}

impl ObservationMethod for DialectFederationMethod {
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
                "Watches dialect records from a configured set of communities and reports lens-set changes since the previous snapshot."
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
        if !self.watched.contains(&event.did) {
            return Ok(());
        }
        let Some(AnyRecord::Dialect(dialect)) = &event.record else {
            return Ok(());
        };

        let state = self.state.entry(event.did.clone()).or_default();
        // Keep the latest-by-createdAt dialect. Lexicons allow
        // multiple revisions; the federation view follows the most
        // recent one per DID.
        let incoming_newer = state
            .current_created_at
            .as_ref()
            .is_none_or(|cur| dialect.created_at.as_str() > cur.as_str());
        if !incoming_newer {
            return Ok(());
        }

        let at_uri = format!(
            "at://{did}/dev.idiolect.dialect/{rkey}",
            did = event.did,
            rkey = event.rkey,
        );
        let lenses: BTreeSet<String> = dialect
            .preferred_lenses
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .filter_map(|lr| lr.uri.clone())
            .collect();

        state.current_dialect_uri = Some(at_uri);
        state.current_created_at = Some(dialect.created_at.clone());
        state.current_lenses = lenses;
        Ok(())
    }

    fn snapshot(&self) -> ObserverResult<Option<serde_json::Value>> {
        if self.watched.is_empty() {
            return Ok(None);
        }
        let communities: Vec<serde_json::Value> = self
            .watched
            .iter()
            .map(|did| {
                let s = self.state.get(did).cloned().unwrap_or_default();
                let added: Vec<&String> = s
                    .current_lenses
                    .difference(&s.previous_lenses_at_last_flush)
                    .collect();
                let removed: Vec<&String> = s
                    .previous_lenses_at_last_flush
                    .difference(&s.current_lenses)
                    .collect();
                serde_json::json!({
                    "did": did,
                    "currentDialect": s.current_dialect_uri,
                    "currentCreatedAt": s.current_created_at,
                    "lensCount": s.current_lenses.len(),
                    "addedLenses": added,
                    "removedLenses": removed,
                })
            })
            .collect();
        Ok(Some(serde_json::json!({
            "watchedCount": self.watched.len(),
            "communities": communities,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use idiolect_indexer::IndexerAction;
    use idiolect_records::generated::defs::LensRef;
    use idiolect_records::generated::dialect::Dialect;

    fn dialect(created_at: &str, lens_uris: &[&str]) -> Dialect {
        Dialect {
            created_at: created_at.to_owned(),
            deprecations: None,
            description: None,
            idiolects: None,
            name: "watched".into(),
            owning_community: "at://did:plc:c/dev.idiolect.community/main".into(),
            preferred_lenses: Some(
                lens_uris
                    .iter()
                    .map(|u| LensRef {
                        cid: None,
                        direction: None,
                        uri: Some((*u).to_owned()),
                    })
                    .collect(),
            ),
            previous_version: None,
            version: Some("1".into()),
        }
    }

    fn event_from(did: &str, rkey: &str, dialect: Dialect) -> IndexerEvent {
        IndexerEvent {
            seq: 1,
            live: true,
            did: did.to_owned(),
            rev: "r".into(),
            collection: idiolect_records::Nsid::parse("dev.idiolect.dialect").expect("valid nsid"),
            rkey: rkey.to_owned(),
            action: IndexerAction::Create,
            cid: None,
            record: Some(AnyRecord::Dialect(dialect)),
        }
    }

    #[test]
    fn ignores_events_from_unwatched_dids() {
        let mut m = DialectFederationMethod::new();
        m.watch("did:plc:watched");
        m.observe(&event_from(
            "did:plc:other",
            "d",
            dialect("2026-04-21T00:00:00Z", &["at://lens/1"]),
        ))
        .unwrap();
        // Snapshot shows the watched DID with no current dialect.
        let snap = m.snapshot().unwrap().unwrap();
        assert_eq!(snap["watchedCount"], 1);
        assert!(snap["communities"][0]["currentDialect"].is_null());
    }

    #[test]
    fn records_latest_dialect_by_created_at() {
        let mut m = DialectFederationMethod::new();
        m.watch("did:plc:w");

        m.observe(&event_from(
            "did:plc:w",
            "d1",
            dialect("2026-04-21T00:00:00Z", &["at://l/1"]),
        ))
        .unwrap();
        m.observe(&event_from(
            "did:plc:w",
            "d2",
            dialect("2026-04-22T00:00:00Z", &["at://l/2", "at://l/3"]),
        ))
        .unwrap();

        let snap = m.snapshot().unwrap().unwrap();
        let community = &snap["communities"][0];
        assert_eq!(community["currentCreatedAt"], "2026-04-22T00:00:00Z");
        assert_eq!(community["lensCount"], 2);
        // First flush: every current lens is "added" since previous
        // snapshot was empty.
        let added = community["addedLenses"].as_array().unwrap();
        assert_eq!(added.len(), 2);
    }

    #[test]
    fn stale_dialect_does_not_overwrite_newer_current() {
        let mut m = DialectFederationMethod::new();
        m.watch("did:plc:w");

        m.observe(&event_from(
            "did:plc:w",
            "d-new",
            dialect("2026-04-22T00:00:00Z", &["at://l/new"]),
        ))
        .unwrap();
        // Older createdAt arrives second — should not overwrite.
        m.observe(&event_from(
            "did:plc:w",
            "d-old",
            dialect("2026-04-21T00:00:00Z", &["at://l/old"]),
        ))
        .unwrap();

        let snap = m.snapshot().unwrap().unwrap();
        assert_eq!(
            snap["communities"][0]["currentCreatedAt"],
            "2026-04-22T00:00:00Z"
        );
        assert_eq!(snap["communities"][0]["lensCount"], 1);
    }

    #[test]
    fn snapshot_returns_none_when_watch_list_empty() {
        let m = DialectFederationMethod::new();
        assert!(m.snapshot().unwrap().is_none());
    }
}
