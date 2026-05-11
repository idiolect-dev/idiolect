# Publish and resolve a lens

A lens on the network is two artifacts:

1. A `dev.panproto.schema.lens` record on a PDS, carrying the
   protolens (or protolens chain) blob and pointers to the source
   and target schemas.
2. The two schemas themselves, each a `dev.panproto.schema.schema`
   record (or a `getSchema` xrpc response from a panproto vcs).

The lens record is what consumers resolve by at-uri. The schemas
are what the runtime instantiates against.

## Build the lens

The shortest path is to derive it from a schema diff:

```bash
schema lens generate old.json new.json --protocol atproto > chain.json
```

`schema` is the panproto CLI. `chain.json` is a protolens chain in
panproto's serialized format. See the
[panproto protolens skill](https://panproto.dev/) for what the chain
looks like and how to inspect it.

## Stage it

```rust
use idiolect_records::{PanprotoLens, AtUri, Datetime};

let chain: serde_json::Value = serde_json::from_slice(&std::fs::read("chain.json")?)?;

let lens = PanprotoLens {
    blob: Some(chain),
    created_at: Datetime::parse("2026-04-19T00:00:00.000Z").unwrap(),
    laws_verified: Some(true),
    object_hash: format!("sha256:{}", sha256_hex(&blob_bytes)),
    round_trip_class: Some("isomorphism".into()),
    source_schema: AtUri::parse(
        "at://did:plc:tutorial.dev/dev.panproto.schema.schema/v1",
    )?,
    target_schema: AtUri::parse(
        "at://did:plc:tutorial.dev/dev.panproto.schema.schema/v2",
    )?,
};
```

Three fields warrant care:

- `object_hash` is a content-addressed identifier for the chain
  bytes. The `VerifyingResolver` will refuse to hand the lens to
  the runtime unless the hash matches the canonical bytes.
- `round_trip_class` is the optic class panproto's classifier
  produces (`isomorphism`, `injection`, `projection`, `affine`,
  `general`). Consumers use this to route review.
- `laws_verified` is a soft assertion that the chain passed
  panproto's coercion-law and existence checks. A true value here
  is meaningless without a corresponding `dev.idiolect.verification`
  record from a publisher you trust; treat it as a pre-publish
  smoke signal.

## Publish

Construct a `SigningPdsWriter` from a reqwest PDS client plus a
DPoP prover, wrap it in a `RecordPublisher`, and call `create`:

```rust
use idiolect_lens::{
    P256DpopProver, RecordPublisher, ReqwestPdsClient, SigningPdsWriter,
};

let client = ReqwestPdsClient::with_service_url(&session.pds_url);
let prover = P256DpopProver::from_pkcs8_pem(&pkcs8_pem)?;
let writer = SigningPdsWriter::new(
    client,
    session.access_jwt.clone(),
    prover,
    session.dpop_nonce.clone(),
);
let publisher = RecordPublisher::new(writer, session.did.clone());

let resp = publisher.create(&lens).await?;
```

`pkcs8_pem` is converted from the session's
`dpop_private_key_jwk` via an external JWK-to-PKCS8 helper.
Driving the OAuth dance and persisting the session is the
caller's job; see [Configure OAuth sessions](./oauth.md). The
PDS rejects the record if the chain blob does not parse, the
schema at-uris do not resolve, or the canonical bytes do not
match the declared `object_hash`.

## Resolve it

The complement of publishing is resolving. Given an at-uri, the
[`Resolver`](../reference/crates/idiolect-lens.md#resolver) trait
hands back a `PanprotoLens` record:

```rust
use idiolect_lens::{
    PdsResolver, ReqwestPdsClient, VerifyingResolver, CachingResolver, Resolver,
};
use std::sync::Arc;
use std::time::Duration;

let client = ReqwestPdsClient::with_service_url("https://bsky.social");
let inner: Arc<dyn Resolver> = Arc::new(PdsResolver::new(client));
let verifying = Arc::new(VerifyingResolver::sha256(inner));
let resolver = CachingResolver::new(verifying, Duration::from_secs(300));

let lens = resolver.resolve(&lens_uri).await?;
```

`VerifyingResolver` re-hashes the bytes the inner resolver returned
and rejects the record on mismatch. `CachingResolver` keeps the
result in a TTL'd cache so repeated `apply_lens` calls do not
re-fetch.

`Arc<dyn Resolver>` is supported (since v0.8.0); the resolver
futures are `Send`, so handlers in async HTTP frameworks can hold
the resolver behind a trait object and call `apply_lens` from
inside an `#[async_trait]` impl.

## Make it discoverable

The orchestrator is the catalog the network queries. Once the
firehose indexer ingests your `dev.panproto.schema.lens` commit,
the orchestrator's lens query will return it. To make consumers
prefer your lens over alternatives:

- Publish a `dev.idiolect.recommendation` from a community DID
  endorsing the lens path under stated conditions.
- Publish `dev.idiolect.verification` records covering the
  properties consumers care about.
- Register the lens in a `dev.idiolect.dialect`'s
  `preferredLenses` so dialect-aware consumers find it without a
  separate query.
