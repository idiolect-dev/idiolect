# idiolect-indexer

> **API reference:** [docs.rs/idiolect-indexer](https://docs.rs/idiolect-indexer/latest/idiolect_indexer/)
> · **Source:** [`crates/idiolect-indexer/`](https://github.com/idiolect-dev/idiolect/tree/main/crates/idiolect-indexer)
> · **Crate:** [crates.io/idiolect-indexer](https://crates.io/crates/idiolect-indexer)
>
> This page is an editorial overview. The per-symbol surface
> (every public type, trait, function, and feature flag) is the
> docs.rs link above. That is the authoritative reference.

The crate factors a [firehose](../../glossary.md#firehose) consumer
into three trait surfaces. It
owns the loop. You bring the stream, the handler, and the cursor
store.

```toml
[dependencies]
idiolect-indexer = { version = "0.12.1", features = ["firehose-jetstream", "cursor-filesystem", "reconnecting"] }
```

## Public surface

### Trait surface

```text
pub trait EventStream: Send + Sync {
    async fn next_event(&mut self) -> Result<Option<RawEvent>, IndexerError>;
}

pub trait CursorStore: Send + Sync {
    async fn load(&self, subscription_id: &str) -> Result<Option<u64>, IndexerError>;
    async fn commit(&self, subscription_id: &str, seq: u64) -> Result<(), IndexerError>;
    async fn list(&self) -> Result<Vec<(String, u64)>, IndexerError> { /* default */ }
}

pub trait RecordHandler<F: RecordFamily = IdiolectFamily>: Send + Sync {
    async fn handle(&self, event: &IndexerEvent<F>) -> Result<(), IndexerError>;
}
```

`IndexerEvent<F>` carries the decoded event: `seq`, `live`, the
DID, repo revision, rkey, NSID, action (create / update /
delete), CID, and the typed record body (`Option<F::AnyRecord>`).

### Composer

```text
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
    C: CursorStore;
```

`drive_idiolect_indexer` is the convenience alias when
`F = IdiolectFamily`.

### Shipped impls

| Type | Feature | Purpose |
| --- | --- | --- |
| `JetstreamEventStream` | `firehose-jetstream` | Subscribes to a Jetstream websocket feed. |
| `TappedEventStream` | `firehose-tapped` | Subscribes to the at-proto-native firehose via `tapped`. |
| `ReconnectingEventStream<C, F, S>` | `reconnecting` | Recreates an `S: EventStream` with a caller-supplied async connection factory and exponential backoff. |
| `InMemoryCursorStore` | (always) | `HashMap`-backed; for tests. |
| `FilesystemCursorStore` | `cursor-filesystem` | One JSON file containing the cursor map for every subscription ID. |
| `SqliteCursorStore` | `cursor-sqlite` | One row per stream. Pairs with handlers that also write SQLite. |
| `NoopRecordHandler` | (always) | Counts events and drops them. Useful as a baseline. |
| `RetryingHandler` / `CircuitBreakerHandler` | `resilience` | Wraps an inner handler with retry / circuit-breaker policies. |

## Error surface

`IndexerError` flattens the failure modes from all three
boundaries. Variants:

| Variant | Trigger |
| --- | --- |
| `Stream(String)` | Transport error from the event stream. |
| `Cursor(String)` | Cursor store read or write failed. |
| `Decode(DecodeError)` | A known NSID failed to decode into its typed record. |
| `Handler(String)` | Handler returned a handler-defined error. |
| `MissingBody(String)` | The firehose event had no record body or the body was malformed. |
| `FamilyContract(String)` | `contains` accepted an NSID but `decode` returned `None`; this is a family-implementation bug. |

## Feature flags

| Feature | Adds |
| --- | --- |
| `firehose-jetstream` | Jetstream websocket client. |
| `firehose-tapped` | Tapped at-proto-native firehose client. |
| `cursor-filesystem` | Filesystem cursor store. |
| `cursor-sqlite` | SQLite cursor store. |
| `reconnecting` | Reconnect wrapper. |
| `resilience` | Retry and circuit-breaker handler wrappers. |

## Cursor commit semantics

`drive_indexer` commits the cursor only after the handler
returns `Ok`. A failing handler does not commit. The loop
returns the error to its caller. Handler retries require the
`RetryingHandler` wrapper or an application-level policy. The
shipped driver is thus at-least-once: an event may be handled
again if the cursor commit fails. A custom driver can coordinate
its data write and cursor update in one storage transaction when
both use the same backend.
