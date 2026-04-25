//! Tapped-backed [`EventStream`](crate::EventStream) adapter.
//!
//! Feature-gated (`firehose-tapped`) so the core crate stays
//! transport-agnostic. When enabled, this module adapts a
//! [`tapped::EventReceiver`] into an [`EventStream`] by pulling one
//! event at a time, projecting record events onto [`RawEvent`] with
//! an owned json body, and acknowledging the event when the
//! [`tapped::ReceivedEvent`] wrapper drops.
//!
//! Identity events are skipped: the indexer processes `dev.idiolect.*`
//! record commits, and handle / status changes flow through a
//! different side-channel in the appview.

use tapped::{Event, EventReceiver};

use crate::error::IndexerError;
use crate::event::IndexerAction;
use crate::stream::{EventStream, RawEvent};

/// Wraps a [`tapped::EventReceiver`] behind the [`EventStream`] trait.
///
/// Ownership of the receiver moves into the adapter; dropping the
/// adapter also drops the receiver, which ends the tap acknowledgment
/// channel.
pub struct TappedEventStream {
    /// The receiving half of the tapped channel.
    receiver: EventReceiver,
}

impl TappedEventStream {
    /// Construct an adapter from an existing tapped receiver.
    #[must_use]
    pub const fn new(receiver: EventReceiver) -> Self {
        Self { receiver }
    }
}

impl EventStream for TappedEventStream {
    async fn next_event(&mut self) -> Result<Option<RawEvent>, IndexerError> {
        // loop until we get a record event or the channel closes.
        // identity events are filtered at this layer because the
        // indexer's `EventStream` shape carries a record body.
        loop {
            let received = match self.receiver.recv().await {
                Ok(received) => received,
                Err(err) => {
                    // distinguish clean close from transport failure.
                    // `tapped::Error::ChannelClosed` stringifies with
                    // the word "closed", everything else is transport
                    // noise we bubble up.
                    let msg = err.to_string();
                    if msg.to_ascii_lowercase().contains("closed") {
                        return Ok(None);
                    }

                    return Err(IndexerError::Stream(msg));
                }
            };

            // `received` derefs to `tapped::Event`. non-record
            // events (identity updates, future tapped variants) get
            // filtered at this layer. the `ReceivedEvent` drops at
            // the end of the loop iteration, which acknowledges it
            // to tap. tapped::Event is non_exhaustive, so the
            // let-else catches today's Identity and anything added
            // in a future release.
            let Event::Record(record) = &received.event else {
                continue;
            };

            // lift tapped's action enum into ours. non_exhaustive on
            // both sides; a future tap variant lands here as an
            // IndexerError::Stream rather than silently misrouting.
            let action = match record.action {
                tapped::RecordAction::Create => IndexerAction::Create,
                tapped::RecordAction::Update => IndexerAction::Update,
                tapped::RecordAction::Delete => IndexerAction::Delete,
                _ => {
                    return Err(IndexerError::Stream(format!(
                        "unrecognized tapped action on {}/{}/{}",
                        record.did, record.collection, record.rkey,
                    )));
                }
            };

            // Parse the tapped collection string into a typed Nsid at
            // the stream-decode boundary. Same warn-and-skip policy as
            // the jetstream adapter: a single malformed event must not
            // bring down the firehose.
            let collection = match idiolect_records::Nsid::parse(&record.collection) {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!(
                        did = %record.did,
                        rkey = %record.rkey,
                        collection = %record.collection,
                        error = %e,
                        "skipping tapped commit with invalid NSID collection",
                    );
                    continue;
                }
            };

            // materialize the body into an owned serde_json::Value so
            // the indexer does not hold a reference into the tapped
            // receive buffer across the handler await.
            let body = match record.record_as_str() {
                Some(raw) => Some(
                    serde_json::from_str::<serde_json::Value>(raw)
                        .map_err(|e| IndexerError::Stream(e.to_string()))?,
                ),
                None => None,
            };

            return Ok(Some(RawEvent {
                seq: record.id,
                live: record.live,
                did: record.did.clone(),
                rev: record.rev.clone(),
                collection,
                rkey: record.rkey.clone(),
                action,
                cid: record.cid.clone(),
                body,
            }));
        }
    }
}
