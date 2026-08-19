# The vocabulary knowledge graph

`dev.idiolect.vocab` gives open-enum slugs a graph-shaped interpretation. A
record may contain the earlier `actions`/`parents` tree, the newer
`nodes`/`edges` graph, or both. `VocabGraph::from_vocab` normalizes these forms
into one read-side representation.

## From a tree to a relation graph

In the earlier form, each action has zero or more parents. Normalization turns
each action into a concept node and each parent pointer into a `subsumed_by`
edge. If `docker-run` names `subprocess` as a parent, the normalized graph
contains:

$$
\operatorname{subsumed\_by}(\texttt{docker-run},\texttt{subprocess})
$$

The graph form generalizes this arrangement by allowing multiple named
relations over the same node set. A community can represent subsumption,
equivalence, opposition, membership, or a domain-specific relation without
adding a new field to every record that uses the vocabulary.

## Nodes and edges

A node requires only a stable `id`. Optional fields provide a kind, label,
status, external identifiers, and human-facing annotations. The known node
kinds are `concept`, `relation`, `instance`, `type`, and `collection`; the field
itself uses `knownValues`, so other kind slugs remain valid strings.

An edge is a triple $(s,r,t)$ consisting of a source id, relation slug, and
target id. Relation-kind nodes may carry `relationMetadata`. The metadata uses
property-characteristic names familiar from the
[OWL 2 structural specification](https://www.w3.org/TR/owl2-syntax/), including
`symmetric`, `asymmetric`, `transitive`, `reflexive`, `irreflexive`,
`functional`, and `inverseFunctional`.

The resemblance is deliberately limited. A `dev.idiolect.vocab` record is not
an OWL ontology, and `VocabGraph` is not an OWL reasoner. We call its supported
behavior **relation-aware reachability (RAR)**.

## What RAR computes

`walk_relation(source, relation, reflexive)` follows reachable edges for the
named relation. It always follows paths to closure, regardless of the relation's
`transitive` metadata. If the relation is marked `symmetric`, the walk follows
both outgoing and incoming edges. If the caller sets `reflexive`, the result
also contains the source.

The convenience operations are defined in terms of that walk:

- `subsumed_by(source)` uses `subsumed_by` with reflexive reachability.
- `is_subsumed_by(specific, general)` tests membership in that closure.
- `direct_targets` and `direct_sources` return one-hop neighbors.
- `equivalent_in(source, other)` searches `equivalent_to` reachability for a
  node id known to the other graph.
- `top` returns a unique non-relation root under `subsumed_by`, when one exists;
  `top_with` gives an explicit record field priority.

The current implementation stores `inverseOf` and per-relation `world` in the
generated record type but does not apply them in `VocabGraph` traversal.
Likewise, setting `reflexive` metadata does not force a walk to include its
source; the call's `reflexive` argument controls that behavior. Consumers should
not infer full OWL semantics from the serialized names.

## Validation boundary

`VocabGraph::validate` checks the constraints that can be established directly
from the authored edges and metadata:

1. functional relations have at most one target per source;
2. inverse-functional relations have at most one source per target;
3. irreflexive relations contain no self-loop;
4. asymmetric relations contain no pair of reverse edges; and
5. declarations do not combine `symmetric` with `asymmetric` or `reflexive`
   with `irreflexive`.

The method reports `VocabViolation` values. It does not reject the record during
deserialization, add missing closure edges, or establish that a label's intended
meaning is correct. That division is the **validation boundary (VB)**:
structural inconsistency is machine-checkable, while semantic trust remains
consumer policy. At the VB, a clean graph is not yet a trustworthy vocabulary.

## Human-facing annotations

The node fields `label`, `alternateLabels`, `hiddenLabels`, `description`,
`scopeNote`, `example`, `historyNote`, `editorialNote`, `changeNote`, and
`notation` are modeled after the
[SKOS reference](https://www.w3.org/TR/skos-reference/). They provide
authoring and display metadata, but the record is not serialized as RDF and
the runtime does not enforce SKOS integrity conditions.

`externalIds` adds mappings to identifiers outside ATProto with a mapping type
such as `exact`, `close`, `broader`, `narrower`, or `related`. These mappings are
stored on normalized nodes only indirectly: the current `NormalizedNode` view
does not expose `externalIds`. Callers that need them must inspect the generated
`Vocab` record.

## Multiple vocabularies

`VocabRegistry` caches normalized graphs by caller-supplied URI string. It can
test a relation in one registered graph or translate a slug between two graphs
with `equivalent_in`. Translation succeeds when the target graph already knows
the slug or when `equivalent_to` reachability produces a node id that the target
knows.

This operation is pairwise. It does not search an arbitrary network of
vocabulary records, fetch missing records, or select which publisher is
authoritative. A consumer must load the two records, validate them, and decide
that their equivalence declarations are acceptable.

## Revision and supersession

A vocabulary record may point to a predecessor with `supersedes`. ATProto still
allows a publisher to update a record at the same AT-URI, producing a new CID,
or to create a record at a new key. The Lexicon does not require one revision
strategy. Consumers that need an immutable vocabulary revision should retain a
strong reference or otherwise pin the CID rather than relying on the AT-URI
alone.

[Open enums and vocabularies](./open-enums.md) explains how generated slug types
call into these graph operations. The [vocabulary guide](../guide/vocabulary.md)
covers record authoring.
