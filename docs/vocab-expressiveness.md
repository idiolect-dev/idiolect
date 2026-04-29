# Expressive power of `dev.idiolect.vocab`

This document characterises what an idiolect vocabulary can and cannot represent. Use it to decide whether a domain model fits the vocab's expressive ceiling, or whether a richer formalism is warranted.

## TL;DR

**RDFS + full OWL Lite + full SKOS Core**, with two distinctive features: per-relation world discipline, and federated authoring as the substrate. Plenty of expressive room for thesauri, taxonomies, controlled vocabularies, lightweight ontologies, and cross-system identifier alignment. Not the right tool for axiomatic reasoning beyond OWL Lite or rich datatype modelling.

## What the vocab expresses

### Ontological primitives

- **Typed nodes.** Every node carries an `id`, an open-enum `kind` (canonical values: `concept`, `relation`, `instance`, `type`; communities extend with their own), and optional `subkindUri` pointing at another node. Plus optional `label`, `alternateLabels`, `description`, `externalIds`, `status`, `deprecatedBy`.
- **Typed directed edges** keyed by relation slug, between nodes within the same vocabulary or across vocabularies via AT-URI. Edges are first-class records of the form `{source, target, relationSlug}`.
- **Status and supersession.** Nodes carry a lifecycle status (`proposed` / `provisional` / `established` / `deprecated`); deprecated nodes optionally point at successors via `deprecatedBy`. The vocab record itself can declare `supersedes` to point at the previous revision.

### Relation algebra (full OWL Lite property characteristics)

A relation is itself a node (`kind = "relation"`) carrying `relationMetadata`. Eight first-class flags cover every OWL Lite property characteristic:

- `symmetric`: `A R B` implies `B R A`. Walks traverse outbound and inbound edges as one set.
- `asymmetric`: `A R B` implies NOT (`B R A`). Declarative; mutually exclusive with `symmetric` (the `RelationProperties::contradictions` method flags the violation).
- `transitive`: `A R B` and `B R C` imply `A R C`. Walks compute the transitive closure (the default `walk_relation` behaviour).
- `reflexive`: `A R A` for every `A`. Reflected in the `reflexive` argument of `walk_relation`.
- `irreflexive`: NOT (`A R A`) for any `A`. Declarative; mutually exclusive with `reflexive`.
- `functional`: each `A` has at most one `B` with `A R B`.
- `inverseFunctional`: each `B` has at most one `A` with `A R B`. Useful for identifier-like relations (e.g. `has_isbn`).
- `inverseOf`: names the inverse relation. Edges in this relation imply mirror-inverse edges in the named relation.
- `world`: per-relation override of the vocab-level `world` discipline (open / closed-with-default / hierarchy-closed).

### Edge metadata

Each edge optionally carries:

- `weight`: integer 0–1000 (scaled 0.0–1.0)
- `metadata.confidence`: integer 0–1000
- `metadata.startDate` / `metadata.endDate`: temporal validity bounds
- `metadata.source`: free-form attestation

### Cross-system grounding

`externalId` mappings ground a vocab node to identifiers in non-ATProto knowledge bases. Known systems include Wikidata, ROR, ORCID, ISNI, VIAF, LCSH, FAST, SKOS, Dublin Core, Schema.org, MeSH, and AAT (open-extensible). Each external mapping carries a SKOS-style `matchType`: `exact` / `close` / `broader` / `narrower` / `related`.

### Composition and federation

- Vocabularies are themselves ATProto records under `dev.idiolect.vocab`. Authoring, versioning, and federation come from the substrate.
- Cross-vocab references work via AT-URI to the target vocabulary, plus the node's id within that vocabulary.
- `equivalent_to` edges plus the `map_enum` lens combinator enable decentralised cross-vocab translation: one community publishes a bridge vocab whose edges import another community's slugs into its own dialect, no central registry required.

### SKOS-aligned node attributes

Every node carries the full SKOS Core annotation set:

- `label` (SKOS `prefLabel`)
- `alternateLabels` (SKOS `altLabel`)
- `hiddenLabels` (SKOS `hiddenLabel`) — searchable but not displayed
- `description` (SKOS `definition`) — a complete explanation
- `scopeNote` (SKOS `scopeNote`) — application guidance distinct from the definition
- `example` (SKOS `example`)
- `historyNote`, `editorialNote`, `changeNote` (SKOS annotation properties)
- `notation` (SKOS `notation`) — non-text classification codes (Dewey, ISO, etc.)
- `kind = "collection"` plus `member_of` edges express SKOS `Collection`

## Equivalence with established standards

| Standard | Coverage |
|---|---|
| **RDFS** | Full. Triples ↔ edges. `rdfs:subClassOf` ↔ `subsumed_by`. `rdfs:label`, `rdfs:comment` ↔ `label` / `description`. |
| **OWL Lite** | Full. All eight property characteristics (`symmetric`, `asymmetric`, `transitive`, `reflexive`, `irreflexive`, `functional`, `inverseFunctional`, `inverseOf`), plus `equivalentClass` / `equivalentProperty` via `equivalent_to` edges between concept- or relation-kind nodes. Property characteristics are validated for internal consistency (`symmetric+asymmetric` and `reflexive+irreflexive` are flagged). Missing: role chains, class restrictions (those are OWL DL territory, deliberately out of scope). |
| **SKOS Core** | Full. Concept (`kind="concept"`), ConceptScheme (= Vocab record), Collection (`kind="collection"` + `member_of` edges), `prefLabel` / `altLabel` / `hiddenLabel`, `broader` / `narrower` / `related`, the `*Match` family via `externalId.matchType`, `definition` / `scopeNote` / `example` / `historyNote` / `editorialNote` / `changeNote`, `notation`, `topConceptOf` (via `top` field). |
| **Property graph (Cypher / Neo4j)** | Partial. Typed nodes plus typed edges plus edge metadata, but property bags on nodes are fixed-schema (the SKOS-aligned set above), not arbitrary key/value. |

## What it deliberately does not express

- **Class restrictions** (`owl:Restriction`, `someValuesFrom`, `allValuesFrom`, `hasValue`, cardinality bounds). OWL DL territory, deliberately out of scope; record-shape validation belongs at the lexicon level.
- **Role chains** (`owl:propertyChainAxiom`). OWL 2 territory; not OWL Lite.
- **Class disjointness** (`owl:disjointWith`). The `polar_opposite_of` relation is informal; consumers do not get automatic inconsistency detection.
- **Property restrictions** of the form "the `purpose` field of an encounter must be from this vocab." That's the consumer's job to enforce.
- **N-ary relations.** Only binary edges. Ternary or higher relations need a reified-relation node.
- **Datatype properties beyond `externalIds`.** No `xsd:date` / `xsd:int` slots on nodes; the only structured pointer is `externalIds`.
- **Anonymous / blank nodes.** Every node has a stable id.
- **Built-in inference or reasoner.** Consumers walk the relations themselves through the `VocabGraph` API.
- **Ordered Collections** (SKOS `OrderedCollection`). Collection membership is unordered; consumers needing order put `displayOrder` in `editorialNote` or layer ordering on top.

## The genuinely new bit

### Per-relation world discipline

Standard ontology languages pick one stance: OWL is open-world by default, most rule systems are closed-world. The vocab lets each relation pick its world independently. `subsumed_by` can be `closed-with-default` (every undeclared id rolls up to `top`) while `equivalent_to` is `open` (undeclared equivalences are simply unknown, not refuted) and `polar_opposite_of` is `hierarchy-closed` (only the declared edges count). All within one vocabulary.

The point: communities tune what *undeclared* means per relation, rather than committing to a global stance.

### Federated authoring on ATProto

Because vocabularies are ATProto records, the substrate handles distribution. Communities author their own vocabs under their own DIDs; no central registry mediates. Cross-vocab translation runs entirely through declarative edges plus the `map_enum` lens combinator.

## Runtime API

Three types in `idiolect_records::vocab` cover every traversal and matching need:

- **`VocabGraph`** — normalized read-only view over a single `Vocab` record. Lifts both legacy tree shape (`actions` + `parents`) and graph shape (`nodes` + `edges`) into a uniform set of indexed maps.
- **`VocabRegistry`** — long-lived cache of `(at-uri → VocabGraph)`. Consumers (orchestrators, observers, lens migrators) register vocabs once at catalog ingest and run subsumption / satisfaction / translation queries in O(walk).
- **`RelationProperties`** — algebraic profile of a relation, with `contradictions()` for OWL Lite consistency validation.

The three core query verbs:

```rust
// Hierarchy: legacy subsumption.
registry.is_subsumed_by(vocab_uri, "moderator", "member");          // Some(true)

// Generalised: any relation, any direction.
registry.satisfies(verifs_uri, "formal-proof",
                   "stronger_than", "property-test");               // Some(true)

// Cross-vocab: equivalence walking.
registry.translate(blacksky_vocab, idiolect_vocab, "agree");        // Some("agree")
```

## Computational profile

All operations route through `VocabGraph::walk_relation`: deduplicated traversal over typed edges, polynomial in graph size, terminates on cycles via a `seen` set. No undecidable constructs. Subsumption queries, OWL-Lite-style satisfaction checks, and SKOS-style mappings all reduce to the same primitive with different relation slugs.

| Operation | Complexity |
|---|---|
| Build `VocabGraph` from `Vocab` | O(\|actions\| + \|nodes\| + \|edges\|) |
| `walk_relation(source, relation, reflexive)` | O(\|reachable subgraph\|) |
| `is_subsumed_by(specific, general)` | O(\|ancestors(specific)\|) |
| `equivalent_in(source, other)` (last-pass fallback) | O(\|other.nodes\| × \|equivalent_to closure\|) |
| `top()` | O(\|nodes\|) |

## Practical ceiling

### Express comfortably

- Thesauri, taxonomies, controlled vocabularies (SKOS-style)
- Role and permission hierarchies (community roles, ACL roles)
- Action lattices and purpose hierarchies (the original ThUse use case)
- Lifecycle state machines (statuses, deliberation outcomes)
- Vote-stance mappings (Polis-style, plus richer)
- Governance categories
- Cross-system identifier alignments (Wikidata + local + SKOS bridges)
- Sandbox-isolation and verification-strength lattices (the seed examples)

### Express awkwardly

- Rich logical theories with axiomatic constraints
- Domain models with deep cardinality requirements
- Reified n-ary relations (requires hand-rolling intermediate nodes)
- Property-graph workloads with arbitrary key/value bags on nodes

### Don't express

- Anything requiring a reasoner. Pair with a separate inference engine if you need description-logic entailment, SPARQL property paths, or rule-based forward chaining.
- Quantitative datatype models. The vocab carries identifiers and labels; the data on either side belongs in records that *reference* the vocab.

## Where this fits

The vocab is a substrate for *mutual intelligibility* between communities — what idiolect calls a dialect's "vocabulary surface." It is enough power to subsume what most communities mean by "our shared vocabulary" (a controlled set of concepts plus a few typed relations), while staying small enough that traversal is tractable and the substrate stays decentralised. When that ceiling becomes binding, the right move is to layer a richer formalism *on top* and translate through `equivalent_to` edges, not to push the vocab schema into OWL DL territory.

## See also

- Lexicon: [`lexicons/dev/idiolect/vocab.json`](../lexicons/dev/idiolect/vocab.json)
- Runtime API: `idiolect_records::vocab::VocabGraph` ([source](../crates/idiolect-records/src/vocab.rs))
- Cross-vocab translation: `idiolect_lens::map_enum` ([source](../crates/idiolect-lens/src/map_enum.rs))
- Seed reference vocabularies: [`lexicons/dev/idiolect/examples/vocab/`](../lexicons/dev/idiolect/examples/vocab/)
- Acorn bridge demonstration: [`examples/idiolect-acorn/`](../examples/idiolect-acorn/)
