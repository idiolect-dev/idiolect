# idiolect-verify

> **Source:** [`crates/idiolect-verify/`](https://github.com/idiolect-dev/idiolect/tree/main/crates/idiolect-verify)
>
> This crate is `publish = false` and is not on docs.rs. The
> authoritative reference is the source above plus the rustdoc
> built locally with `cargo doc -p idiolect-verify --open`.

Verification runners with declarative dispatch. Driven by
`verify-spec/runners.json`; codegen emits the kind taxonomy.

Because the crate is `publish = false`, depend via git or path:

```toml
[dependencies]
idiolect-verify = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.8.0" }
```

## Public surface

```rust
pub trait VerificationRunner: Send + Sync {
    fn kind(&self) -> VerificationKind;
    fn tool(&self) -> Tool;
    async fn run(&self, target: &VerificationTarget) -> VerifyResult<Verification>;
}
```

A runner returns a `Verification` record with `result` set to
`Holds`, `Falsified`, or `Inconclusive`. Falsification is *not*
an error: a falsified verification is a first-class record.
`VerifyError` is reserved for input-shape, transport, or
irrecoverable-state failures.

The `build_verification` helper packages a runner result into a
`Verification` record shaped for direct publication via
`idiolect_lens::RecordPublisher::create`.

## Shipped runners

The spec at `verify-spec/runners.json` declares four bundled
kinds:

| Kind | Runner |
| --- | --- |
| `roundtrip-test` | `RoundtripTestRunner` — runs `put(get(a)) == a` over a corpus. |
| `property-test` | `PropertyTestRunner` — runs an arbitrary boolean predicate over a corpus. |
| `static-check` | `StaticCheckRunner` — runs panproto's existence and structural checks against the lens chain. |
| `coercion-law` | `CoercionLawRunner` — runs panproto's sample-based coercion-law checker, optionally via a `CoercionLawClient`. |

The lexicon's `verification.kind` field is open-enum and lists
additional kinds (`formal-proof`, `conformance-test`,
`convergence-preserving`); those kinds are *recognised* but not
shipped as runners. Communities that need them author their own
runner against the trait.

## Errors

`VerifyError` covers input-shape, transport, and
irrecoverable-state failures; `VerifyResult<T>` is its alias.

## Result records

A passing run produces a `Verification` record with
`result: "holds"`. A falsifying run produces one with
`result: "falsified"` plus a counterexample (when the runner
captured one). The publisher path uses
`idiolect_lens::RecordPublisher`.

## Adding a runner

1. Add the kind's entry to `verify-spec/runners.json`.
2. Run `cargo run -p idiolect-codegen` to refresh the generated
   kind taxonomy.
3. Implement the `VerificationRunner` trait against the new
   kind in a new module under `crates/idiolect-verify/src/`.
4. Re-export it from `lib.rs` and add to the runner registry
   wiring.
