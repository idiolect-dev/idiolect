# idiolect-observer

> **Source:** [`crates/idiolect-observer/`](https://github.com/idiolect-dev/idiolect/tree/main/crates/idiolect-observer)
>
> This crate is `publish = false` and is not on docs.rs. The
> authoritative reference is the source above plus the rustdoc
> built locally with `cargo doc -p idiolect-observer --open`.

Fold encounter-family records into observation records. Driven
by the declarative spec at `observer-spec/methods.json`; codegen
emits the method-descriptor table.

Because the crate is `publish = false`, depend via git or path:

```toml
[dependencies]
idiolect-observer = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.8.0", features = ["daemon"] }
```

## Public surface

```rust
pub trait ObservationMethod: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    /* observe(...) plus snapshot accessors; see source */
}

pub trait ObservationPublisher: Send + Sync {
    /* persist or transmit a finished observation record */
}

pub struct ObserverHandler<M, P> { /* RecordHandler<IdiolectFamily> impl wiring an ObservationMethod onto an ObservationPublisher */ }

pub async fn drive_observer<S, C, M, P>(
    stream: &mut S,
    cursor_store: &C,
    handler: &ObserverHandler<M, P>,
    schedule: FlushSchedule,
) -> ObserverResult<()>;
```

`ObservationMethod` is the stateful aggregator. It folds decoded
events into internal state and snapshots that state on flush.
`ObserverHandler` wires the method onto the indexer's
`RecordHandler` boundary; `drive_observer` runs the indexer loop
and triggers periodic flushes that publish observations through
the configured publisher.

## Shipped methods

The spec at `observer-spec/methods.json` declares eight bundled
methods; the typed structs ship in `crates/idiolect-observer/src/methods/`:

| Spec name | Module | Folds |
| --- | --- | --- |
| `correction-rate` | `correction_rate` | Per-lens correction counts grouped by reason. |
| `encounter-throughput` | `encounter_throughput` | Encounter traffic by kind and downstream result. |
| `verification-coverage` | `verification_coverage` | Per-lens verification counts by kind, result, and distinct verifiers. |
| `lens-adoption` | `lens_adoption` | Per-lens encounter count and distinct invokers. |
| `action-distribution` | `action_distribution` | Encounter counts grouped by `use.action` (with optional vocab roll-up). |
| `purpose-distribution` | `purpose_distribution` | Encounter counts grouped by `use.purpose`. |
| `basis-distribution` | `basis_distribution` | Record counts grouped by `basis` variant, bucketed by record kind. |
| `attribution-chains` | `attribution_chains` | Counts of `dev.idiolect.belief` records by holder and subject. |

Methods come in two forms (declared in the spec):

- `Record`-form methods consume `&IndexerEvent<IdiolectFamily>`
  directly and implement `ObservationMethod`.
- `Instance`-form methods consume a panproto `WInstance` plus
  the NSID and implement `InstanceMethod`. They wrap into
  `ObservationMethod` via `InstanceMethodAdapter`.

`default_methods()` returns boxed instances of every
record-form method.

## Publisher

`ObservationPublisher` is the persistence boundary. Shipped
implementations:

| Type | Backing |
| --- | --- |
| `InMemoryPublisher` | `Vec<Observation>`. For tests. |
| `PdsPublisher` | Writes via an `idiolect_lens::PdsWriter` to the observer's PDS. |

## Errors

`ObserverError` is a flattened error type; `ObserverResult<T>`
is its alias.

## Feature flags

| Feature | Adds |
| --- | --- |
| `daemon` | The `idiolect-observer` binary plus its CLI. Pulls in tracing-subscriber, anyhow, and the indexer's tapped firehose / sqlite cursor features. |
| `pds-atrium` | Forwards to `idiolect-lens/pds-atrium` for the PDS publisher. |

## Adding a method

Edit `observer-spec/methods.json`, add the method's entry, run
`cargo run -p idiolect-codegen`. The generated descriptor table
picks up the new method; you write the
`ObservationMethod` (or `InstanceMethod`) impl in
`crates/idiolect-observer/src/methods/<module>.rs` and add it to
the `default_methods()` constructor.
