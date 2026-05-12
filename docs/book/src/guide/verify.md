# Author a verification runner

A verification runner is a function that takes a lens (and a
small set of typed inputs) and returns a `Verification` record
with `result` set to `Holds`, `Falsified`, or `Inconclusive`.
The result record is publishable as a
`dev.idiolect.verification`; downstream consumers reading the
record can decide whether to trust it.

The runner trait:

```rust
pub trait VerificationRunner: Send + Sync {
    fn kind(&self) -> VerificationKind;
    fn tool(&self) -> Tool;
    async fn run(&self, target: &VerificationTarget) -> VerifyResult<Verification>;
}
```

A *falsified* property returns
`Ok(Verification { result: Falsified, ... })`, not an error.
Falsification is the signal the community is paying the runner
to produce; `VerifyError` is reserved for input-shape, transport,
or irrecoverable-state failures.

## Shipped runners

Four kinds ship in `crates/idiolect-verify/src/`:

| Kind | Runner |
| --- | --- |
| `roundtrip-test` | `RoundtripTestRunner` |
| `property-test` | `PropertyTestRunner` |
| `static-check` | `StaticCheckRunner` |
| `coercion-law` | `CoercionLawRunner` |

The lexicon's `verification.kind` field is open-enum and lists
additional kinds (`formal-proof`, `conformance-test`,
`convergence-preserving`); those kinds are recognised but not
shipped as runners. Communities that need them author their own.

## Add a runner kind

The project's spec-driven layout means adding a kind is two
edits:

1. Add an entry to `verify-spec/runners.json` declaring the kind
   and its description. Run `cargo run -p idiolect-codegen`;
   the generated kind taxonomy
   (`crates/idiolect-verify/src/generated.rs`) picks up the
   new kind.
2. Implement `VerificationRunner` for a struct in a new module
   under `crates/idiolect-verify/src/`. Re-export it from
   `lib.rs`.

The runner's `kind()` returns the new `VerificationKind`
variant; the `run` method does the work and returns a
`Verification` record. Use `build_verification` (in
`runner.rs`) to package the result with the structured
`property` field.

## Test

Every shipped runner has integration tests that exercise the
holds / falsified / inconclusive cases against fixtures. New
runners follow the same pattern: a fixture with a known result,
a call to `runner.run(...)`, an assertion on the returned
`Verification`.

## Publish a verification record

Once you have a `Verification`, publish it via
`idiolect_lens::RecordPublisher`:

```rust
use idiolect_lens::RecordPublisher;

let publisher = RecordPublisher::new(writer, my_did);
let resp = publisher.create(&verification).await?;
println!("published: {}", resp.uri);
```

The publisher serializes the record, splices the `$type` field,
and forwards to the configured `PdsWriter`. The signing path
goes through `SigningPdsWriter` plus a `DpopProver` from the
`pds-reqwest` and (optionally) `dpop-p256` features.

## CLI surface

The `idiolect verify <kind>` subcommand wraps each shipped
runner against a live PDS via `PdsResolver` + `PdsSchemaLoader`:

```text
idiolect verify roundtrip-test  --lens AT_URI [--corpus PATH]
idiolect verify property-test   --lens AT_URI  --corpus PATH  [--budget N]
idiolect verify static-check    --lens AT_URI
idiolect verify coercion-law    --lens AT_URI  --vcs-url URL  --standard STD
```

Corpus files may be JSON arrays or JSON Lines. The
`property-test` generator cycles through the corpus by index;
`--budget` controls case count. The CLI prints the typed
`Verification` record as JSON and exits non-zero on `Falsified`
or `Inconclusive`. Publishing the result is a separate step;
pipe to `idiolect publish verification --record -`.
