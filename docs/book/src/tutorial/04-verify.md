# Run a verification

A `dev.idiolect.verification` record is a signed claim that a lens
satisfies a property. The verifier crate
([`idiolect-verify`](../reference/crates/idiolect-verify.md))
ships four runner kinds:

- **`roundtrip-test`** runs `put(get(a)) == a` over a corpus of
  source records.
- **`property-test`** runs an arbitrary boolean predicate over a
  corpus, suitable for laws beyond round-trip (idempotence,
  commutativity, naturality, ...).
- **`static-check`** runs panproto's existence and structural
  checks against the lens chain.
- **`coercion-law`** runs panproto's sample-based coercion-law
  checker against any `CoerceType` step.

Each ships as a struct implementing `VerificationRunner`. The
crate is library-only at v0.8.0; runners are invoked
programmatically.

## Wire up a runner

`idiolect-verify` and `idiolect-lens` are `publish = false`;
depend on them via git:

```toml
# in Cargo.toml
idiolect-verify = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.8.0" }
idiolect-lens   = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.8.0", features = ["pds-reqwest"] }
tokio = { version = "1", features = ["full"] }
```

```rust
use idiolect_lens::{
    InMemoryResolver, FilesystemSchemaLoader, PdsResolver, ReqwestPdsClient,
};
use idiolect_verify::{
    RoundtripTestRunner, VerificationRunner, VerificationTarget,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = ReqwestPdsClient::with_service_url("https://bsky.social");
    let resolver = PdsResolver::new(client);
    let loader = FilesystemSchemaLoader::new("./schema-cache")?;
    // Construct the runner. The exact constructor and the
    // `VerificationTarget` shape are documented in the
    // `idiolect-verify` source; load the corpus, lens, and
    // schema-loader handles into the target.
    let runner = RoundtripTestRunner::new(/* corpus and config */);
    let target = VerificationTarget {
        // lens, schema_loader, sampling config, ...
    };

    let verification = runner.run(&target).await?;
    println!("{:?}", verification.result); // Holds / Falsified / Inconclusive
    Ok(())
}
```

A runner returns a `Verification` record. The `result` field is
the headline; falsifying runs additionally carry a
`counterexample` content reference and (for property tests) a
shrunk witness.

## Falsified is a record, not an error

A *falsified* property returns
`Ok(Verification { result: Falsified, ... })`, not an error. The
substrate's view: a falsified verification is the signal the
community is paying the runner to produce. Treat it like a
finding, sign and publish it. Consumers reading the falsified
record decide whether to continue invoking the lens.

`VerifyError` is reserved for input-shape, transport, or
irrecoverable-state failures (a corpus the runner could not load,
a schema the loader could not resolve). Those are operator
problems; the lens's actual behaviour is captured in the
returned record.

## Publish the result

Once you have a `Verification`, publish it through
`idiolect_lens::RecordPublisher`:

```rust
use idiolect_lens::RecordPublisher;

let publisher = RecordPublisher::new(writer, my_did);
let resp = publisher.create(&verification).await?;
println!("published: {}", resp.uri);
```

`writer` is any `PdsWriter` impl. The shipped path is
`SigningPdsWriter` (DPoP-bound, behind the `pds-reqwest` feature)
plus a `DpopProver` (typically `P256DpopProver` from the
`dpop-p256` feature, paired with an OAuth session loaded from an
`idiolect_oauth::OAuthTokenStore`). Authentication is the
caller's responsibility: see
[Configure OAuth sessions](../guide/oauth.md) for the session
side.

## Reading verifications back

The orchestrator's HTTP API exposes
`GET /v1/verifications?lens_uri=...` for "every verification on
this lens". From the CLI:

```bash
idiolect orchestrator verifications --lens_uri at://did:plc:.../lens/example
```

The orchestrator reads its catalog (populated by the indexer)
and returns the matching verification records. This is the
shape downstream consumers use to decide whether to invoke a
lens: query the verifier registry, accept the verifications they
trust, reject the rest, and proceed only if the surviving set
covers the properties their use case requires.

## Planned functionality

A future `idiolect verify <kind>` CLI subcommand would let
operators run a runner without writing Rust. It is not shipped
at v0.8.0; runners are library-only. The four shipped runner
kinds (`roundtrip-test`, `property-test`, `static-check`,
`coercion-law`) cover the shape; additional kinds in the
lexicon's `verification.kind` enum (`formal-proof`,
`conformance-test`, `convergence-preserving`) are recognised
slugs awaiting community-contributed runners.

The next chapter publishes a `dev.idiolect.recommendation` that
endorses a lens path under specific applicability conditions.
