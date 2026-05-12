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
crate is library-only; runners are invoked programmatically.

## Wire up a runner

```toml
# in Cargo.toml
idiolect-verify = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.9.0" }
idiolect-lens   = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.9.0", features = ["pds-reqwest"] }
idiolect-records = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.9.0" }
panproto-schema  = { git = "https://github.com/panproto/panproto.git", tag = "v0.47.0" }
tokio            = { version = "1", features = ["full"] }
```

`src/main.rs`:

```rust
use idiolect_lens::{PdsResolver, PdsSchemaLoader, ReqwestPdsClient};
use idiolect_records::Datetime;
use idiolect_records::generated::dev::idiolect::defs::LensRef;
use idiolect_verify::{
    RoundtripTestRunner, VerificationRunner, VerificationTarget,
};
use panproto_schema::Protocol;

const PDS:      &str = "https://jellybaby.us-east.host.bsky.network";
const LENS_URI: &str = "at://did:plc:wdl4nnvxxdy4mc5vddxlm6f3/dev.panproto.schema.lens/tutorial-rename-sort-string-to-text";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client   = ReqwestPdsClient::with_service_url(PDS);
    let resolver = PdsResolver::new(client.clone());
    let loader   = PdsSchemaLoader::new(client);

    // Small corpus matching the lens's source schema (single
    // `text` string child on a `post:body` object).
    let corpus = vec![
        serde_json::json!({ "text": "hello, world" }),
        serde_json::json!({ "text": "" }),
        serde_json::json!({ "text": "líneas con tildes y emoji 🦀" }),
    ];

    let runner = RoundtripTestRunner::new(resolver, loader, Protocol::default(), corpus);

    let target = VerificationTarget {
        lens: LensRef {
            uri: Some(LENS_URI.parse()?),
            cid: None,
            direction: None,
        },
        verifier: "did:plc:wdl4nnvxxdy4mc5vddxlm6f3".parse()?,
        occurred_at: Datetime::parse("2026-05-12T00:00:00.000Z")?,
        tool_override: None,
    };

    let verification = runner.run(&target).await?;
    println!("result = {:?}", verification.result);
    println!("kind   = {:?}", verification.kind);
    println!("tool   = {} {}",
             verification.tool.name, verification.tool.version);
    Ok(())
}
```

```bash
cargo run
```

```text
result = Holds
kind   = RoundtripTest
tool   = idiolect-verify/roundtrip-test 0.9.0
```

The runner walked the corpus, applied the rename-sort lens
forward then backward, and confirmed every record round-tripped
byte-for-byte. A single counterexample would have produced
`result = Falsified`.

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

The `verification` value the runner returned is a typed
`Verification` record ready to publish. The publishing path
goes through `idiolect_lens::RecordPublisher`; see [chapter 5
on publishing](./05-publish.md) for the wire-up.

## Reading verifications back

The orchestrator's HTTP API exposes
`GET /v1/verifications?lens_uri=...` for "every verification on
this lens":

```bash
idiolect orchestrator verifications \
  --lens_uri at://did:plc:wdl4nnvxxdy4mc5vddxlm6f3/dev.panproto.schema.lens/tutorial-rename-sort-string-to-text
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
yet; runners are library-only. The four shipped runner kinds
(`roundtrip-test`, `property-test`, `static-check`,
`coercion-law`) cover the shape; additional kinds in the
lexicon's `verification.kind` enum (`formal-proof`,
`conformance-test`, `convergence-preserving`) are recognised
slugs awaiting community-contributed runners.

The next chapter publishes a `dev.idiolect.recommendation` that
endorses a lens path under specific applicability conditions.
