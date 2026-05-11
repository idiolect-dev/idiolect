# Author a community vocabulary

Open-enum slugs across the idiolect lexicons resolve through a
`dev.idiolect.vocab` record. That record carries a typed
multi-relation knowledge graph: nodes (concept, relation, instance,
type, collection) and edges with a relation slug, plus full OWL
Lite property characteristics and SKOS Core annotations.

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

The shipped properties cover the OWL Lite set: `symmetric`,
`asymmetric`, `transitive`, `reflexive`, `irreflexive`,
`functional`, `inverseFunctional`, plus `inverseOf` and a per-
relation `world` override. The runtime walks the asserted edges
and validates them against these properties; contradictions
(`symmetric+asymmetric`, `reflexive+irreflexive`) produce a
`PropertyContradiction` violation at publish time.

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
    { "system": "wikidata", "id": "Q4116214", "match": "exact" }
  ]
}
```

The full annotation set is `label`, `alternateLabels`,
`hiddenLabels`, `description` (definition), `scopeNote`, `example`,
`historyNote`, `editorialNote`, `changeNote`, `notation`, and
`externalIds`. The match types on `externalIds` carry SKOS
semantics (`exact`, `close`, `broader`, `narrower`, `related`).

A `kind: "collection"` plus `member_of` edges expresses a SKOS
Collection.

## Validate

```bash
schema check vocab.json
```

The check runs panproto's structural validation plus the OWL Lite
consistency walker. Violations are listed with the offending edge
or property.

For programmatic validation, construct a `Vocab` value and call
`VocabGraph::from_vocab(&vocab).validate()`; violations are
returned as a `Vec<VocabViolation>` listing each offending edge
or property.

## Publish

Same path as any other record. Construct a writer, wrap it in
`RecordPublisher`, and call `create`:

```rust
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
`dpop_private_key_jwk` via an external JWK-to-PKCS8 helper. The
PDS validates the record against the lexicon before commit;
malformed values surface as commit errors. Driving the OAuth
dance and persisting the session is the caller's job; see
[Configure OAuth sessions](./oauth.md).

## Use the published vocab

Once the vocab is on the network, any open-enum field whose
sibling `*Vocab` points at your at-uri resolves slugs through your
nodes. Consumers reading the record see one of three things:

- A known slug (matches a node id).
- An `Other(String)` value (the slug exists in your vocab but the
  reading consumer is on an older codegen run).
- An unknown value (the slug does not appear in your vocab and
  `world` is permissive enough to accept it).

The `is_subsumed_by`, `satisfies`, and `translate_to` helpers
emitted on every open-enum type use the vocab to answer
slug-relation questions without the consumer manually walking the
edges. See
[The vocabulary knowledge graph](../concepts/vocab-graph.md) for
the semantics.
