# idiolect-records

> **API reference:** [docs.rs/idiolect-records](https://docs.rs/idiolect-records/latest/idiolect_records/)
> · **Source:** [`crates/idiolect-records/`](https://github.com/idiolect-dev/idiolect/tree/main/crates/idiolect-records)
> · **Crate:** [crates.io/idiolect-records](https://crates.io/crates/idiolect-records)
>
> This page is an editorial overview. The per-symbol surface
> (every public type, trait, function, and feature flag) is the
> docs.rs link above. That is the authoritative reference.

The crate provides Serde record types that mirror the `dev.idiolect.*`
[lexicons](../../glossary.md#lexicon "A schema document in the AT Protocol Lexicon language"). The
contents of `crates/idiolect-records/src/generated/` are written
by [`idiolect-codegen`](./idiolect-codegen.md). Do not edit by
hand.

```toml
[dependencies]
idiolect-records = "0.11.1"
```

The crate has no transport dependencies; it contains data types and their
validation and dispatch helpers.

## Public types

### `Record` trait

Every generated record type implements `Record`, with associated
constants and methods that let consumers be generic over the
family.

### `AnyRecord` enum

The dispatch primitive returned by
`decode_record(&nsid, value)`. One variant per shipped
`dev.idiolect.*` record kind (`Adapter`, `Belief`, `Bounty`,
`Community`, `Correction`, `Deliberation`,
`DeliberationOutcome`, `DeliberationStatement`,
`DeliberationVote`, `Dialect`, `Encounter`, `Observation`,
`Recommendation`, `Retrospection`, `Verification`, `Vocab`).

The vendored panproto record types (`PanprotoLens`,
`PanprotoSchema`, `PanprotoTheory`, `PanprotoProtolens`,
`PanprotoProtolensChain`, `PanprotoComplement`,
`PanprotoLensAttestation`, `PanprotoProtocol`, plus
`PanprotoCommit`, `PanprotoRefUpdate`, `PanprotoRepo`) are
re-exported at the crate root as their own structs. They are
not variants of `AnyRecord` (which is scoped to
`IdiolectFamily`'s NSIDs).

### Family

[`RecordFamily`](../../glossary.md#record-family "A typed set of record NSIDs with a shared decoder")
is the trait every family implements. The crate
ships `IdiolectFamily` for `dev.idiolect.*` and the
`OrFamily<F1, F2>` composer that recognizes every NSID either
side claims. `detect_or_family_overlap` audits a probe set at
boot so a configuration mistake does not silently shadow the
right-side family.

### Typed wrappers

| Type | Format |
| --- | --- |
| `AtUri` | `at-uri` |
| `Did` | `did` |
| `Nsid` | `nsid` |
| `Datetime` | RFC 3339 |
| `Uri` | URL |
| `Cid` | CID |
| `Language` | BCP 47 |

Each wraps a string with a parser. The parser fires at
deserialize time. `Display` / `as_str` / `Deref<Target=str>` are
uniform.

### Vocab graph helpers

`VocabGraph` is a normalized read-only view over a `Vocab`
record (graph form, lifted from the legacy tree where present).
`VocabRegistry` caches multiple graphs by AT-URI for
cross-vocabulary work. The shipped query verbs
(`walk_relation` on the graph; `is_subsumed_by`, `satisfies`,
`translate` on the registry)
plus the `validate` walker are documented on docs.rs and in
[The vocabulary knowledge graph](../../concepts/vocab-graph.md).

## Examples module

`idiolect_records::examples::*` exports a fixture per record
kind. Each fixture is the deserialized result of the JSON
constant under `lexicons/dev/idiolect/examples/<name>.json`. The
shipped fixtures cover: `adapter`, `belief`, `bounty`,
`community`, `correction`, `dialect`, `encounter`,
`observation`, `recommendation`, `retrospection`, `verification`,
`vocab`, plus the vendored panproto records (`panproto_lens`,
`panproto_schema`, `panproto_commit`, ...). Use them in tests so
you do not have to hand-roll JSON.

The four deliberation lexicons do not currently ship example
fixtures. Consumers building deliberation tests construct
records directly via the typed structs.

## Feature flags

None. The crate is feature-flag-free and has no transport
dependencies.

## Errors

The family-decode path returns `DecodeError`, re-exported as
`idiolect_records::DecodeError`. `UnknownNsid(String)` reports an
NSID outside the generated family; `Serde(serde_json::Error)` reports
a typed deserialization failure. The indexer's separate
`IndexerError::FamilyContract` variant detects disagreement between a
family's `contains` and `decode` methods.
