//! Operational observation method for community migration health.
//!
//! The method keeps only the latest `MigrationRun` body at each
//! at-uri, so firehose updates replace earlier progress instead of
//! inflating counters. Deletes remove the run from the snapshot.

use std::collections::BTreeMap;

use idiolect_indexer::{IndexerAction, IndexerEvent};
use idiolect_records::AnyRecord;
use idiolect_records::generated::dev::idiolect::migration_run::{MigrationRun, MigrationRunStatus};
use idiolect_records::generated::dev::idiolect::observation::{
    ObservationMethod as ObservationMethodDescriptor, ObservationScope,
};

use crate::error::ObserverResult;
use crate::method::ObservationMethod;

/// Canonical method name.
pub const METHOD_NAME: &str = "migration-health";
/// Method version.
pub const METHOD_VERSION: &str = "1.0.0";

/// Latest migration state, keyed by record at-uri.
#[derive(Debug, Default, Clone)]
pub struct MigrationHealthMethod {
    runs: BTreeMap<String, MigrationRun>,
}

impl MigrationHealthMethod {
    /// Construct an empty migration observer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl ObservationMethod for MigrationHealthMethod {
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
                "Latest migration progress and failure state by community; updates replace prior checkpoints."
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
            self.runs.remove(&uri);
            return Ok(());
        }
        if let Some(AnyRecord::MigrationRun(run)) = &event.record {
            self.runs.insert(uri, run.clone());
        }
        Ok(())
    }

    fn snapshot(&self) -> ObserverResult<Option<serde_json::Value>> {
        if self.runs.is_empty() {
            return Ok(None);
        }

        #[derive(Default)]
        struct Totals {
            runs: u64,
            active: u64,
            completed: u64,
            failed_runs: u64,
            processed: u64,
            failed_records: u64,
            known_total: u64,
        }

        let mut communities: BTreeMap<String, Totals> = BTreeMap::new();
        for run in self.runs.values() {
            let totals = communities
                .entry(run.community.as_str().to_owned())
                .or_default();
            totals.runs = totals.runs.saturating_add(1);
            totals.processed = totals
                .processed
                .saturating_add(u64::try_from(run.processed).unwrap_or_default());
            totals.failed_records = totals
                .failed_records
                .saturating_add(u64::try_from(run.failed).unwrap_or_default());
            totals.known_total = totals.known_total.saturating_add(
                run.total
                    .and_then(|value| u64::try_from(value).ok())
                    .unwrap_or_default(),
            );
            match run.status {
                MigrationRunStatus::Planned
                | MigrationRunStatus::Running
                | MigrationRunStatus::Paused => totals.active = totals.active.saturating_add(1),
                MigrationRunStatus::Completed => {
                    totals.completed = totals.completed.saturating_add(1);
                }
                MigrationRunStatus::Failed => {
                    totals.failed_runs = totals.failed_runs.saturating_add(1);
                }
                MigrationRunStatus::RolledBack | MigrationRunStatus::Other(_) => {}
            }
        }

        let communities = communities
            .into_iter()
            .map(|(community, totals)| {
                (
                    community,
                    serde_json::json!({
                        "runs": totals.runs,
                        "active": totals.active,
                        "completed": totals.completed,
                        "failedRuns": totals.failed_runs,
                        "processed": totals.processed,
                        "failedRecords": totals.failed_records,
                        "knownTotal": totals.known_total,
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
    use idiolect_indexer::IndexerAction;
    use idiolect_records::{IdiolectFamily, Nsid, RecordFamily};

    fn event(action: IndexerAction, record: Option<AnyRecord>) -> IndexerEvent<IdiolectFamily> {
        IndexerEvent {
            seq: 1,
            live: true,
            did: "did:plc:operator".to_owned(),
            rev: "3l5".to_owned(),
            rkey: "run".to_owned(),
            collection: Nsid::parse("dev.idiolect.migrationRun").unwrap(),
            action,
            cid: None,
            record,
        }
    }

    #[test]
    fn updates_replace_and_deletes_remove_runs() {
        let mut method = MigrationHealthMethod::new();
        let mut run = idiolect_records::examples::migration_run();
        method
            .observe(&event(
                IndexerAction::Create,
                Some(<IdiolectFamily as RecordFamily>::AnyRecord::MigrationRun(
                    run.clone(),
                )),
            ))
            .unwrap();
        run.processed = 92;
        method
            .observe(&event(
                IndexerAction::Update,
                Some(<IdiolectFamily as RecordFamily>::AnyRecord::MigrationRun(
                    run,
                )),
            ))
            .unwrap();

        let snapshot = method.snapshot().unwrap().unwrap();
        assert_eq!(
            snapshot["communities"]["at://did:plc:community/dev.idiolect.community/3lcommunity"]["processed"],
            92
        );

        method.observe(&event(IndexerAction::Delete, None)).unwrap();
        assert!(method.snapshot().unwrap().is_none());
    }
}
