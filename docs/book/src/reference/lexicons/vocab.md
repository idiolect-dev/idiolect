# dev.idiolect.vocab

A community-published vocabulary. Two compatible shapes are
supported:

1. The legacy single-relation **tree** (`actions` + `top` + `world`),
   where every entry declares its direct subsumers and the world
   pins the inference discipline.
2. The typed multi-relation **knowledge graph** (`nodes` + `edges`),
   where every node is first-class and edges carry a relation slug.

Authors choose either. Consumers normalize the legacy tree to the
graph form at access time by lifting each `actionEntry` to a
node and each `parents` entry to a `subsumed_by` edge. New
vocabularies should prefer the graph shape; the tree stays valid
for backward compatibility and remains the right shape for pure
subsumption hierarchies.

The graph shape is modelled after `pub.chive.graph.{node,edge}`,
so cross-vocabulary translation, SKOS-style
`broader_than`/`narrower_than` mappings, and external-id
alignment (Wikidata, ROR, ORCID, ...) are first-class.

> **Source:** [`lexicons/dev/idiolect/vocab.json`](https://github.com/idiolect-dev/idiolect/blob/main/lexicons/dev/idiolect/vocab.json)
> · **Rust:** [`idiolect_records::Vocab`](https://docs.rs/idiolect-records/latest/idiolect_records/struct.Vocab.html)
> · **TS:** `@idiolect-dev/schema/vocab`
> · **Fixture:** `idiolect_records::examples::vocab`

## Top-level shape

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `name` | string (≤128) | yes | Human-readable name. |
| `description` | string (≤4000 graphemes) | no | Narrative description. |
| `world` | enum | yes | `closed-with-default` / `open` / `hierarchy-closed`. Default subsumption discipline. |
| `defaultRelation` | at-uri | no | Pointer to the relation-kind node consumers should treat as canonical when no relation is specified. |
| `top` | string (≤256) | no | Identifier of the vocabulary's top action under `subsumed_by`. Required when `actions` is populated and `world=closed-with-default`. |
| `actions` | array (≤4096) of `actionEntry` | no | Legacy tree shape. |
| `nodes` | array (≤4096) of `vocabNode` | no | Graph shape. |
| `edges` | array (≤16384) of `vocabEdge` | no | Graph shape. |
| `supersedes` | at-uri | no | Prior vocabulary this one replaces. |
| `occurredAt` | datetime | yes | Publication timestamp. |

## The `world` discipline

A closed-enum field naming how undeclared identifiers are treated:

| Slug | Behavior |
| --- | --- |
| `closed-with-default` | Every undeclared id is subsumed by `top` and nothing else. The natural default for hierarchical taxonomies. |
| `open` | Undeclared ids are incomparable to every declared id. Right when the vocab is one community's view of an open-ended space. |
| `hierarchy-closed` | Only the declared edges exist. Strictest; appropriate when the vocabulary is a closed enumeration. |

The slug is *closed enum*: it is meta-policy on the inference
engine, not a domain term, so extending it would change runtime
semantics. Per-relation overrides live on the relation node's
metadata.

## The `actionEntry` (legacy tree)

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `id` | string (≤256) | yes | Stable action identifier. |
| `parents` | array of strings | yes | Direct subsumers. Empty for `top`. |
| `class` | string (≤256) | no | Identifier of the attitudinal composition this action instances. |
| `description` | string (≤500 graphemes) | no | Optional human-readable description. |

## The `vocabNode` (graph form)

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `id` | string (≤256) | yes | Stable slug. Used as edge endpoint. |
| `kind` | open enum | no | `concept` / `relation` / `instance` / `type` / `collection`. |
| `kindVocab` | `vocabRef` | no | Vocab the kind slug resolves against. |
| `subkindUri` | at-uri | no | Pointer to another node typing this node's subkind. |
| `label` | string (≤500) | no | Primary human-readable label (SKOS `prefLabel`). |
| `alternateLabels` | array (≤50) | no | SKOS `altLabel`. |
| `hiddenLabels` | array (≤50) | no | SKOS `hiddenLabel`: searchable, not displayed. |
| `description` | string (≤2000 graphemes) | no | SKOS `definition`. |
| `scopeNote` | string (≤2000 graphemes) | no | SKOS `scopeNote`: usage guidance. |
| `example` | string (≤2000 graphemes) | no | SKOS `example`. |
| `historyNote` | string (≤2000 graphemes) | no | SKOS `historyNote`. |
| `editorialNote` | string (≤2000 graphemes) | no | SKOS `editorialNote`. |
| `changeNote` | string (≤2000 graphemes) | no | SKOS `changeNote`. |
| `notation` | string (≤500) | no | SKOS `notation`: non-text identifier. |
| `externalIds` | array (≤20) of `externalId` | no | Mappings to external knowledge bases. |
| `status` | open enum | no | `proposed` / `active` / `deprecated`. |
| `relationMetadata` | `relationMetadata` | no | OWL Lite property characteristics. Required for `kind: relation`. |

### `vocabNode.kind`

| Slug | Meaning |
| --- | --- |
| `concept` | A SKOS concept; the typical case. |
| `relation` | This node represents a relation type. Edges with this slug reference this node. |
| `instance` | An individual; a specific entity. |
| `type` | A metaclass; a type-of-types. |
| `collection` | A SKOS Collection. Members linked via `member_of` edges. |

## The `vocabEdge`

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `source` | string | yes | Source node id. |
| `target` | string | yes | Target node id. |
| `relationSlug` | string | yes | Relation slug. References a relation-kind node. |
| `metadata` | unknown | no | Edge metadata, free-form. |

## OWL Lite property characteristics

A `relation`-kind node carries metadata declaring algebraic
properties:

| Property | Meaning |
| --- | --- |
| `symmetric` | $r(a, b) \Rightarrow r(b, a)$ |
| `asymmetric` | $r(a, b) \Rightarrow \lnot r(b, a)$ |
| `transitive` | $r(a, b) \land r(b, c) \Rightarrow r(a, c)$ |
| `reflexive` | $r(a, a)$ for all $a$ in scope |
| `irreflexive` | $\lnot r(a, a)$ for all $a$ in scope |
| `functional` | $r(a, b_1) \land r(a, b_2) \Rightarrow b_1 = b_2$ |
| `inverseFunctional` | $r(a_1, b) \land r(a_2, b) \Rightarrow a_1 = a_2$ |
| `inverseOf` | Pointer to the inverse relation node. |
| `world` | Per-relation override of the vocabulary-level `world`. |

Contradictions (`symmetric+asymmetric`, `reflexive+irreflexive`)
are caught at validation time. Functional / inverse-functional
violations are caught at edge-walk time.
[`VocabGraph::validate`](../crates/idiolect-records.md) walks
the asserted edges and emits one violation per inconsistency.

## SKOS Core annotations

The full annotation set on every concept node:

| Field | SKOS counterpart |
| --- | --- |
| `label` | `prefLabel` |
| `alternateLabels` | `altLabel` |
| `hiddenLabels` | `hiddenLabel` |
| `description` | `definition` |
| `scopeNote` | `scopeNote` |
| `example` | `example` |
| `historyNote` | `historyNote` |
| `editorialNote` | `editorialNote` |
| `changeNote` | `changeNote` |
| `notation` | `notation` |
| `externalIds[]` | `exactMatch` / `closeMatch` / `broadMatch` / `narrowMatch` / `relatedMatch` |

## External-id mappings

Each `externalId` carries:

| Subfield | Type | Notes |
| --- | --- | --- |
| `system` | string | Knowledge-base identifier (`wikidata`, `ror`, `orcid`, `lcsh`, ...). |
| `id` | string | Identifier in that system. |
| `match` | enum | `exact` / `close` / `broader` / `narrower` / `related`. |

External ids enable cross-system translation. A consumer holding
two vocabs with `wikidata:Q4116214` external ids can match the
nodes across the vocabs even when the slugs differ.

## Backward compatibility

The lifting from tree to graph form is mechanical:

| Tree element | Graph element |
| --- | --- |
| `actionEntry` | `vocabNode { id, kind: "concept" }` |
| Each parent in `actionEntry.parents` | `vocabEdge { source = entry.id, target = parent, relationSlug: "subsumed_by" }` |
| `actionEntry.class` (when present) | `vocabEdge { source = entry.id, target = class, relationSlug: "instance_of" }` |
| `top` | The unique node with no outbound `subsumed_by` edge. |

A vocab record with both `actions` and `nodes`/`edges` populated
is interpreted as the union after lifting. This is the
transition shape; new vocabularies should prefer pure graph
form.

## Example (graph form)

```json
{
  "$type": "dev.idiolect.vocab",
  "name": "vote-stances",
  "description": "Default deliberation vote stances.",
  "world": "open",
  "nodes": [
    { "id": "agree",    "kind": "concept", "label": "Agree" },
    { "id": "disagree", "kind": "concept", "label": "Disagree" },
    { "id": "pass",     "kind": "concept", "label": "Pass" },
    { "id": "polar_opposite_of",
      "kind": "relation",
      "label": "Polar opposite of",
      "relationMetadata": { "symmetric": true, "irreflexive": true }
    }
  ],
  "edges": [
    { "source": "agree",
      "target": "disagree",
      "relationSlug": "polar_opposite_of" }
  ],
  "occurredAt": "2026-04-19T00:00:00.000Z"
}
```

## Concept references

- [Concepts: Open enums and vocabularies](../../concepts/open-enums.md)
- [Concepts: The vocabulary knowledge graph](../../concepts/vocab-graph.md)
- [Guides: Author a community vocabulary](../../guide/vocabulary.md)
- [Crate reference: idiolect-records (`VocabGraph`, `VocabRegistry`)](../crates/idiolect-records.md)
