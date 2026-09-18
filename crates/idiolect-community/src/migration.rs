//! Durable migration-run state and checkpoint updates.

use crate::error::{CommunityError, CommunityResult};
use crate::governance::now;
use crate::model::{MigrationCheckpoint, MigrationFailure, MigrationRun, MigrationStatus};

/// Create a planned migration run for an approved change packet.
///
/// # Errors
///
/// Returns an error when the current timestamp cannot be formatted.
pub fn create_migration_run(
    id: impl Into<String>,
    change: impl Into<String>,
    lens: Option<String>,
    total: Option<u64>,
) -> CommunityResult<MigrationRun> {
    let timestamp = now()?;
    Ok(MigrationRun {
        id: id.into(),
        change: change.into(),
        lens,
        status: MigrationStatus::Planned,
        total,
        processed: 0,
        failed: 0,
        checkpoints: Vec::new(),
        failures: Vec::new(),
        created_at: timestamp.clone(),
        updated_at: timestamp,
    })
}

/// Apply a bounded progress update to a migration run.
///
/// Counts are increments. The run completes only when the expected total has
/// been reached and no failures remain; callers must explicitly mark a failed
/// run after deciding that its failures are not retryable.
///
/// # Errors
///
/// Returns an error when the run is terminal, a counter overflows, or the
/// checkpoint timestamp cannot be formatted.
pub fn advance_migration(
    run: &mut MigrationRun,
    processed: u64,
    failures: &[MigrationFailure],
    cursor: Option<String>,
    requested_status: Option<MigrationStatus>,
) -> CommunityResult<()> {
    if matches!(
        run.status,
        MigrationStatus::Completed | MigrationStatus::RolledBack
    ) {
        return Err(CommunityError::Invalid(format!(
            "migration {} is terminal ({:?})",
            run.id, run.status
        )));
    }
    run.processed = run
        .processed
        .checked_add(processed)
        .ok_or_else(|| CommunityError::Invalid("processed count overflow".to_owned()))?;
    run.failed = run
        .failed
        .checked_add(failures.len() as u64)
        .ok_or_else(|| CommunityError::Invalid("failure count overflow".to_owned()))?;
    // Keep a useful sample without allowing a pathological data source to grow
    // the operational record indefinitely.
    let remaining = 100usize.saturating_sub(run.failures.len());
    run.failures
        .extend(failures.iter().take(remaining).cloned());
    let timestamp = now()?;
    if let Some(cursor) = cursor {
        run.checkpoints.push(MigrationCheckpoint {
            cursor,
            processed: run.processed,
            recorded_at: timestamp.clone(),
        });
    }
    run.status = if let Some(status) = requested_status {
        status
    } else if run.total.is_some_and(|total| run.processed >= total) {
        if run.failed == 0 {
            MigrationStatus::Completed
        } else {
            MigrationStatus::Failed
        }
    } else {
        MigrationStatus::Running
    };
    run.updated_at = timestamp;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaching_total_completes_a_clean_run() {
        let mut run = create_migration_run("run-1", "change-1", None, Some(3)).unwrap();
        advance_migration(&mut run, 3, &[], Some("cursor-3".into()), None).unwrap();
        assert_eq!(run.status, MigrationStatus::Completed);
        assert_eq!(run.checkpoints.len(), 1);
    }

    #[test]
    fn failures_are_not_silently_green() {
        let mut run = create_migration_run("run-1", "change-1", None, Some(1)).unwrap();
        advance_migration(
            &mut run,
            1,
            &[MigrationFailure {
                record: "r1".into(),
                reason: "invalid target".into(),
            }],
            None,
            None,
        )
        .unwrap();
        assert_eq!(run.status, MigrationStatus::Failed);
    }
}
