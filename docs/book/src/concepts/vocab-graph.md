# The vocabulary knowledge graph

`dev.idiolect.vocab` is a typed multi-relation knowledge graph.
A vocabulary record carries:

- **Nodes** with a `kind` discriminator (`concept`, `relation`,
  `instance`, `type`, `collection`).
- **Edges** $(s, t, r)$ where $s$ and $t$ are node ids and $r$ is
  a relation slug.
- **Relation metadata** declaring algebraic properties of $r$.
- **SKOS Core annotations** on every node.
- **External-id mappings** for cross-system grounding.

The shape is modelled on `pub.chive.graph.{node,edge}`.

## Why a graph, not a tree

Earlier idiolect releases shipped a single-relation tree (the
`subsumed_by` parent pointer). That covers the canonical
hierarchical case (a `docker-run` is a `subprocess`) but not:

- Multiple relations on the same nodes (`equivalent_to`,
  `polar_opposite_of`, `instance_of`, `member_of`, ...).
- Cross-vocabulary translation. A node in one vocab and a node in
  another can be linked by `equivalent_to` so consumers can
  follow the bridge.
- Algebraic properties beyond reflexivity (`symmetric`,
  `transitive`, `inverse_of`, ...). These are the OWL Lite
  predicates.

The graph form generalises the tree: `subsumed_by` is just one
relation among many.

## Node kinds

| Kind | What it represents |
| --- | --- |
| `concept` | A SKOS concept; the typical case. |
| `relation` | A relation type. The node carries the relation's metadata; edges of that kind reference this node by slug. |
| `instance` | A specific entity (a `did`, an `at-uri`, a real-world identifier). |
| `type` | A type-of-types; rare but useful for meta-vocabularies. |
| `collection` | A SKOS Collection. Members are linked by `member_of` edges. |

## OWL-Lite-style property characteristics

A `relation`-kind node carries metadata declaring the relation's
algebraic properties. The shipped set follows the OWL Lite
terminology adopted by the project; strictly, it spans both
OWL Lite (`symmetric`, `transitive`, `functional`,
`inverseFunctional`, `inverseOf`) and OWL 2 additions
(`asymmetric`, `reflexive`, `irreflexive`).

| Property | Meaning |
| --- | --- |
| `symmetric` | $r(a, b) \Rightarrow r(b, a)$ |
| `asymmetric` | $r(a, b) \Rightarrow \lnot r(b, a)$ |
| `transitive` | $r(a, b) \land r(b, c) \Rightarrow r(a, c)$ |
| `reflexive` | $r(a, a)$ for all $a$ in scope |
| `irreflexive` | $\lnot r(a, a)$ for all $a$ in scope |
| `functional` | $r(a, b_1) \land r(a, b_2) \Rightarrow b_1 = b_2$ |
| `inverseFunctional` | $r(a_1, b) \land r(a_2, b) \Rightarrow a_1 = a_2$ |
| `inverseOf` | A pointer to the inverse relation node. |
| `world` (per-relation override) | Open-enum policy for this relation only. |

Contradictions (`symmetric+asymmetric`, `reflexive+irreflexive`)
are caught at validation time; functional / inverse-functional
violations are caught at edge-walk time. The `VocabGraph::validate`
walker emits one
[`VocabViolation`](../reference/crates/idiolect-records.md) per
inconsistency.

## SKOS Core annotations

Every concept-kind node accepts the SKOS Core annotation set:

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

The annotations do not affect runtime resolution (the slug is the
key); they are surface-area for human authors and downstream
display tooling.

## Runtime queries

`VocabGraph` is a normalised read-only view over a single vocab.
The shipped methods on it:

- `walk_relation(source, relation, reflexive) -> Vec<String>` —
  the canonical traversal primitive.
- `subsumed_by(source) -> Vec<String>` — the transitive +
  reflexive `subsumed_by` walk from `source`.
- `is_subsumed_by(specific, general) -> bool` — the legacy
  hierarchical query.
- `direct_targets(source, relation) -> &[String]` and
  `direct_sources(target, relation) -> &[String]` — one-hop
  neighbours.
- `equivalent_in(source, other) -> Option<String>` — find the
  equivalent slug in another graph via `equivalent_to` edges.
- `top()` / `top_with(explicit)` — the vocab's top node under
  `subsumed_by`.
- `relation_properties(relation) -> RelationProperties` — the
  OWL Lite metadata for the named relation.
- `validate() -> Vec<VocabViolation>` — the OWL Lite consistency
  walker.

`VocabRegistry` caches multiple graphs by AT-URI for
cross-vocabulary work. It exposes:

- `insert(uri, &vocab)` / `get(uri)` / `len()` / `is_empty()` —
  registry plumbing.
- `is_subsumed_by(uri, specific, general) -> Option<bool>` and
  `satisfies(uri, x, relation, y) -> Option<bool>` — query a
  named vocab. `None` means the registry has no entry for that
  URI.
- `translate(from_uri, to_uri, slug) -> Option<String>` — walk
  `equivalent_to` edges to translate a slug across vocabs.
- `validate() -> BTreeMap<String, Vec<VocabViolation>>` — batch
  validate every registered vocab.

The exact signatures and return types are on docs.rs at
[`idiolect_records::VocabGraph`](https://docs.rs/idiolect-records/latest/idiolect_records/struct.VocabGraph.html).

## Cross-vocab translation

Two vocabs that publish `equivalent_to` edges between their nodes
form an undirected bridge. A consumer holding both records and a
`VocabRegistry` can ask:

```rust
let translated: Option<String> = registry.translate(
    &from_uri,
    &to_uri,
    "agree", // a slug in the from-vocab
);
```

If the bridge exists, the consumer gets the equivalent slug in
the target vocab. If it does not, `None`. This is the engine for
[Open enums](./open-enums.md)' `translate_to` helper.

## Versioning

A vocab edit is a record edit (a new `dev.idiolect.vocab` record)
under a new rkey, with the old vocab listed as `supersedes`. The
new record is a separate at-uri; consumers depending on the old
slug set continue to resolve through the old vocab until they
update their references.

The lexicon-evolution policy classifies vocab edits the same way
it classifies schema edits:

- Adding a node or an edge — Iso (open enums tolerate).
- Renaming a node — Iso via `RenameEdgeName`.
- Removing a node — Projection over consumers using that node.
- Adding `equivalent_to` between two vocabs — derives a free lens
  available through the orchestrator's `mapEnum` cache.

The full classification is in
[Lexicon evolution policy](./lexicon-evolution.md).
