# Open enums and vocabularies

Closed enums are the wrong default for a federated lexicon.
Adding a value should not require coordinating with every
consumer; refusing to accept a new value should not be the
default.

idiolect's policy is: every enum-shaped field is open, and the
extension story is mechanical.

## The wire shape

An open-enum field carries `knownValues` and a sibling `*Vocab`
reference:

```json
"kind": {
  "type": "string",
  "knownValues": ["subprocess", "http", "wasm"],
  "description": "Slug; resolves as a node in the vocab referenced by `kindVocab`."
},
"kindVocab": {
  "type": "ref",
  "ref": "dev.idiolect.defs#vocabRef",
  "description": "Vocabulary record whose nodes constitute the open extension."
}
```

`kindVocab` is optional. When omitted, the canonical
idiolect-published vocab for that field is the implicit default.
A community-published vocab listed here extends the slugs the
field accepts.

## The codegen shape

`idiolect-codegen` reads `knownValues` and emits:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Kind {
    Subprocess,
    Http,
    Wasm,
    Other(String),
}
```

Plus hand-written `Serialize` / `Deserialize` impls that round-trip
unknown slugs through `Other(String)`. The TypeScript half emits
`'a' | 'b' | (string & {})` for the same purpose.

Three helper methods sit on every emitted open-enum type:

```rust
impl Kind {
    pub fn is_subsumed_by(
        &self,
        graph: &VocabGraph,
        ancestor: &str,
    ) -> bool { /* ... */ }

    pub fn satisfies(
        &self,
        graph: &VocabGraph,
        relation: &str,
        target: &str,
    ) -> bool { /* ... */ }

    pub fn translate_to<T: From<String>>(
        &self,
        src_vocab_uri: &str,
        tgt_vocab_uri: &str,
        registry: &VocabRegistry,
    ) -> Option<T> { /* ... */ }
}
```

These are what consumers call instead of comparing strings. A
consumer asking "is this `kind` a subprocess?" calls
`k.is_subsumed_by(&vocab, "subprocess")` and gets `true` for any
slug the vocab declares as `subsumed_by` subprocess (`docker-run`,
`fly-machines-launch`, etc.) without changing the consumer's code.

## Why this shape

Closed enums force a coordination problem: adding a value requires
every consumer to upgrade their code before any producer publishes
the new value. Open enums turn it into a vocabulary problem: the
producer publishes the vocab, the consumer queries the vocab at
runtime, and unknown slugs degrade gracefully to `Other(String)`
when the consumer has not loaded the vocab.

The cost is one extra indirection per slug interpretation. The
shipped `VocabRegistry` caches vocabs by at-uri, so the cost is
amortized across the process lifetime.

## What stays closed

A few fields are intentionally closed. They are meta-policy fields
where extending the value space would change the runtime's
contract, not the data:

- `vocab.world` (`open` / `closed-with-default` / `hierarchy-closed`)
  controls the runtime's open-enum policy itself.
- `lensClass` (`isomorphism` / `injection` / `projection` /
  `affine` / `general`) is a panproto contract; extending it
  changes what the runtime promises.
- `recordHosting` (`member-hosted` / `community-hosted` / `hybrid`)
  controls a federation policy.

A new value here is a runtime change, not a record change.

## Migration

Converting a closed enum to an open enum is wire-compatible:
existing records continue to validate, and the codegen-emitted
helpers degrade to "if `Other`, ignore" in consumers that have
not regenerated. Going the other way is breaking; the shipped
lexicons do not do that.

## Codegen identifier collisions

When two distinct slugs would pascal-case to the same Rust
identifier (`foo-bar` and `foo_bar`), the second occurrence gets
a numeric suffix (`FooBar2`). The collision is resolved
deterministically per lexicon, so two regenerations of the same
lexicon produce the same identifier names. The collision report
is printed at codegen time so authors can rename a slug when the
generated name is awkward.
