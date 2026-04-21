//! Jetstream-backed [`EventStream`](crate::EventStream) adapter.
//!
//! [Jetstream](https://github.com/bluesky-social/jetstream) is
//! Bluesky's managed json-over-websocket fan-out for the atproto
//! firehose. It is an alternative to the `tap`/`tapped` managed-sync
//! transport used by [`TappedEventStream`](crate::TappedEventStream);
//! consumers who do not want the tap middleman can enable
//! `firehose-jetstream` and drop in [`JetstreamEventStream`].
//!
//! # Event shape
//!
//! Every jetstream frame is a single json line. The fields this
//! adapter depends on:
//!
//! ```json
//! {
//!   "did": "did:plc:...",
//!   "time_us": 1234567890123456,
//!   "kind": "commit" | "identity" | "account",
//!   "commit": {
//!     "rev":    "...",
//!     "operation": "create" | "update" | "delete",
//!     "collection": "...",
//!     "rkey":   "...",
//!     "record": { ... },       // absent on delete
//!     "cid":    "bafyrei..."   // absent on delete
//!   }
//! }
//! ```
//!
//! Non-commit frames (identity / account) are silently skipped: the
//! indexer's handler surface is record-shaped.
//!
//! # Cursor semantics
//!
//! Jetstream carries a `time_us` per frame — microseconds since unix
//! epoch — rather than tap's dense sequence. We project it onto the
//! `RawEvent.seq: u64` slot unchanged: the indexer treats seq as
//! opaque and monotonic, and `time_us` satisfies both invariants. The
//! cursor committed to a [`CursorStore`](crate::CursorStore) is the
//! most recent `time_us`; to resume from it, pass
//! `?cursor=<time_us>` in the jetstream subscribe URL.
//!
//! # Transport
//!
//! [`JetstreamEventStream::connect`] opens a websocket to the given
//! URL via `tokio-tungstenite` and parses frames as they arrive.
//! [`JetstreamEventStream::from_lines`] is a testable constructor
//! that consumes an iterator of json-line strings so unit tests do
//! not need a real websocket server.
//!
//! Feature-gated under `firehose-jetstream`.

use std::collections::VecDeque;

use futures_util::StreamExt;
use serde::Deserialize;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::error::IndexerError;
use crate::event::IndexerAction;
use crate::stream::{EventStream, RawEvent};

/// Jetstream frame, deserialized directly from a websocket message.
#[derive(Debug, Deserialize)]
struct JetstreamFrame {
    did: String,
    #[serde(default)]
    time_us: u64,
    kind: String,
    #[serde(default)]
    commit: Option<JetstreamCommit>,
}

#[derive(Debug, Deserialize)]
struct JetstreamCommit {
    rev: String,
    operation: String,
    collection: String,
    rkey: String,
    #[serde(default)]
    record: Option<serde_json::Value>,
    #[serde(default)]
    cid: Option<String>,
}

/// Parse a single jetstream json line into a [`RawEvent`], returning
/// `Ok(None)` for non-commit frames the indexer should silently skip.
///
/// # Errors
///
/// Returns [`IndexerError::Stream`] if the frame fails to parse or
/// carries an `operation` outside the create/update/delete taxonomy.
pub fn parse_frame(line: &str) -> Result<Option<RawEvent>, IndexerError> {
    let frame: JetstreamFrame = serde_json::from_str(line)
        .map_err(|e| IndexerError::Stream(format!("jetstream frame parse: {e}")))?;

    if frame.kind != "commit" {
        return Ok(None);
    }

    let Some(commit) = frame.commit else {
        return Ok(None);
    };

    let action = match commit.operation.as_str() {
        "create" => IndexerAction::Create,
        "update" => IndexerAction::Update,
        "delete" => IndexerAction::Delete,
        other => {
            return Err(IndexerError::Stream(format!(
                "unknown jetstream commit operation {other:?} on {}/{}/{}",
                frame.did, commit.collection, commit.rkey,
            )));
        }
    };

    Ok(Some(RawEvent {
        seq: frame.time_us,
        live: true,
        did: frame.did,
        rev: commit.rev,
        collection: commit.collection,
        rkey: commit.rkey,
        action,
        cid: commit.cid,
        body: commit.record,
    }))
}

/// Jetstream-backed event stream. Owns either a live websocket or a
/// pre-populated queue of frames (testing).
///
/// # Keepalive
///
/// On a live websocket, the stream sends a `Ping` frame every
/// [`keepalive_interval`](Self::keepalive_interval) seconds of idle
/// time so the upstream does not close the connection during quiet
/// periods. Jetstream deployments typically idle-timeout WS
/// connections around 60 s; the default 30 s interval leaves
/// comfortable headroom. Test mode (`Lines`) never pings.
pub struct JetstreamEventStream {
    source: JetstreamSource,
    /// Buffer of already-parsed events not yet returned by
    /// `next_event`. A live websocket normally produces one event
    /// per frame, but the parser can synthesize multiple events per
    /// frame in a future schema change and the queue absorbs that.
    buffered: VecDeque<RawEvent>,
    keepalive_interval: std::time::Duration,
}

type LiveWriter = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;

type LiveReader = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

enum JetstreamSource {
    /// Live websocket: reader for incoming frames, writer for
    /// outgoing pings.
    Socket {
        writer: LiveWriter,
        reader: LiveReader,
    },
    /// A finite queue of pre-supplied json lines (testing / fixtures).
    Lines(VecDeque<String>),
}

impl JetstreamEventStream {
    /// Connect to a jetstream endpoint and return a live stream.
    ///
    /// `url` follows the jetstream subscribe URL convention, e.g.
    /// `wss://jetstream2.us-east.bsky.network/subscribe?wantedCollections=dev.idiolect.*`.
    /// Optionally include `cursor=<time_us>` to resume from a prior
    /// cursor.
    ///
    /// # Errors
    ///
    /// Returns [`IndexerError::Stream`] for URL parse failures,
    /// DNS/TCP errors, or WS handshake failures.
    pub async fn connect(url: &str) -> Result<Self, IndexerError> {
        let parsed = url::Url::parse(url)
            .map_err(|e| IndexerError::Stream(format!("jetstream url {url}: {e}")))?;
        let (ws, _resp) = connect_async(parsed.as_str())
            .await
            .map_err(|e| IndexerError::Stream(format!("jetstream connect: {e}")))?;
        let (writer, reader) = ws.split();
        Ok(Self {
            source: JetstreamSource::Socket { writer, reader },
            buffered: VecDeque::new(),
            keepalive_interval: std::time::Duration::from_secs(30),
        })
    }

    /// Override the keepalive ping interval. Shorter intervals burn
    /// a ping every N seconds; longer intervals risk idle-timeout
    /// disconnects on quiet backends. Has no effect on test mode.
    #[must_use]
    pub fn with_keepalive_interval(mut self, interval: std::time::Duration) -> Self {
        self.keepalive_interval = interval;
        self
    }

    /// Current keepalive ping interval.
    #[must_use]
    pub const fn keepalive_interval(&self) -> std::time::Duration {
        self.keepalive_interval
    }

    /// Construct a stream that replays a sequence of pre-supplied json
    /// frames. Used by tests and offline fixtures.
    pub fn from_lines<I, S>(lines: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            source: JetstreamSource::Lines(lines.into_iter().map(Into::into).collect()),
            buffered: VecDeque::new(),
            keepalive_interval: std::time::Duration::from_secs(30),
        }
    }
}

impl EventStream for JetstreamEventStream {
    async fn next_event(&mut self) -> Result<Option<RawEvent>, IndexerError> {
        use futures_util::SinkExt;

        loop {
            if let Some(ev) = self.buffered.pop_front() {
                return Ok(Some(ev));
            }

            let line = match &mut self.source {
                JetstreamSource::Lines(queue) => match queue.pop_front() {
                    Some(s) => s,
                    None => return Ok(None),
                },
                JetstreamSource::Socket { writer, reader } => {
                    // Race the next incoming frame against a ping
                    // timer. When the timer wins, send a ping and
                    // loop back to keep waiting for a real event.
                    let sleep = tokio::time::sleep(self.keepalive_interval);
                    tokio::pin!(sleep);
                    tokio::select! {
                        maybe_msg = reader.next() => match maybe_msg {
                            Some(Ok(Message::Text(t))) => t.to_string(),
                            Some(Ok(Message::Binary(b))) => String::from_utf8(b.to_vec())
                                .map_err(|e| IndexerError::Stream(format!("jetstream binary: {e}")))?,
                            Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => continue,
                            Some(Ok(Message::Close(_))) => return Ok(None),
                            Some(Err(e)) => {
                                return Err(IndexerError::Stream(format!("jetstream recv: {e}")));
                            }
                            None => return Ok(None),
                        },
                        () = &mut sleep => {
                            // No frame within the keepalive window — send
                            // a ping and keep waiting. If the ping send
                            // itself fails the socket is effectively dead;
                            // surface that so the reconnecting wrapper can
                            // drop and reopen.
                            writer
                                .send(Message::Ping(Vec::new().into()))
                                .await
                                .map_err(|e| IndexerError::Stream(format!(
                                    "jetstream keepalive ping send failed: {e}"
                                )))?;
                            continue;
                        }
                    }
                }
            };

            if let Some(event) = parse_frame(&line)? {
                return Ok(Some(event));
            }
            // non-commit (identity / account): keep looping.
        }
    }
}
