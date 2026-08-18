# Author a verification runner

A [verification](../glossary.md#verification "A signed claim that a lens satisfies a stated property")
runner takes a lens and typed inputs, then returns a `Verification`
record with `result` set to `Holds`, `Falsified`, or `Inconclusive`.
The result record is publishable as a
`dev.idiolect.verification`; downstream consumers reading the
record can decide whether to trust it.

The runner trait:

```text
pub trait VerificationRunner: Send + Sync {
    fn kind(&self) -> VerificationKind;
    fn tool(&self) -> Tool;
    async fn run(&self, target: &VerificationTarget) -> VerifyResult<Verification>;
}
```

A *falsified* property returns
`Ok(Verification { result: Falsified, ... })`, not an error.
A falsified result is a finding the runner is meant to report,
not an error. `VerifyError` is reserved for input-shape,
transport, or irrecoverable-state failures.

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
`convergence-preserving`). Those kinds are recognized but not
shipped as runners. Communities that need them author their own.

## Add a runner kind

Adding a kind requires two source edits and regeneration:

1. Add an entry to `verify-spec/runners.json` declaring the kind
   and its description. Run `cargo run -p idiolect-codegen`.
   The generated kind taxonomy
   (`crates/idiolect-verify/src/generated.rs`) picks up the
   new kind.
2. Implement `VerificationRunner` for a struct in a new module
   under `crates/idiolect-verify/src/`. Re-export it from
   `lib.rs`.

The runner's `kind()` returns the new `VerificationKind` variant. Its
`run` method performs the check and returns a `Verification`. Use
`idiolect_verify::runner::build_verification` to package the result
with its structured `property` field.

## Test

Every shipped runner has integration tests that exercise the
holds / falsified / inconclusive cases against fixtures. New
runners follow the same pattern: a fixture with a known result,
a call to `runner.run(...)`, an assertion on the returned
`Verification`.

## Publish a verification record

Once you have a `Verification`, publish it via
`idiolect_lens::RecordPublisher`:

```text
use idiolect_lens::RecordPublisher;

let publisher = RecordPublisher::new(writer, my_did);
let resp = publisher.create(&verification).await?;
println!("published: {}", resp.uri);
```

The publisher serializes the record, inserts `$type`, and forwards it
to the configured `PdsWriter`. For OAuth-bound writes, combine
`SigningPdsWriter` with a `DpopProver` from the `pds-reqwest` and
`dpop-p256` features.

## CLI surface

The `idiolect verify <kind>` subcommand wraps each shipped
runner against a live PDS via `PdsResolver` + `PdsSchemaLoader`:

```text
idiolect verify roundtrip-test  --lens AT_URI [--corpus PATH] [--pds-url URL] [--verifier-did DID]
idiolect verify property-test   --lens AT_URI --corpus PATH [--budget N] [--pds-url URL] [--verifier-did DID]
idiolect verify static-check    --lens AT_URI [--pds-url URL] [--verifier-did DID]
idiolect verify coercion-law    --lens AT_URI --vcs-url URL --standard STD [--version V] [--violation-threshold N] [--verifier-did DID]
```

Corpus files may be JSON arrays or JSON Lines. The
`property-test` generator cycles through the corpus by index,
and `--budget` controls case count. The CLI prints the typed
`Verification` record as JSON and exits non-zero on `Falsified`
or `Inconclusive`. Publishing the result is a separate step:
pipe to `idiolect publish verification --record -`.
