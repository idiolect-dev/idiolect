# Apply a lens

A lens is a structure-preserving translation between two schemas.
On the wire it is a `dev.panproto.schema.lens` record (re-exported
by `idiolect-records` as `PanprotoLens`); at runtime it is a
`panproto_lens::Lens` instantiated against a `Schema` graph.
[`idiolect-lens`](../reference/crates/idiolect-lens.md) bridges the
two: it resolves a lens record by at-uri, loads both schemas, and
runs the lens.

## What a lens does

A lens has a forward direction and a backward direction:

$$
\get : A \to (B, \complement) \qquad \put : (B, \complement) \to A
$$

`get` translates a source record `A` into a target view `B` plus a
**complement** $\complement$ (the data that the projection
discarded). `put` reconstructs `A` from a (possibly modified) `B`
and the complement. The two directions obey the GetPut and PutGet
laws covered in [Lens semantics and laws](../concepts/lens-laws.md).

## Wire it up

`idiolect-lens` is `publish = false`; depend on it via git (or a
path, when working inside the workspace):

```toml
# in Cargo.toml
idiolect-lens = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.8.0", features = ["pds-reqwest"] }
tokio = { version = "1", features = ["full"] }
```

```rust
use idiolect_lens::{
    apply_lens, ApplyLensInput, AtUri, FilesystemSchemaLoader, PdsResolver,
    ReqwestPdsClient,
};
use panproto_schema::Protocol;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = ReqwestPdsClient::with_service_url("https://bsky.social");
    let resolver = PdsResolver::new(client);
    let loader = FilesystemSchemaLoader::new("./schema-cache")?;
    let protocol = Protocol::default();

    let lens_uri = AtUri::parse(
        "at://did:plc:idiolect.dev/dev.panproto.schema.lens/example",
    )?;
    let source_record: serde_json::Value = serde_json::from_str(SAMPLE)?;

    let out = apply_lens(
        &resolver,
        &loader,
        &protocol,
        ApplyLensInput {
            lens_uri,
            source_record,
            source_root_vertex: None,
        },
    )
    .await?;

    println!("{}", serde_json::to_string_pretty(&out.target_record)?);
    Ok(())
}

const SAMPLE: &str = r#"{ "title": "hello", "body": "world" }"#;
```

`apply_lens` is one async call. It does five things in order:

1. Resolve the lens record from the PDS via `PdsResolver`.
2. Load the source and target schemas from the schema loader.
3. Instantiate the protolens (or protolens chain) against the source
   schema under the given protocol.
4. Parse `source_record` into a panproto w-type instance, project it
   through `get`, and serialize the view back to JSON under the
   target schema.
5. Return the target record together with the complement.

The complement is a typed `panproto_lens::Complement`. Treat it
as an opaque token: store it next to the target view, hand it
back to `apply_lens_put` when you want to run the reverse
direction, do not edit it.

## Reverse the direction

```rust
use idiolect_lens::{apply_lens_put, ApplyLensPutInput};

let back = apply_lens_put(
    &resolver,
    &loader,
    &protocol,
    ApplyLensPutInput {
        lens_uri: lens_uri.clone(),
        target_record: out.target_record,
        complement: out.complement,
        target_root_vertex: None,
    },
)
.await?;

assert_eq!(back.source_record, source_record);
```

If the lens is an isomorphism, `put(get(a))` returns the original
`a` byte-for-byte. If it is a projection (information was dropped on
the way through), `put` reconstructs `a` from the target plus the
complement. If you modify the target between the calls, `put`
applies the modification on top of the original source.

## What can go wrong

| Symptom | Cause |
| --- | --- |
| `LensError::NotFound` | The lens at-uri did not resolve. Check the DID and the rkey. |
| `LensError::LexiconParse` | The schema loader returned bytes that were not a valid panproto schema. |
| `LensError::Translate` | The source record did not parse as an instance of the source schema. |
| Output complement is huge | The lens is closer to a projection than you thought. See [Lens semantics](../concepts/lens-laws.md). |

The runtime is `Send`-clean. You can hold an `Arc<dyn Resolver>` and
call `apply_lens` from inside an `#[async_trait]` handler in an
HTTP server.

The next chapter takes that lens and runs a verification against
it.
