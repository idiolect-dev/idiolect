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

A `SchemaLoader` impl that fetches `dev.panproto.schema.schema`
records from a PDS does not ship in v0.8.0 (the shipped
`FilesystemSchemaLoader` reads ATProto lexicons; the records
this tutorial uses carry serialized panproto `Schema` graphs).
The contract is small, so we write one in the program. Add
`reqwest` to the dependencies:

```toml
reqwest = { version = "0.12", features = ["json"] }
```

Then `src/main.rs`:

```rust
use std::pin::Pin;

use idiolect_lens::{
    apply_lens, ApplyLensInput, AtUri, LensError, PdsResolver,
    ReqwestPdsClient, SchemaLoader,
};
use panproto_schema::{Protocol, Schema};

const PDS: &str = "https://jellybaby.us-east.host.bsky.network";
const LENS: &str = "at://did:plc:wdl4nnvxxdy4mc5vddxlm6f3/dev.panproto.schema.lens/tutorial-rename-sort-string-to-text";

struct PdsSchemaLoader { http: reqwest::Client }

impl PdsSchemaLoader {
    fn new() -> Self { Self { http: reqwest::Client::new() } }
}

impl SchemaLoader for PdsSchemaLoader {
    fn load<'a>(
        &'a self,
        at_uri: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Schema, LensError>> + Send + 'a>> {
        Box::pin(async move {
            let rest = at_uri.strip_prefix("at://").ok_or_else(|| {
                LensError::Transport(format!("not an at-uri: {at_uri}"))
            })?;
            let mut parts = rest.splitn(3, '/');
            let (did, coll, rkey) = match (parts.next(), parts.next(), parts.next()) {
                (Some(d), Some(c), Some(r)) => (d, c, r),
                _ => return Err(LensError::Transport(format!("malformed at-uri: {at_uri}"))),
            };
            let url = format!(
                "{PDS}/xrpc/com.atproto.repo.getRecord?repo={did}&collection={coll}&rkey={rkey}"
            );
            let body: serde_json::Value = self.http.get(&url).send().await
                .map_err(|e| LensError::Transport(format!("{e}")))?
                .json().await
                .map_err(|e| LensError::Transport(format!("{e}")))?;
            let blob = body.get("value").and_then(|v| v.get("blob")).cloned()
                .ok_or_else(|| LensError::LexiconParse("no blob".into()))?;
            serde_json::from_value(blob)
                .map_err(|e| LensError::LexiconParse(e.to_string()))
        })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client   = ReqwestPdsClient::with_service_url(PDS);
    let resolver = PdsResolver::new(client);
    let loader   = PdsSchemaLoader::new();
    let protocol = Protocol::default();

    let lens_uri = AtUri::parse(LENS)?;
    let source_record: serde_json::Value =
        serde_json::from_str(r#"{ "text": "hello, world" }"#)?;

    let out = apply_lens(&resolver, &loader, &protocol, ApplyLensInput {
        lens_uri, source_record, source_root_vertex: None,
    }).await?;

    println!("{}", serde_json::to_string_pretty(&out.target_record)?);
    Ok(())
}
```

Run it:

```bash
cargo run
```

```text
{
  "text": "hello, world"
}
```

The lens referenced above is real. The project DID has the
three records the runtime needs to resolve it published on its
PDS:

- `dev.panproto.schema.schema/tutorial-post-body-v1` — a
  single-field "post:body" record with a string `text` child.
- `dev.panproto.schema.schema/tutorial-post-body-v2` — the same
  shape with the kind relabelled to `text`.
- `dev.panproto.schema.lens/tutorial-rename-sort-string-to-text`
  — a single-step `rename_sort` chain. The optic class is
  `Iso`; round-trip is byte-equal.

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
