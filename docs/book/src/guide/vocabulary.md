# Author a community vocabulary

[Open-enum](../glossary.md#open-enum "An enum that preserves unknown string values")
slugs may point to a `dev.idiolect.vocab` record. The record stores a
typed graph whose relation nodes may declare
[OWL 2](https://www.w3.org/TR/owl2-syntax/) property characteristics and whose
concept nodes may carry SKOS Core fields.

This guide covers the authoring side: writing the JSON, declaring
relation properties, and publishing the record.

## Minimum vocabulary

Every vocabulary needs a name, a description, and at least one
node:

```json
{
  "$type": "dev.idiolect.vocab",
  "name": "vote-stances",
  "description": "Default deliberation vote stances.",
  "world": "open",
  "nodes": [
    { "id": "agree",    "kind": "concept", "label": "Agree" },
    { "id": "disagree", "kind": "concept", "label": "Disagree" },
    { "id": "pass",     "kind": "concept", "label": "Pass" }
  ],
  "edges": [],
  "occurredAt": "2026-05-01T00:00:00.000Z"
}
```

`world` controls whether unknown values are accepted as
extensions:

- `open` — unknown values are first-class extensions.
- `closed-with-default` — unknown values fall back to a designated
  default.
- `hierarchy-closed` — unknown values are rejected.

Per-relation overrides live on the relation node's metadata.

## Add typed relations

A relation is itself a node, with its algebraic properties
declared as metadata:

```json
{
  "id": "polar_opposite_of",
  "kind": "relation",
  "label": "Polar opposite of",
  "metadata": {
    "symmetric": true,
    "transitive": false,
    "reflexive": false,
    "irreflexive": true
  }
}
```

The shipped fields cover these OWL 2 property characteristics: `symmetric`,
`asymmetric`, `transitive`, `reflexive`, `irreflexive`,
`functional`, `inverseFunctional`, plus `inverseOf` and a per-
relation `world` override. `VocabGraph::validate` walks asserted edges
and reports violations. `RecordPublisher` does not call this method,
so run it before publication.

## Add edges

Now express the relations on top of the nodes:

```json
{
  "edges": [
    { "source": "agree",    "target": "disagree", "relationSlug": "polar_opposite_of" },
    { "source": "disagree", "target": "agree",    "relationSlug": "polar_opposite_of" }
  ]
}
```

If the relation is symmetric the runtime walks both directions, so
you can author either direction (or both, redundantly).

## Annotate with SKOS Core

Every concept node accepts SKOS-style annotations:

```json
{
  "id": "agree",
  "kind": "concept",
  "label": "Agree",
  "alternateLabels": ["yes", "+1"],
  "hiddenLabels": ["agreed", "agrees"],
  "scopeNote": "Use when the voter affirms the statement as written.",
  "example": "I agree with the proposal as drafted.",
  "notation": "1",
  "externalIds": [
    { "system": "wikidata", "identifier": "Q4116214", "matchType": "exact" }
  ]
}
```

The full annotation set is `label`, `alternateLabels`,
`hiddenLabels`, `description` (definition), `scopeNote`, `example`,
`historyNote`, `editorialNote`, `changeNote`, `notation`, and
`externalIds`. The `matchType` values on `externalIds` carry SKOS
semantics (`exact`, `close`, `broader`, `narrower`, `related`).

A `kind: "collection"` plus `member_of` edges expresses a SKOS
Collection.

## Validate

```text
use idiolect_records::{Vocab, vocab::VocabGraph};

let bytes = std::fs::read("vocab.json")?;
let vocab: Vocab = serde_json::from_slice(&bytes)?;
let violations = VocabGraph::from_vocab(&vocab).validate();
if !violations.is_empty() {
    anyhow::bail!("vocabulary violations: {violations:#?}");
}
```

Deserialization checks the generated record shape. `validate()` then
returns `Vec<VocabViolation>`, with one entry for each graph-property
failure. Panproto's `schema check` compares schema migrations; it does
not validate a `dev.idiolect.vocab` record.

## Publish

Same path as any other record. Construct a writer, wrap it in
`RecordPublisher`, and call `create`:

```text
use idiolect_lens::{
    P256DpopProver, RecordPublisher, ReqwestPdsClient, SigningPdsWriter,
};
use idiolect_records::Vocab;

let vocab: Vocab = serde_json::from_slice(&std::fs::read("vote-stances.json")?)?;

let client = ReqwestPdsClient::with_service_url(&session.pds_url);
let prover = P256DpopProver::from_pkcs8_pem(&pkcs8_pem)?;
let writer = SigningPdsWriter::new(
    client,
    session.access_jwt.clone(),
    prover,
    session.dpop_nonce.clone(),
);
let publisher = RecordPublisher::new(writer, session.did.clone());

let resp = publisher.create(&vocab).await?;
```

`pkcs8_pem` is converted from the session's
`dpop_private_key_jwk` via an external JWK-to-PKCS8 helper. Request
PDS-side lexicon validation when the deployment supports it, but keep
the local typed and graph checks: PDS validation does not run
`VocabGraph::validate`. See [Configure OAuth sessions](./oauth.md).

## Use the published vocab

Once the vocabulary is on the network, an open-enum field may point to
it through the sibling `*Vocab` field. Rust consumers see one of two
enum forms:

- A generated known variant when the slug appears in the lexicon's
  `knownValues` list.
- `Other(String)` for every other wire slug, whether or not a
  referenced vocabulary declares it.

The `is_subsumed_by`, `satisfies`, and `translate_to` helpers query a
loaded `VocabGraph` or `VocabRegistry`; deserializing the enum does not
fetch the referenced record. See
[The vocabulary knowledge graph](../concepts/vocab-graph.md) for
the semantics.
