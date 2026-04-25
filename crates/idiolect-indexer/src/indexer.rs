//! The orchestrator that glues [`EventStream`], [`RecordHandler`],
//! and [`CursorStore`] into a loop.
//!
//! `drive_indexer` runs until the stream cleanly returns `Ok(None)`
//! (bounded mocks in tests) or hits an error (production firehose).
//! At steady state the loop is:
//!
//! 1. Pull a [`RawEvent`] from the stream.
//! 2. If the collection is in the `dev.idiolect.*` family, decode the
//!    body through [`idiolect_records::decode_record`]. Unknown nsids
//!    are skipped silently (the appview's filter, not an error).
//! 3. Invoke the handler with the decoded
//!    [`IndexerEvent`](crate::IndexerEvent).
//! 4. On a live event, commit the cursor.
//!
//! Backfill commits advance the decode/handler path but leave the
//! cursor pinned: tap replays backfill whenever a repo resyncs, so a
//! cursor committed against a backfill event would be stale the
//! moment the next resync happens.

use idiolect_records::{AnyRecord, DecodeError, Nsid, decode_record};

use crate::cursor::CursorStore;
use crate::error::IndexerError;
use crate::event::{IndexerAction, IndexerEvent};
use crate::handler::RecordHandler;
use crate::stream::{EventStream, RawEvent};

/// Configuration for a single run of [`drive_indexer`].
///
/// Kept as a struct (rather than a positional argument list) so
/// callers do not mis-order `subscription_id` and `nsid_prefix`: the
/// former identifies the cursor slot, the latter filters the
/// collection set, and confusing them silently indexes the wrong
/// data.
#[derive(Debug, Clone)]
pub struct IndexerConfig {
    /// Cursor slot identifier. One per firehose endpoint the appview
    /// subscribes to.
    pub subscription_id: String,
    /// Only commits whose `collection` starts with this prefix are
    /// decoded and dispatched. `"dev.idiolect."` is the canonical
    /// value for an idiolect-only indexer. An empty string disables
    /// the filter and routes every collection through decode, which
    /// is only useful for debugging.
    pub nsid_prefix: String,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            subscription_id: "idiolect-indexer".to_owned(),
            nsid_prefix: "dev.idiolect.".to_owned(),
        }
    }
}

/// Run the indexer loop until the stream closes or an error
/// terminates it.
///
/// # Errors
///
/// Returns the first [`IndexerError`] that escapes the stream, the
/// handler, or the cursor store. Decode errors for individual
/// records whose nsid starts with `nsid_prefix` are fatal (a known
/// collection that fails to decode is a data-shape drift); unknown
/// nsids outside the prefix are silently filtered.
pub async fn drive_indexer<S, H, C>(
    stream: &mut S,
    handler: &H,
    cursor_store: &C,
    config: &IndexerConfig,
) -> Result<(), IndexerError>
where
    S: EventStream,
    H: RecordHandler,
    C: CursorStore,
{
    while let Some(raw) = stream.next_event().await? {
        process_event(raw, handler, cursor_store, config).await?;
    }

    Ok(())
}

/// Process one raw event.
///
/// Extracted from [`drive_indexer`] so the "per-event" path is
/// individually testable without standing up a full loop.
async fn process_event<H, C>(
    raw: RawEvent,
    handler: &H,
    cursor_store: &C,
    config: &IndexerConfig,
) -> Result<(), IndexerError>
where
    H: RecordHandler,
    C: CursorStore,
{
    // 1. prefix filter. commits outside the configured nsid prefix
    // are not our business; skip without decoding. The prefix is
    // matched against the collection's authority on segment
    // boundaries via `Nsid::starts_with_authority`, so a
    // configured prefix of `dev.idiolect` matches
    // `dev.idiolect.encounter` but never `dev.idiolectx.foo`.
    if !config.nsid_prefix.is_empty() && !raw.collection.starts_with_authority(&config.nsid_prefix)
    {
        return Ok(());
    }

    // 2. decode the body, if any. deletes carry no body.
    let record = match (raw.action, &raw.body) {
        (IndexerAction::Delete, _) => None,
        (_, Some(body)) => Some(decode_body(&raw.collection, body.clone())?),
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

    let event = IndexerEvent {
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

/// Decode a record body against the nsid carried by the commit.
///
/// Wraps `decode_record`, promoting unknown nsids into
/// [`IndexerError::Decode`] only after the prefix filter has already
/// decided the commit is in the idiolect family. Any error here is a
/// data drift, not a routing oversight.
fn decode_body(nsid: &Nsid, body: serde_json::Value) -> Result<AnyRecord, IndexerError> {
    decode_record(nsid, body).map_err(|err| match err {
        // UnknownNsid after a prefix match means the prefix-filter
        // accepted a collection that decode_record does not know
        // about — a codegen-vs-runtime drift we want to surface
        // loudly rather than swallow.
        DecodeError::UnknownNsid(_) | DecodeError::Serde(_) => IndexerError::Decode(err),
    })
}
