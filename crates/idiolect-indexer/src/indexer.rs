//! The orchestrator that glues [`EventStream`], [`RecordHandler`],
//! and [`CursorStore`] into a loop.
//!
//! `drive_indexer` runs until the stream cleanly returns `Ok(None)`
//! (bounded mocks in tests) or hits an error (production firehose).
//! At steady state the loop is:
//!
//! 1. Pull a [`RawEvent`] from the stream.
//! 2. Ask the configured [`RecordFamily`] whether the commit's
//!    collection NSID is a member. Out-of-family commits are
//!    skipped silently.
//! 3. Decode in-family bodies via
//!    [`RecordFamily::decode`](idiolect_records::RecordFamily::decode).
//! 4. Invoke the handler with the decoded
//!    [`IndexerEvent`](crate::IndexerEvent).
//! 5. On a live event, commit the cursor.
//!
//! Backfill commits advance the decode/handler path but leave the
//! cursor pinned: tap replays backfill whenever a repo resyncs, so a
//! cursor committed against a backfill event would be stale the
//! moment the next resync happens.

use idiolect_records::{IdiolectFamily, RecordFamily};

use crate::cursor::CursorStore;
use crate::error::IndexerError;
use crate::event::{IndexerAction, IndexerEvent};
use crate::handler::RecordHandler;
use crate::stream::{EventStream, RawEvent};

/// Configuration for a single run of [`drive_indexer`].
///
/// Membership in the family is determined by the type parameter
/// `F`'s [`RecordFamily::contains`](idiolect_records::RecordFamily::contains).
/// Pre-v0.5 the indexer carried an `nsid_prefix` field for the same
/// purpose; that's gone. One source of truth, the family trait.
#[derive(Debug, Clone)]
pub struct IndexerConfig {
    /// Cursor slot identifier. One per firehose endpoint the appview
    /// subscribes to.
    pub subscription_id: String,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            subscription_id: "idiolect-indexer".to_owned(),
        }
    }
}

/// Run the indexer loop until the stream closes or an error
/// terminates it.
///
/// Generic over `F: RecordFamily`. The `IdiolectFamily` default
/// keeps the public symbol source-compatible for callers that
/// don't care about the family parameter; downstream consumers
/// (e.g. `layers-pub`) write `drive_indexer::<LayersFamily, _, _, _>(...)`.
///
/// # Errors
///
/// Returns the first [`IndexerError`] that escapes the stream, the
/// handler, or the cursor store. Decode errors for in-family
/// records are fatal (a known collection that fails to decode is a
/// data-shape drift); out-of-family NSIDs are silently filtered via
/// `F::contains`.
pub async fn drive_indexer<F, S, H, C>(
    stream: &mut S,
    handler: &H,
    cursor_store: &C,
    config: &IndexerConfig,
) -> Result<(), IndexerError>
where
    F: RecordFamily,
    S: EventStream,
    H: RecordHandler<F>,
    C: CursorStore,
{
    while let Some(raw) = stream.next_event().await? {
        process_event::<F, _, _>(raw, handler, cursor_store, config).await?;
    }

    Ok(())
}

/// Convenience alias: run [`drive_indexer`] against the
/// `dev.idiolect.*` family. Equivalent to
/// `drive_indexer::<IdiolectFamily, _, _, _>(...)`.
///
/// # Errors
///
/// Same as [`drive_indexer`]: surfaces the first
/// [`IndexerError`] that escapes the stream, the handler, or the
/// cursor store.
pub async fn drive_idiolect_indexer<S, H, C>(
    stream: &mut S,
    handler: &H,
    cursor_store: &C,
    config: &IndexerConfig,
) -> Result<(), IndexerError>
where
    S: EventStream,
    H: RecordHandler<IdiolectFamily>,
    C: CursorStore,
{
    drive_indexer::<IdiolectFamily, _, _, _>(stream, handler, cursor_store, config).await
}

/// Process one raw event.
///
/// Extracted from [`drive_indexer`] so the "per-event" path is
/// individually testable without standing up a full loop.
async fn process_event<F, H, C>(
    raw: RawEvent,
    handler: &H,
    cursor_store: &C,
    config: &IndexerConfig,
) -> Result<(), IndexerError>
where
    F: RecordFamily,
    H: RecordHandler<F>,
    C: CursorStore,
{
    // 1. family membership check. commits whose collection isn't in
    // the configured family are not our business; skip without
    // decoding. The membership predicate is whatever the family's
    // RecordFamily::contains returns — typically a `matches!` over
    // the family's NSIDs.
    if !F::contains(&raw.collection) {
        return Ok(());
    }

    // 2. decode the body, if any. deletes carry no body.
    let record: Option<F::AnyRecord> = match (raw.action, &raw.body) {
        (IndexerAction::Delete, _) => None,
        (_, Some(body)) => match F::decode(&raw.collection, body.clone()) {
            Ok(Some(decoded)) => Some(decoded),
            Ok(None) => {
                // F::contains said yes but F::decode came back None.
                // That's a family-impl bug: contains and decode have
                // diverged. Surface it loudly rather than silently
                // dropping the record.
                return Err(IndexerError::Handler(format!(
                    "family decode returned None for in-family nsid {}",
                    raw.collection,
                )));
            }
            Err(e) => return Err(IndexerError::Decode(e)),
        },
        (_, None) => {
            // create / update without a body is malformed; tap should
            // never hand us one, but guard against a misbehaving
            // upstream.
            return Err(IndexerError::MissingBody(format!(
                "{}/{}/{}",
                raw.did, raw.collection, raw.rkey,
            )));
        }
    };

    let event = IndexerEvent::<F> {
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

    // 3. dispatch. handler errors are fatal — this is firehose data
    // flow; if the appview cannot ingest, catching up silently is
    // worse than halting.
    handler.handle(&event).await?;

    // 4. commit cursor on live events only.
    if event.live {
        cursor_store
            .commit(&config.subscription_id, event.seq)
            .await?;
    }

    Ok(())
}
