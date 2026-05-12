# Validate against the lexicon

Every `dev.idiolect.*` lexicon ships in two forms: the JSON document
under `lexicons/dev/idiolect/` and a generated Rust type under
[`idiolect_records::generated`](../reference/crates/idiolect-records.md).
The generated type carries serde codecs that fail closed: a record
that does not match the lexicon will not deserialize.

## Add the records crate

```bash
cargo new --lib idiolect-tutorial
cd idiolect-tutorial
cargo add idiolect-records anyhow serde_json tokio --features tokio/macros,tokio/rt
```

`idiolect-records` is a small crate (no transport dependencies). It
exports one Rust type per record kind plus the
[`Record`](../reference/crates/idiolect-records.md) trait.

## Decode a record by NSID

The crate ships minimally-valid fixtures for every record kind
under `idiolect_records::examples`. They are real records: same
shape the codec emits for a wire-format publish, same
constraints, same `Record` impl. Use them to exercise the
decode path without depending on what's on the network.

In `src/main.rs`:

```rust
use idiolect_records::{decode_record, examples, AnyRecord, Dialect, Record};

fn main() -> anyhow::Result<()> {
    // The fixture is a typed value; serialise it back to JSON
    // and decode through the dispatcher to exercise the same
    // path a firehose handler would take.
    let dialect = examples::dialect();
    let value = serde_json::to_value(&dialect)?;

    let rec = decode_record(&Dialect::nsid(), value)?;

    match rec {
        AnyRecord::Dialect(d) => println!(
            "{}: {} idiolects, {} preferred lenses",
            d.name,
            d.idiolects.as_ref().map_or(0, |v| v.len()),
            d.preferred_lenses.as_ref().map_or(0, |v| v.len()),
        ),
        other => anyhow::bail!(
            "expected a dialect, got {}",
            other.nsid_str(),
        ),
    }
    Ok(())
}
```

Run it:

```bash
cargo run
```

```text
ud-en-2026: 0 idiolects, 0 preferred lenses
```

`decode_record` is the dispatch primitive: it takes an NSID and
a JSON value, looks up the matching `Record` impl, and hands
back an `AnyRecord`. If the JSON does not match the schema, it
returns an error pointing at the first invalid field.

The same path works for a JSON file fetched from a PDS, once
some party publishes a `dev.idiolect.dialect` record on the
network: read the bytes from disk (or from the
`com.atproto.repo.getRecord` body), parse as
`serde_json::Value`, hand to `decode_record`. The fixture
shortcut is just a way to make this chapter not depend on
network state.

## Validation is structural, not just type-shaped

The generated codecs validate every constraint declared in the
lexicon, not just the field types:

- `maxLength` and `maxGraphemes` on strings.
- `format` constraints (`at-uri`, `did`, `nsid`, `datetime`,
  `language`, `cid-link`).
- `knownValues` for open enums (the value is preserved verbatim, but
  the codec records whether it was a known slug or fell through to
  `Other(String)` so consumers can decide).
- `required` arrays.
- `union` discriminator tags via `$type`.

A record that violates one of these surfaces as a deserialization
error with a `serde_path_to_error`-shaped path:

```text
Error: dialect.entries[0].nsid: invalid format `at-uri`: missing scheme
```

The boundary is exactly where you want it: at the parse, before any
business logic touches the value.

## Map of the family

The generated tree mirrors the lexicons one-to-one. As of v0.8.0:

| Record | Module | NSID |
| --- | --- | --- |
| `Adapter` | `adapter` | `dev.idiolect.adapter` |
| `Belief` | `belief` | `dev.idiolect.belief` |
| `Bounty` | `bounty` | `dev.idiolect.bounty` |
| `Community` | `community` | `dev.idiolect.community` |
| `Correction` | `correction` | `dev.idiolect.correction` |
| `Deliberation` | `deliberation` | `dev.idiolect.deliberation` |
| `DeliberationStatement` | `deliberation_statement` | `dev.idiolect.deliberationStatement` |
| `DeliberationVote` | `deliberation_vote` | `dev.idiolect.deliberationVote` |
| `DeliberationOutcome` | `deliberation_outcome` | `dev.idiolect.deliberationOutcome` |
| `Dialect` | `dialect` | `dev.idiolect.dialect` |
| `Encounter` | `encounter` | `dev.idiolect.encounter` |
| `Observation` | `observation` | `dev.idiolect.observation` |
| `Recommendation` | `recommendation` | `dev.idiolect.recommendation` |
| `Retrospection` | `retrospection` | `dev.idiolect.retrospection` |
| `Verification` | `verification` | `dev.idiolect.verification` |
| `Vocab` | `vocab` | `dev.idiolect.vocab` |

Fixtures are exported under `idiolect_records::examples::*` for
every record kind except the four deliberation lexicons, which
are recent additions still awaiting bundled fixtures. The
shipped fixtures are minimally-valid: every required field is
present and the codec round-trip is a no-op.

## Reusable trait surface

Anything you write that consumes records can be generic over the
`Record` trait:

```rust
use idiolect_records::Record;

fn describe<R: Record>() -> String {
    format!("{} record (NSID: {})", R::kind(), R::NSID)
}
```

`R::kind()` returns the short kind name (`"encounter"`,
`"recommendation"`, ...) and `R::NSID` is the fully-qualified
NSID constant. `Record` does not carry instance-level methods on
the record body itself; the at-uri at which a record lives is
external (a `(did, collection, rkey)` triple from the firehose
or PDS response).

The same trait is what the indexer (chapter on
[indexing a firehose](../guide/index-firehose.md)) uses to
filter out-of-family commits before decode.

The next chapter takes a record we trust and runs it through a
panproto lens.
