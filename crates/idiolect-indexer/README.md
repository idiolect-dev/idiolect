# idiolect-indexer

Firehose consumer for `dev.idiolect.*` records.

## Overview

Sits between a firehose transport (`tapped`, jetstream, a custom
adapter) and an appview's per-record handlers. Three traits carry
every boundary:

- **`EventStream`** — yields commits, one at a time. Impls for
  in-memory fixtures and for tapped + jetstream connections.
- **`CursorStore`** — persists the ack cursor so the indexer
  resumes after a restart. Impls for in-memory, filesystem JSON,
  and sqlite.
- **`RecordHandler`** — user code. Receives each decoded commit as
  an `IndexerEvent` with the body already materialized into
  `AnyRecord`.

`drive_indexer` wires the three together and owns the event loop:
decode, dispatch, commit the cursor, handle backpressure errors,
exit cleanly on stream close. `ReconnectingEventStream` layers
exponential-backoff reconnect + cursor replay for production
deployments where transport flaps are routine. `RetryingHandler`
and `CircuitBreakerHandler` wrap any `RecordHandler` with the
matching resilience policy.

## Usage

```rust
use idiolect_indexer::{
    InMemoryCursorStore, InMemoryEventStream, IndexerConfig, NoopRecordHandler,
    drive_indexer,
};

let mut stream = InMemoryEventStream::new();
let handler = NoopRecordHandler::new();
let cursors = InMemoryCursorStore::new();

drive_indexer(&mut stream, &handler, &cursors, &IndexerConfig::default()).await?;
```

## Feature flags

| Flag | Default | Effect |
| ---- | ------- | ------ |
| `firehose-tapped` | off | `TappedEventStream` backed by [`tapped`](https://crates.io/crates/tapped). Live firehose + repo backfill. |
| `firehose-jetstream` | off | `JetstreamEventStream` for jetstream's json-over-websocket. Includes keepalive pings. |
| `cursor-filesystem` | off | `FilesystemCursorStore` — one JSON file per subscription id. |
| `cursor-sqlite` | off | `SqliteCursorStore` — WAL-journaled sqlite table. |
| `reconnecting` | off | `ReconnectingEventStream` + `BackoffPolicy`. |
| `resilience` | off | `RetryingHandler` + `CircuitBreakerHandler`. |

## Design notes

- Every event carries a `live: bool`. Live and backfill events
  dispatch identically at the handler, but the cursor store only
  advances on live events — replaying backfill on reconnect is
  safe and expected.
- Trait objects are not dyn-compatible because the traits use
  native `async fn`; the crate ships Arc blanket impls so consumers
  share state via `Arc<ConcreteImpl>` instead.

## Related

- [`idiolect-records`](../idiolect-records) — `AnyRecord` and
  `decode_record` materialize bodies inside the indexer.
- [`idiolect-orchestrator`](../idiolect-orchestrator) and
  [`idiolect-observer`](../idiolect-observer) — both consume this
  crate's firehose stream.
