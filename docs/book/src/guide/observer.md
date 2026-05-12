# Run the observer daemon

The observer reads encounter-family records from the firehose,
folds them through an `ObservationMethod`, and publishes a
`dev.idiolect.observation` record on each flush. Observations
are the durable summary; encounters are the event log.

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
- a signature.

The record kind is shared with deliberation tallies via the
`deliberationOutcome` lexicon, but that is a separate
record-kind, not an observation flavour.

## Why the observer publishes records, not just metrics

Observations are content-addressed and signed, like any other
ATProto record. A consumer reading an observation can:

- verify the signer DID,
- ask the indexer for the underlying encounters and re-fold them
  independently,
- treat the observation as a soft assertion of fact, not a
  single source of truth.

A central metrics endpoint cannot do that. The same record
shape also lets multiple observers run in parallel and disagree.

## Run the daemon

```bash
cargo install --path crates/idiolect-observer --features daemon
```

The shipped binary's exact CLI surface is documented by its
`--help`. It wires:

- A firehose stream (tapped, via the indexer's
  `firehose-tapped` feature, transitively pulled in by
  `daemon`).
- A SQLite cursor store.
- An `ObserverHandler<M, P>` connecting an `ObservationMethod`
  to an `ObservationPublisher`.
- A flush schedule that triggers observation publication.

## Bundled methods

The spec at `observer-spec/methods.json` declares eight bundled
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

Methods come in two forms (declared in the spec): record-form
methods consume `&IndexerEvent<IdiolectFamily>` directly;
instance-form methods consume a panproto `WInstance` and wrap
into the record form via `InstanceMethodAdapter`.

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
- Multiple observers publishing observations of the same scope
  is expected and useful; consumers can require quorum among $k$
  of $n$ trusted observers before treating an observation as
  authoritative.

## Note on `deliberation-tally`

The shipped `deliberation-tally` method emits its
per-statement per-stance vote counts inside an
`observation.output` blob, not as a typed
`dev.idiolect.deliberationOutcome` record. The data shape is
the same; the surface differs. A variant that publishes the
typed outcome record directly is a small refactor on top of
the existing `DeliberationTallyMethod`.
