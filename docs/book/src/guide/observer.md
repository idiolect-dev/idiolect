# Run the observer daemon

The observer folds records from a
[firehose](../glossary.md#firehose "A stream of repository commit events")
through an `ObservationMethod` and emits a
`dev.idiolect.observation` at each configured flush. Encounters remain
the event log; observations carry the computed summary.

The crate is
[`idiolect-observer`](../reference/crates/idiolect-observer.md).
Its daemon binary lives behind the `daemon` feature.

## What it produces

A `dev.idiolect.observation` record carries:

- the observer's DID,
- a structured `method` descriptor (name, version, optional
  parameters and code reference),
- a `scope` describing which records the aggregation covers,
- the method's `output` payload (shape is method-defined),
- the observation's publication timestamp and visibility.

The `deliberation-tally` method currently places its result in
`observation.output`; `dev.idiolect.deliberationOutcome` is a separate
record kind.

## Choose the publication boundary

Observations are content-addressed and signed, like any other
ATProto record. A consumer reading an observation can:

- verify the signer DID,
- ask the indexer for the underlying encounters and re-fold them
  independently,
- treat the observation as a soft assertion of fact, not a
  single source of truth.

This record boundary lets several observers publish different folds
without erasing the underlying events.

## Run the daemon

```bash
cargo install --path crates/idiolect-observer --features daemon
```

The binary is configured through environment variables, not command
flags. Set `IDIOLECT_OBSERVER_DID`; optionally set
`IDIOLECT_TAP_URL`, `IDIOLECT_TAP_ADMIN_PASSWORD`,
`IDIOLECT_OBSERVER_CURSORS`, `IDIOLECT_FLUSH_EVENTS`, and
`IDIOLECT_PDS_URL`. It wires:

- A firehose stream (tapped, via the indexer's
  `firehose-tapped` feature, transitively pulled in by
  `daemon`).
- A SQLite cursor store.
- A `CorrectionRateMethod` inside `ObserverHandler<M, P>`.
- A flush schedule that triggers observation publication.

If `IDIOLECT_PDS_URL` is unset, the reference daemon uses an in-memory
publisher and persists no observation records. Its PDS branch does not
yet supply authentication; use a wrapper binary with an authenticated
`PdsWriter` for production publication.

## Bundled methods

The spec at `observer-spec/methods.json` declares nine bundled
methods. Each lives in `crates/idiolect-observer/src/methods/`.

| Method | Folds |
| --- | --- |
| `correction-rate` | Per-lens correction counts grouped by reason. |
| `encounter-throughput` | Encounter traffic by kind and downstream result. |
| `verification-coverage` | Per-lens verification counts by kind, result, and distinct verifiers. |
| `lens-adoption` | Per-lens encounter count and distinct invokers. |
| `action-distribution` | Encounter counts grouped by `use.action`, optionally rolled up through a vocab. |
| `purpose-distribution` | Encounter counts grouped by `use.purpose`. |
| `basis-distribution` | Record counts grouped by `basis` variant, bucketed by record kind. |
| `attribution-chains` | `dev.idiolect.belief` counts by holder and subject. |
| `deliberation-tally` | Per-statement per-stance `deliberationVote` counts (see the note below). |

The current spec declares all nine methods in record form; they consume
`&IndexerEvent<IdiolectFamily>`. The library also supports instance-form
methods over panproto `WInstance` through `InstanceMethodAdapter`.

`default_methods()` returns boxed instances of every
record-form method; instance-form methods need a caller-supplied
schema resolver and are constructed individually.

## Add a method

Edit `observer-spec/methods.json`, add the method's entry, run
`cargo run -p idiolect-codegen`. The generated descriptor table
picks up the new method. Implement `ObservationMethod` (or
`InstanceMethod`) in
`crates/idiolect-observer/src/methods/<module>.rs` and add it
to the `default_methods()` constructor.

## Operational notes

- Observers should run with their own DID, distinct from the
  DIDs whose encounters they observe.
- Multiple observers may publish observations of the same scope.
  Consumers can require quorum among $k$ of $n$ trusted observers
  before treating an observation as authoritative.

## `deliberation-tally` output

The shipped `deliberation-tally` method emits its
per-statement per-stance vote counts inside an
`observation.output` blob, not as a typed
`dev.idiolect.deliberationOutcome` record. The data shape is
the same; the publication surface differs. Publishing a typed outcome
requires separate application code.
