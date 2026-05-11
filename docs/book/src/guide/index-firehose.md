# Index a firehose

[`idiolect-indexer`](../reference/crates/idiolect-indexer.md) is
a firehose consumer factored into three trait surfaces:

- `EventStream`: yields `RawEvent`s from a PDS firehose. Shipped
  impls: `JetstreamEventStream` (Jetstream websocket feed) and
  `TappedFirehoseStream` (the at-proto-native firehose via
  `tapped`).
- `RecordHandler<F: RecordFamily = IdiolectFamily>`: handles one
  decoded `IndexerEvent<F>`. The family parameter narrows the
  handler to the records the indexer should not skip; everything
  outside the family is dropped before decode.
- `CursorStore`: persists the last-acknowledged sequence number
  per subscription so a restart resumes where the previous run
  left off.

`drive_indexer` composes the three. `drive_idiolect_indexer` is
the convenience alias when the family is `IdiolectFamily`.

## Minimum viable indexer

```rust
use idiolect_indexer::{
    drive_idiolect_indexer, FilesystemCursorStore, IndexerConfig,
    IndexerEvent, JetstreamEventStream, RecordHandler,
};
use idiolect_records::IdiolectFamily;

struct PrintHandler;

#[async_trait::async_trait]
impl RecordHandler<IdiolectFamily> for PrintHandler {
    async fn handle(
        &self,
        event: &IndexerEvent<IdiolectFamily>,
    ) -> Result<(), idiolect_indexer::IndexerError> {
        println!("{} {} {:?}", event.did, event.collection, event.action);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut stream = JetstreamEventStream::connect("wss://...").await?;
    let cursors = FilesystemCursorStore::open("./cursor.json")?;
    let handler = PrintHandler;
    let config = IndexerConfig::default();

    drive_idiolect_indexer(&mut stream, &handler, &cursors, &config).await?;
    Ok(())
}
```

Add features:

```bash
cargo add idiolect-indexer \
  --features firehose-jetstream,cursor-filesystem,reconnecting
```

`reconnecting` wraps the inner stream in an exponential-backoff
loop. `cursor-sqlite` swaps the filesystem cursor store for a
SQLite-backed one. `resilience` adds retry and circuit-breaker
handler wrappers.

## Family-typed dispatch

The `IdiolectFamily` parameter is a typed predicate over NSIDs.
A commit whose collection is not in the family drops before
decode, so an upstream PDS adding a record type ahead of your
codegen run does not halt the loop. To handle two families
(idiolect plus a downstream community's lexicons), compose:

```rust
use idiolect_records::{IdiolectFamily, OrFamily};

struct MyFamily;
// `MyFamily` impls `RecordFamily`.

let handler: MyHandler<OrFamily<IdiolectFamily, MyFamily>> = ...;
```

`OrFamily<F1, F2>` recognises every NSID either side claims. Its
`AnyRecord` is `OrAny`, a tagged union over the two halves.
`detect_or_family_overlap` audits a probe set at boot so a
configuration mistake does not silently shadow the right-side
family.

## Cursor semantics

`drive_indexer` calls
`CursorStore::commit(subscription_id, seq)` after the handler
returns `Ok`. A handler that wants at-least-once semantics
should make its work idempotent before returning; a handler
that wants exactly-once semantics needs to coordinate the commit
with its own storage transaction.

Errors propagate as `IndexerError`. The variants distinguish
transport failures (`Stream`), decode failures (`Decode`),
family-contract bugs (`FamilyContract`, fired only when
`contains` returns true but `decode` returns `None`),
handler-defined errors (`Handler`), missing-body events
(`MissingBody`), and cursor-store failures (`Cursor`).

## Observability

Every shipped surface logs through `tracing`. Wire a
subscriber:

```rust
tracing_subscriber::fmt()
    .with_env_filter("idiolect_indexer=info")
    .init();
```

You will see one log line per accepted commit, one per skipped
commit (debug level), and one per cursor commit. The
orchestrator exposes a Prometheus surface; see
[Run the orchestrator HTTP API](./orchestrator.md).
