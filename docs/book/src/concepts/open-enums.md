# Open enums and vocabularies

ATProto Lexicon distinguishes suggested string values from closed enumeration.
`knownValues` lists common values but does not restrict the string; `enum`
defines a closed set. The distinction is part of the official
[Lexicon string specification](https://atproto.com/specs/lexicon#string).

idiolect builds its extension convention on `knownValues`. We call the
combination of an open slug and an optional vocabulary reference the
**open-enum pair (OEP)**.

## Wire shape

The adapter Lexicon contains an OEP for its invocation protocol:

```json
{
  "kind": {
    "type": "string",
    "knownValues": ["subprocess", "http", "wasm"]
  },
  "kindVocab": {
    "type": "ref",
    "ref": "dev.idiolect.defs#vocabRef"
  }
}
```

The record's `kind` value may be `subprocess` or a value that did not exist when
the consumer generated its bindings. `kindVocab`, when present, identifies a
[vocabulary](../glossary.md#vocabulary "A published graph that assigns relations and annotations to open slugs")
in which the slug can be interpreted.

Many idiolect Lexicons describe an omitted `*Vocab` field as selecting a
canonical project vocabulary. That default is a convention in the schema
description, not a URI inserted by deserialization. A consumer that needs graph
semantics must choose or configure the default record itself.

## Generated bindings

The Rust generator turns the example into
`AdapterInvocationProtocolKind::{Subprocess, Http, Wasm, Other(String)}`.
Serialization preserves the wire slug, including the string inside `Other`.
The TypeScript generator emits the literal union
`"subprocess" | "http" | "wasm" | string & {}` so editors retain completion for
known values without rejecting extensions.

Rust open-enum types also expose three graph-facing operations:

- `is_subsumed_by` tests the `subsumed_by` relation in one `VocabGraph`.
- `satisfies` tests reachability under a caller-selected relation.
- `translate_to` asks a `VocabRegistry` for an `equivalent_to` translation
  between two registered vocabulary URIs.

None of these methods fetches a vocabulary record. Loading, validating, and
caching those records remains the caller's responsibility.

## Preservation before interpretation

The OEP separates two requirements. **Preservation** means that an old consumer
can decode and reserialize an unfamiliar slug without replacing it. Generated
`Other(String)` variants provide that behavior. **Interpretation** means that a
consumer knows how the slug relates to a requirement such as `subprocess`.
Interpretation requires a loaded graph and a relation query.

This separation avoids a common failure mode in federated systems: treating an
unknown value as invalid merely because local code has not seen it. It does not
require a consumer to accept the value for every purpose. A policy may preserve
`fly-machine` on the wire and still decline to execute it because the relevant
vocabulary is missing or untrusted.

## Closed fields

The shipped Lexicons still use `enum` for meta-policy fields whose extension
would alter a parser or runtime contract. `vocab.world` and the per-relation
`world` override, for instance, are closed over `open`, `closed-with-default`,
and `hierarchy-closed`. Unions may also be explicitly closed under the Lexicon
rules.

Changing a field from `enum` to `knownValues` expands the accepted wire values,
but it can also change generated source types. Existing record values remain in
the larger set; downstream code still needs regeneration and review.

## Identifier collisions

Distinct slugs can normalize to the same Rust variant name. The generator keeps
the first name and adds a numeric suffix to later collisions, deterministically
within the generated enum. It also avoids using `Other` as the fallback name
when `Other` is itself a declared slug, selecting another fallback variant
instead. These rules preserve every wire value, though authors should still
prefer slugs whose generated names remain readable.

[The vocabulary knowledge graph](./vocab-graph.md) develops the graph semantics
that turn preserved strings into queryable relations.
