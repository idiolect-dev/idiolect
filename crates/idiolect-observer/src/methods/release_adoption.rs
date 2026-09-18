//! Operational observation method for signed community releases.
//!
//! The output provides a small adoption surface for dashboards and
//! federation tooling: release, artifact, dependency, and distinct
//! signer counts per community.

use std::collections::{BTreeMap, BTreeSet};

use idiolect_indexer::{IndexerAction, IndexerEvent};
use idiolect_records::generated::dev::idiolect::observation::{
    ObservationMethod as ObservationMethodDescriptor, ObservationScope,
};
use idiolect_records::{AnyRecord, CommunityRelease};

use crate::error::ObserverResult;
use crate::method::ObservationMethod;

/// Canonical method name.
pub const METHOD_NAME: &str = "release-adoption";
/// Method version.
pub const METHOD_VERSION: &str = "1.0.0";

/// Latest community releases, keyed by record at-uri.
#[derive(Debug, Default, Clone)]
pub struct ReleaseAdoptionMethod {
    releases: BTreeMap<String, CommunityRelease>,
}

impl ReleaseAdoptionMethod {
    /// Construct an empty release observer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl ObservationMethod for ReleaseAdoptionMethod {
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
                "Published releases, artifacts, federation dependencies, and distinct signers by community."
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
        let uri = event.at_uri();
        if event.action == IndexerAction::Delete {
            self.releases.remove(&uri);
            return Ok(());
        }
        if let Some(AnyRecord::CommunityRelease(release)) = &event.record {
            self.releases.insert(uri, release.clone());
        }
        Ok(())
    }

    fn snapshot(&self) -> ObserverResult<Option<serde_json::Value>> {
        if self.releases.is_empty() {
            return Ok(None);
        }

        #[derive(Default)]
        struct Totals {
            releases: u64,
            artifacts: u64,
            dependencies: u64,
            signed_releases: u64,
            signers: BTreeSet<String>,
        }

        let mut communities: BTreeMap<String, Totals> = BTreeMap::new();
        for release in self.releases.values() {
            let totals = communities
                .entry(release.community.as_str().to_owned())
                .or_default();
            totals.releases = totals.releases.saturating_add(1);
            totals.artifacts = totals.artifacts.saturating_add(
                u64::try_from(release.artifacts.as_ref().map_or(0, Vec::len)).unwrap_or(u64::MAX),
            );
            totals.dependencies = totals.dependencies.saturating_add(
                u64::try_from(release.dependencies.as_ref().map_or(0, Vec::len))
                    .unwrap_or(u64::MAX),
            );
            if !release.signatures.is_empty() {
                totals.signed_releases = totals.signed_releases.saturating_add(1);
            }
            totals.signers.extend(
                release
                    .signatures
                    .iter()
                    .map(|signature| signature.signer.as_str().to_owned()),
            );
        }

        let communities = communities
            .into_iter()
            .map(|(community, totals)| {
                (
                    community,
                    serde_json::json!({
                        "releases": totals.releases,
                        "signedReleases": totals.signed_releases,
                        "artifacts": totals.artifacts,
                        "dependencies": totals.dependencies,
                        "distinctSigners": totals.signers.len(),
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>();

        Ok(Some(serde_json::json!({ "communities": communities })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use idiolect_records::Nsid;

    fn event(record: CommunityRelease) -> IndexerEvent {
        IndexerEvent {
            seq: 1,
            live: true,
            did: "did:plc:publisher".to_owned(),
            rev: "3l5".to_owned(),
            rkey: "release".to_owned(),
            collection: Nsid::parse("dev.idiolect.communityRelease").unwrap(),
            action: IndexerAction::Create,
            cid: None,
            record: Some(AnyRecord::CommunityRelease(record)),
        }
    }

    #[test]
    fn counts_signed_release_surface_by_community() {
        let mut method = ReleaseAdoptionMethod::new();
        method
            .observe(&event(idiolect_records::examples::community_release()))
            .unwrap();
        let snapshot = method.snapshot().unwrap().unwrap();
        let community =
            &snapshot["communities"]["at://did:plc:community/dev.idiolect.community/3lcommunity"];
        assert_eq!(community["releases"], 1);
        assert_eq!(community["signedReleases"], 1);
        assert_eq!(community["distinctSigners"], 1);
    }
}
