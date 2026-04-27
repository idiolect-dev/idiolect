//! Top-level orchestrator that ties a firehose stream, an observer
//! handler, and a flush schedule together.
//!
//! Separated from [`idiolect_indexer::drive_indexer`] because the
//! observer's responsibility is strictly larger: it not only
//! dispatches decoded events, it also decides when to publish
//! aggregated observations. The flush cadence is a parameter — tests
//! pass `FlushSchedule::Manual` and drive it explicitly, while the
//! reference binary uses `FlushSchedule::EveryEvents(N)`.
//!
//! The transport-agnostic driver lives here so this crate's default
//! feature set stays decoupled from tokio time primitives. A
//! time-backed scheduler (`FlushSchedule::EveryInterval(duration)`)
//! lives in [`crate::daemon`] behind the `daemon` feature.

use idiolect_indexer::{CursorStore, EventStream, IndexerConfig};

use crate::error::ObserverResult;
use crate::handler::ObserverHandler;
use crate::method::ObservationMethod;
use crate::publisher::ObservationPublisher;

/// How often the driver should ask the handler to flush an
/// observation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum FlushSchedule {
    /// Do not flush automatically. The caller is responsible for
    /// invoking [`ObserverHandler::flush`] on their own cadence
    /// (useful for tests and for deployments that tie flushes to an
    /// external signal).
    #[default]
    Manual,
    /// Flush after every `N` events have been processed.
    EveryEvents(u32),
}

/// Run the observer loop against an event stream.
///
/// Mirrors [`idiolect_indexer::drive_indexer`]: pull events until the
/// stream closes or an error escapes. The observer-specific behavior
/// is (1) the handler is always an [`ObserverHandler`] and (2) a
/// flush is triggered after every `N` events when the schedule calls
/// for it.
///
/// # Errors
///
/// Returns the first [`crate::ObserverError`] that escapes the
/// stream, the handler, the cursor store, or the publisher.
pub async fn drive_observer<S, C, M, P>(
    stream: &mut S,
    handler: &ObserverHandler<M, P>,
    cursor_store: &C,
    config: &IndexerConfig,
    schedule: FlushSchedule,
) -> ObserverResult<()>
where
    S: EventStream,
    C: CursorStore,
    M: ObservationMethod,
    P: ObservationPublisher,
{
    // one-shot loop that inlines the indexer's core so we can
    // intercept each completed event for flush bookkeeping. the
    // indexer crate exposes its orchestrator as a `while let` loop
    // over `next_event`, but not a per-event hook, so we duplicate
    // the tiny skeleton here rather than add a callback API for the
    // one downstream consumer that wants it.
    let mut processed: u32 = 0;

    while let Some(raw) = stream.next_event().await? {
        // delegate one event through the indexer's per-event path.
        // we reuse `drive_indexer` indirectly by reconstructing its
        // body: prefix filter, decode, handler, cursor commit.
        process_one(raw, handler, cursor_store, config).await?;
        processed = processed.saturating_add(1);

        if let FlushSchedule::EveryEvents(n) = schedule
            && n > 0
            && processed.is_multiple_of(n)
        {
            handler.flush().await?;
        }
    }

    // final flush on clean stream close so callers do not lose the
    // tail of aggregated state. manual schedules skip this because
    // the caller owns flushing entirely.
    if !matches!(schedule, FlushSchedule::Manual) {
        handler.flush().await?;
    }

    Ok(())
}

/// Per-event body; kept private because it duplicates
/// [`idiolect_indexer::drive_indexer`]'s inner step. See the
/// function-level comment in `drive_observer` for why.
async fn process_one<C, M, P>(
    raw: idiolect_indexer::RawEvent,
    handler: &ObserverHandler<M, P>,
    cursor_store: &C,
    config: &IndexerConfig,
) -> ObserverResult<()>
where
    C: CursorStore,
    M: ObservationMethod,
    P: ObservationPublisher,
{
    use idiolect_indexer::{IndexerAction, IndexerError, IndexerEvent, RecordHandler};
    use idiolect_records::{IdiolectFamily, RecordFamily};

    // 1. family-membership filter. observers are domain-coupled to
    // the dev.idiolect.* family (encounters, corrections, etc.) by
    // construction; the bound is `IdiolectFamily` and out-of-family
    // commits are skipped via the family's contains predicate.
    if !IdiolectFamily::contains(&raw.collection) {
        return Ok(());
    }

    // 2. decode. delete actions carry no body.
    let record = match (raw.action, &raw.body) {
        (IndexerAction::Delete, _) => None,
        (_, Some(body)) => match IdiolectFamily::decode(&raw.collection, body.clone()) {
            Ok(Some(decoded)) => Some(decoded),
            Ok(None) => {
                return Err(IndexerError::FamilyContract(raw.collection.to_string()).into());
            }
            Err(e) => return Err(IndexerError::Decode(e).into()),
        },
        (_, None) => {
            return Err(IndexerError::MissingBody(format!(
                "{}/{}/{}",
                raw.did, raw.collection, raw.rkey,
            ))
            .into());
        }
    };

    let event = IndexerEvent::<IdiolectFamily> {
        seq: raw.seq,
        live: raw.live,
        did: raw.did,
        rev: raw.rev,
        rkey: raw.rkey,
        collection: raw.collection,
        action: raw.action,
        cid: raw.cid,
        record,
    };

    // 3. dispatch. this folds the event into the observer method.
    handler.handle(&event).await?;

    // 4. commit cursor on live events only.
    if event.live {
        cursor_store
            .commit(&config.subscription_id, event.seq)
            .await?;
    }

    Ok(())
}
