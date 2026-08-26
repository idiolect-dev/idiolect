# Apply the lens

A [lens](../glossary.md#lens "Bidirectional schema translation") translates a
record between two schemas. Its forward operation, `get`, returns the target
record and a [complement](../glossary.md#complement "Data retained for put") that
retains any source information needed by the reverse operation, `put`.

This chapter applies the lens fetched in Chapter 1 to the following source
record:

```json
{
  "text": "hello, world"
}
```

## Confirm the Panproto version

The runnable package pins the Panproto crates used here to 0.71.0:

```toml
panproto-lens   = { git = "https://github.com/panproto/panproto.git", tag = "v0.72.0" }
panproto-schema = { git = "https://github.com/panproto/panproto.git", tag = "v0.72.0" }
```

The idiolect workspace uses the same Panproto version.

## Run `get`

From the repository root, run the supplied client:

```bash
cargo run --quiet \
  --manifest-path scripts/publish-tutorial-lens/Cargo.toml \
  --bin apply-tutorial-lens
```

```text
target_record = {
  "text": "hello, world"
}
```

The executable creates one HTTP client, gives it to the lens and schema
resolvers, and calls `apply_lens`:

```text
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
```

The JSON value is unchanged because this tutorial lens renames a schema sort
from `string` to `text`; it does not rename the record's `text` field. The
unchanged value records a successful run: the runtime resolved the lens, loaded
both schemas, instantiated Panproto 0.71.0, and produced a target-schema value.

The returned `out.complement` belongs with this application of the lens.
Passing it to `apply_lens_put` would reconstruct the source record. Chapter 4
checks that reconstruction over several inputs.
