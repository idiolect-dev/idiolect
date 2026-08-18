# Verify the round trip

A [verification](../glossary.md#verification "Claim about a lens property") is a
typed claim about a lens property. Here we test whether `put(get(source))`
returns `source` for three records: ordinary text, an empty string, and Unicode
text.

## Run the verifier

From the repository root:

```bash
cargo run --quiet \
  --manifest-path scripts/publish-tutorial-lens/Cargo.toml \
  --bin verify-tutorial-lens
```

```text
result = Holds
kind   = RoundtripTest
tool   = idiolect-verify/roundtrip-test 0.11.1
```

The executable gives the corpus to `RoundtripTestRunner` and runs it against
the published lens:

```text
let runner = RoundtripTestRunner::new(
    resolver,
    loader,
    Protocol::default(),
    corpus,
);
let verification = runner.run(&target).await?;
```

For each source record, the runner calls `apply_lens` and then
`apply_lens_put`. `Holds` means that all three records round-tripped exactly.
It supports a claim about this corpus; it is not a proof over every possible
record.

## Distinguish a finding from a failure

A counterexample produces a typed verification with `result = Falsified`.
That finding is a successful verifier run and can be published. Transport,
schema-loading, and malformed-input problems instead return `VerifyError`,
because the runner could not evaluate the property.

The `verification` value is already shaped like a
`dev.idiolect.verification` record. Chapter 5 publishes a recommendation that
identifies the same lens and the source schema under which it applies.
