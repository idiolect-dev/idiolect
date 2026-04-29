# Expressive power of `dev.idiolect.vocab`

This document characterises what an idiolect vocabulary can and cannot represent. Use it to decide whether a domain model fits the vocab's expressive ceiling, or whether a richer formalism is warranted.

## TL;DR

Roughly **RDFS + a subset of OWL Lite + most of SKOS Core**, with two distinctive features: per-relation world discipline, and federated authoring as the substrate. Plenty of expressive room for thesauri, taxonomies, controlled vocabularies, lightweight ontologies, and cross-system identifier alignment. Not the right tool for axiomatic reasoning or rich datatype modelling.

## What the vocab expresses

### Ontological primitives

- **Typed nodes.** Every node carries an `id`, an open-enum `kind` (canonical values: `concept`, `relation`, `instance`, `type`; communities extend with their own), and optional `subkindUri` pointing at another node — the metaclass move. Plus optional `label`, `alternateLabels`, `description`, `externalIds`, `status`, `deprecatedBy`.
- **Typed directed edges** keyed by relation slug, between nodes within the same vocabulary or across vocabularies via AT-URI. Edges are first-class records of the form `{source, target, relationSlug}`.
- **Status and supersession.** Nodes carry a lifecycle status (`proposed` / `provisional` / `established` / `deprecated`); deprecated nodes optionally point at successors via `deprecatedBy`. The vocab record itself can declare `supersedes` to point at the previous revision.

### Relation algebra

A relation is itself a node (`kind = "relation"`) carrying `relationMetadata`:

- `symmetric`: `A R B` implies `B R A`. Walks traverse outbound and inbound edges as one set.
- `transitive`: `A R B` and `B R C` imply `A R C`. Walks compute the transitive closure (the default `walk_relation` behaviour).
- `reflexive`: `A R A` for every `A`. Reflected in the `reflexive` argument of `walk_relation`.
- `functional`: each `A` has at most one `B` with `A R B`. Currently advisory.
- `inverseOf`: names the inverse relation.
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

## Equivalence with established standards

| Standard | Coverage |
|---|---|
| **RDFS** | Full. Triples ↔ edges. `rdfs:subClassOf` ↔ `subsumed_by`. `rdfs:label`, `rdfs:comment` ↔ `label` / `description`. |
| **OWL Lite** | Most. We have `symmetric`, `transitive`, `reflexive`, `functional`, `inverseOf`, `equivalent_to`. Missing: `asymmetric`, `irreflexive`, `inverseFunctional`, role chains. |
| **SKOS Core** | Most. Concept, ConceptScheme (= Vocab record), `prefLabel` / `altLabel`, `broader` / `narrower` / `related`, the `*Match` family via `externalId.matchType`, `definition` / `scopeNote`. Missing: `Collection`. |
| **Property graph (Cypher / Neo4j)** | Partial. Typed nodes plus typed edges plus edge metadata, but property bags on nodes are fixed-schema (`id`, `label`, `description`, `externalIds`), not arbitrary key/value. |

## What it deliberately does not express

- **Cardinality constraints** (`exactly-one`, `someValuesFrom`, `allValuesFrom`). That is a record-validation concern, not a vocab concern.
- **Class disjointness** (`owl:disjointWith`). The `polar_opposite_of` relation is informal; consumers do not get automatic inconsistency detection.
- **Property restrictions** of the form "the `purpose` field of an encounter must be from this vocab." That's the consumer's job to enforce.
- **N-ary relations.** Only binary edges. Ternary or higher relations need a reified-relation node.
- **Datatype properties beyond `externalIds`.** No `xsd:date` / `xsd:int` slots on nodes; the only structured pointer is `externalIds`.
- **Anonymous / blank nodes.** Every node has a stable id.
- **Built-in inference or reasoner.** Consumers walk the relations themselves through the `VocabGraph` API.

## The genuinely new bit

### Per-relation world discipline

Standard ontology languages pick one stance: OWL is open-world by default, most rule systems are closed-world. The vocab lets each relation pick its world independently. `subsumed_by` can be `closed-with-default` (every undeclared id rolls up to `top`) while `equivalent_to` is `open` (undeclared equivalences are simply unknown, not refuted) and `polar_opposite_of` is `hierarchy-closed` (only the declared edges count). All within one vocabulary.

The point: communities tune what *undeclared* means per relation, rather than committing to a global stance.

### Federated authoring on ATProto

Because vocabularies are ATProto records, the substrate handles distribution. Communities author their own vocabs under their own DIDs; no central registry mediates. Cross-vocab translation runs entirely through declarative edges plus the `map_enum` lens combinator.

## Computational profile

All operations route through `VocabGraph::walk_relation`: BFS-deduplicated traversal over typed edges, polynomial in graph size, terminates on cycles via a `seen` set. No undecidable constructs. Subsumption queries, equivalence chasing, and SKOS-style mappings all reduce to the same primitive with different relation slugs.

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
