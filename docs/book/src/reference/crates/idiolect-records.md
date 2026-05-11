# idiolect-records

> **API reference:** [docs.rs/idiolect-records](https://docs.rs/idiolect-records/latest/idiolect_records/)
> · **Source:** [`crates/idiolect-records/`](https://github.com/idiolect-dev/idiolect/tree/main/crates/idiolect-records)
> · **Crate:** [crates.io/idiolect-records](https://crates.io/crates/idiolect-records)
>
> This page is an editorial overview. The per-symbol surface
> (every public type, trait, function, and feature flag) is the
> docs.rs link above; that is the authoritative reference.

Serde record types mirroring the `dev.idiolect.*` lexicons. The
contents of `crates/idiolect-records/src/generated/` are written
by [`idiolect-codegen`](./idiolect-codegen.md); do not edit by
hand.

```toml
[dependencies]
idiolect-records = "0.8"
```

No transport dependencies. Pure data.

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
re-exported at the crate root as their own structs; they are
not variants of `AnyRecord` (which is scoped to
`IdiolectFamily`'s NSIDs).

### Family

`RecordFamily` is the trait every family implements; the crate
ships `IdiolectFamily` for `dev.idiolect.*` and the
`OrFamily<F1, F2>` composer that recognises every NSID either
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

Each wraps a string with a parser; the parser fires at
deserialize time. `Display` / `as_str` / `Deref<Target=str>` are
uniform.

### Vocab graph helpers

`VocabGraph` is a normalised read-only view over a `Vocab`
record (graph form, lifted from the legacy tree where present).
`VocabRegistry` caches multiple graphs by AT-URI for
cross-vocabulary work. The shipped query verbs
(`walk_relation` on the graph; `is_subsumed_by`, `satisfies`,
`translate` on the registry)
plus the `validate` walker are documented on docs.rs and in
[The vocabulary knowledge graph](../../concepts/vocab-graph.md).

## Examples module

`idiolect_records::examples::*` exports a fixture per record
kind. Each fixture is the deserialised result of the JSON
constant under `lexicons/dev/idiolect/examples/<name>.json`. The
shipped fixtures cover: `adapter`, `belief`, `bounty`,
`community`, `correction`, `dialect`, `encounter`,
`observation`, `recommendation`, `retrospection`, `verification`,
`vocab`, plus the vendored panproto records (`panproto_lens`,
`panproto_schema`, `panproto_commit`, ...). Use them in tests so
you do not have to hand-roll JSON.

The four deliberation lexicons do not currently ship example
fixtures; consumers building deliberation tests construct
records directly via the typed structs.

## Feature flags

None. The crate is feature-flag-free and has no transport
dependencies.

## Errors

Decode failures surface as `serde_json::Error` with a
`serde_path_to_error`-shaped path. The structured error type for
the family-decode path is `DecodeError` (re-exported as
`idiolect_records::DecodeError`). It distinguishes unknown
NSIDs, decode failures, and family-contract violations (where a
family's `contains` returned true but `decode` returned `None`).
