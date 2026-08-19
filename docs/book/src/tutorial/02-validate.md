# Validate a typed record

The record fetched in Chapter 1 came from the network. We now use a bundled
fixture so that validation has a stable input and a reproducible result.

An ATProto [lexicon](../glossary.md#lexicon "AT Protocol schema document") defines
a record's fields and constraints. `idiolect-records` generates a Rust type for
each `dev.idiolect.*` lexicon and dispatches incoming JSON by its
[namespaced identifier (NSID)](../glossary.md#nsid "Namespaced identifier").

## Run the valid case

From the repository root, run the tutorial validator:

```bash
cargo run --quiet \
  --manifest-path scripts/publish-tutorial-lens/Cargo.toml \
  --bin validate-tutorial-record
```

```text
validated dev.idiolect.dialect: ud-en-2026
```

The executable serializes the bundled `Dialect` fixture to JSON and sends it
through the runtime dispatcher:

```text
let value = serde_json::to_value(examples::dialect())?;
let record = decode_record(&Dialect::nsid(), value)?;
```

`Dialect::nsid()` selects the generated type. Deserialization then checks the
required fields and the field formats represented by that type. The returned
`AnyRecord::Dialect` value is safe to pass to code that expects a typed dialect.

## See a rejection

The same executable can remove the required `createdAt` field before decoding:

```bash
cargo run --quiet \
  --manifest-path scripts/publish-tutorial-lens/Cargo.toml \
  --bin validate-tutorial-record -- --invalid
```

```text
rejected invalid dev.idiolect.dialect record: record deserialization failed: missing field `createdAt`
```

Validation occurs here: malformed JSON is rejected before application logic
receives a record. Chapter 3 applies the live lens from Chapter 1 to a small
source record.
