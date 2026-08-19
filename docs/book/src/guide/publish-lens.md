# Publish and resolve a lens

A [lens](../glossary.md#lens "A bidirectional transformation between data schemas")
on the network depends on three records:

1. A `dev.panproto.schema.lens` record on a PDS, carrying the
   protolens (or protolens chain) blob and pointers to the source
   and target schemas.
2. Its source schema, as a `dev.panproto.schema.schema` record.
3. Its target schema, as a `dev.panproto.schema.schema` record. A
   `getSchema` XRPC response from a panproto VCS may supply either
   schema instead.

Consumers resolve the lens record by at-uri. The runtime
instantiates against the two schemas.

## Build the lens

The shortest path is to derive it from a schema diff:

```bash
schema lens generate --protocol atproto old.json new.json --save chain.json
```

`schema` is the panproto CLI. `chain.json` is a protolens chain in
panproto's serialized format. See the
[panproto book](https://panproto.dev/book/) for what the chain
looks like and how to inspect it.

## Stage it

```text
use idiolect_records::{AtUri, Datetime, PanprotoLens, PanprotoLensRoundTripClass};

let chain: serde_json::Value = serde_json::from_slice(&std::fs::read("chain.json")?)?;

let lens = PanprotoLens {
    blob: Some(chain),
    created_at: Datetime::parse("2026-04-19T00:00:00.000Z").unwrap(),
    laws_verified: Some(true),
    object_hash: format!("sha256:{}", sha256_hex(&blob_bytes)),
    round_trip_class: Some(PanprotoLensRoundTripClass::Iso),
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
- `round_trip_class` uses the wire values `iso`, `retraction`,
  `projection`, and `opaque`. Consumers may use the class to select
  an appropriate review path.
- `laws_verified` is a soft assertion that the chain passed
  panproto's coercion-law and existence checks. A true value here
  is meaningless without a corresponding `dev.idiolect.verification`
  record from a publisher you trust. Treat it as a pre-publish
  smoke signal.

## Publish

Construct a `SigningPdsWriter` from a reqwest PDS client plus a
DPoP prover, wrap it in a `RecordPublisher`, and call `create`:

```text
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
caller's job. See [Configure OAuth sessions](./oauth.md). A PDS may
validate the record's lexicon shape, but it does not run idiolect's
resolver or content-hash checks. Validate the chain and hash before
publication; consumers should resolve through `VerifyingResolver`.

## Resolve it

The complement of publishing is resolving. Given an at-uri, the
[`Resolver`](../reference/crates/idiolect-lens.md#resolvers) trait
hands back a `PanprotoLens` record:

```text
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

`Resolver` is object-safe, and its futures are `Send`. Thus, an async
service may hold `Arc<dyn Resolver>` and call `apply_lens` from a
spawnable request future.

## Make it discoverable

The 0.12.0 orchestrator catalogs `dev.idiolect.*` records, not
`dev.panproto.schema.lens` records. Consumers fetch a known lens URI
directly. Publish idiolect records that point to the lens to make that
URI discoverable:

- Publish a `dev.idiolect.recommendation` from a community DID
  endorsing the lens path under stated conditions.
- Publish `dev.idiolect.verification` records covering the
  properties consumers care about.
- Register the lens in a `dev.idiolect.dialect`'s
  `preferredLenses` so dialect-aware consumers find it without a
  separate query.
