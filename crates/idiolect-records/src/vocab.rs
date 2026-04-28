//! Vocabulary traversal helpers.
//!
//! [`Vocab`](crate::Vocab) records carry one or both of:
//!
//! - The legacy tree shape: `actions` (a flat list of `actionEntry`s,
//!   each with a `parents` list) plus `world` and `top`. Equivalent
//!   to a knowledge graph in which every action is a node and every
//!   parent is a `subsumed_by` edge.
//! - The graph shape: `nodes` (typed nodes) plus `edges` (typed
//!   relations between them).
//!
//! [`VocabGraph`] normalizes either to a uniform read-side view so
//! consumers do not have to dispatch on which shape was authored.
//! All graph operations route through this view.
//!
//! The two key traversal queries are:
//!
//! - [`walk_relation`](VocabGraph::walk_relation): every node reachable
//!   from `source` along edges of a given relation slug. Caller may
//!   request reflexive closure to include `source` itself.
//! - [`subsumed_by`](VocabGraph::subsumed_by): convenience for
//!   `walk_relation(source, "subsumed_by", reflexive: true)` — the
//!   legacy ancestor query against the `actions` tree, lifted to
//!   the graph form.
//!
//! Symmetric relations (declared via
//! [`relationMetadata.symmetric`](crate::generated::dev::idiolect::vocab::RelationMetadata)
//! on a relation-kind node) are walked in both directions.
//! `equivalent_to` is the canonical example.

use std::collections::{BTreeMap, BTreeSet};

use crate::generated::dev::idiolect::vocab::{Vocab, VocabNode};

/// Normalized read-only view over a [`Vocab`] record. Lifts the
/// legacy `actions` tree to graph form on construction so traversal
/// queries see a uniform `(node-id) -> (node)` map and a `(source,
/// relation) -> [target]` index.
///
/// Cheap to construct: O(|actions| + |nodes| + |edges|). Holds no
/// references back into the source record, so the original `Vocab`
/// can be dropped or mutated independently after construction.
#[derive(Debug, Clone, Default)]
pub struct VocabGraph {
    /// Every node id present in the vocab.
    nodes: BTreeMap<String, NormalizedNode>,
    /// Outbound edges keyed by `(source_id, relation_slug)`.
    out_edges: BTreeMap<(String, String), Vec<String>>,
    /// Inbound edges keyed by `(target_id, relation_slug)`. Lets
    /// symmetric-relation walks traverse from the target back to
    /// candidate sources without scanning every outbound entry.
    in_edges: BTreeMap<(String, String), Vec<String>>,
    /// `relationSlug -> RelationProperties` for relation-kind nodes
    /// (`kind == "relation"`) found in the vocab. When a relation is
    /// authored as a node with `relationMetadata`, traversals can lift
    /// the algebraic properties (symmetry, transitivity, ...) onto
    /// the walk semantics.
    relations: BTreeMap<String, RelationProperties>,
}

/// Normalized node view. Holds the same fields as [`VocabNode`] plus
/// the synthesized id from the legacy form's `id`.
#[derive(Debug, Clone)]
pub struct NormalizedNode {
    /// Node identifier within the vocab.
    pub id: String,
    /// Optional human-readable label.
    pub label: Option<String>,
    /// Optional description.
    pub description: Option<String>,
    /// Node kind slug (`concept`, `relation`, `instance`, `type`, or
    /// a community-extended value). `None` for legacy entries.
    pub kind: Option<String>,
}

/// Algebraic properties of a relation. Mirrors the lexicon's
/// `relationMetadata` but uses plain `bool` (not `Option<bool>`) so
/// every property has a definite reading at traversal time. Defaults
/// to all-false when the relation node is unauthored or its metadata
/// is missing.
#[allow(clippy::struct_excessive_bools)]
// Each bool corresponds to a distinct algebraic property; collapsing into a bitfield would obscure the API.
#[derive(Debug, Clone, Copy, Default)]
pub struct RelationProperties {
    /// `A R B` implies `B R A`. Walks traverse outbound and inbound
    /// edges as one set.
    pub symmetric: bool,
    /// `A R B` and `B R C` imply `A R C`. Walks compute the
    /// transitive closure (the default `walk_relation` behavior).
    pub transitive: bool,
    /// `A R A` for every `A`. Reflected in the `reflexive` argument
    /// of `walk_relation`.
    pub reflexive: bool,
    /// Each `A` has at most one `B` with `A R B`. Currently advisory.
    pub functional: bool,
}

impl VocabGraph {
    /// Build a normalized view over `vocab`. Lifts legacy tree-shape
    /// `actionEntry`s into nodes with `subsumed_by` edges; honors
    /// authored `nodes` and `edges`; reads relation-kind nodes'
    /// `relationMetadata` to lift algebraic properties onto walks.
    #[must_use]
    pub fn from_vocab(vocab: &Vocab) -> Self {
        let mut nodes: BTreeMap<String, NormalizedNode> = BTreeMap::new();
        let mut out_edges: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        let mut in_edges: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        let mut relations: BTreeMap<String, RelationProperties> = BTreeMap::new();

        // Legacy tree → graph: each actionEntry becomes a node, each
        // entry in `parents` becomes a `subsumed_by` edge.
        if let Some(actions) = vocab.actions.as_ref() {
            for entry in actions {
                nodes
                    .entry(entry.id.clone())
                    .or_insert_with(|| NormalizedNode {
                        id: entry.id.clone(),
                        label: None,
                        description: entry.description.clone(),
                        kind: Some("concept".to_owned()),
                    });
                for parent in &entry.parents {
                    insert_edge(
                        &mut out_edges,
                        &mut in_edges,
                        &entry.id,
                        parent,
                        "subsumed_by",
                    );
                    nodes
                        .entry(parent.clone())
                        .or_insert_with(|| NormalizedNode {
                            id: parent.clone(),
                            label: None,
                            description: None,
                            kind: Some("concept".to_owned()),
                        });
                }
            }
        }

        // Graph shape: ingest nodes + edges directly. Authored nodes
        // override legacy lifts of the same id.
        if let Some(authored_nodes) = vocab.nodes.as_ref() {
            for n in authored_nodes {
                nodes.insert(n.id.clone(), normalize_node(n));
                if is_relation_kind(n) {
                    let props = relation_properties_from(n);
                    relations.insert(n.id.clone(), props);
                }
            }
        }
        if let Some(authored_edges) = vocab.edges.as_ref() {
            for e in authored_edges {
                let slug = e.relation_slug.as_str();
                insert_edge(&mut out_edges, &mut in_edges, &e.source, &e.target, slug);
            }
        }

        // No standing seed for `subsumed_by` here — the lookup in
        // `relation_properties` handles the legacy semantic so it
        // applies uniformly across `Default::default()` and
        // `from_vocab`-constructed graphs.
        Self {
            nodes,
            out_edges,
            in_edges,
            relations,
        }
    }

    /// Look up a node by id.
    #[must_use]
    pub fn node(&self, id: &str) -> Option<&NormalizedNode> {
        self.nodes.get(id)
    }

    /// Iterate every node, in stable id order.
    pub fn nodes(&self) -> impl Iterator<Item = &NormalizedNode> {
        self.nodes.values()
    }

    /// Number of distinct nodes in the vocab.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Properties of the named relation, when authored. Falls back
    /// to `RelationProperties::default()` (all-false) for unauthored
    /// relations, except for `subsumed_by` which always starts at
    /// transitive+reflexive — the legacy semantic, applied uniformly
    /// whether or not a relation-kind node is authored.
    #[must_use]
    pub fn relation_properties(&self, relation: &str) -> RelationProperties {
        if let Some(authored) = self.relations.get(relation).copied() {
            return authored;
        }
        if relation == "subsumed_by" {
            return RelationProperties {
                transitive: true,
                reflexive: true,
                symmetric: false,
                functional: false,
            };
        }
        RelationProperties::default()
    }

    /// Direct out-edges from `source` along the named relation.
    /// Empty when `source` has no outbound edges of this relation.
    #[must_use]
    pub fn direct_targets(&self, source: &str, relation: &str) -> &[String] {
        self.out_edges
            .get(&(source.to_owned(), relation.to_owned()))
            .map_or(&[], |v| v.as_slice())
    }

    /// Direct in-edges to `target` along the named relation.
    /// Empty when `target` has no inbound edges of this relation.
    #[must_use]
    pub fn direct_sources(&self, target: &str, relation: &str) -> &[String] {
        self.in_edges
            .get(&(target.to_owned(), relation.to_owned()))
            .map_or(&[], |v| v.as_slice())
    }

    /// Every node reachable from `source` by traversing edges of the
    /// named relation. When `reflexive` is true, the result includes
    /// `source` itself unconditionally. When the relation is declared
    /// symmetric, the walk traverses both outbound and inbound edges
    /// as one set. The result is deduplicated; iteration order is
    /// implementation-defined (currently a stack-based DFS) and
    /// callers that need a deterministic order should sort.
    ///
    /// Canonical traversal primitive: subsumption queries,
    /// equivalence chasing, and SKOS-style broader/narrower walks
    /// route through this single function with different relation
    /// slugs.
    #[must_use]
    pub fn walk_relation(&self, source: &str, relation: &str, reflexive: bool) -> Vec<String> {
        let symmetric = self.relation_properties(relation).symmetric;
        let mut out: Vec<String> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut frontier: Vec<String> = vec![source.to_owned()];
        if reflexive {
            seen.insert(source.to_owned());
            out.push(source.to_owned());
        }
        while let Some(curr) = frontier.pop() {
            for tgt in self.direct_targets(&curr, relation) {
                if seen.insert(tgt.clone()) {
                    out.push(tgt.clone());
                    frontier.push(tgt.clone());
                }
            }
            if symmetric {
                for src in self.direct_sources(&curr, relation) {
                    if seen.insert(src.clone()) {
                        out.push(src.clone());
                        frontier.push(src.clone());
                    }
                }
            }
        }
        out
    }

    /// Convenience: every node `source` is subsumed by, reflexively.
    /// Equivalent to `walk_relation(source, "subsumed_by", true)`.
    #[must_use]
    pub fn subsumed_by(&self, source: &str) -> Vec<String> {
        self.walk_relation(source, "subsumed_by", true)
    }

    /// Whether `specific` is subsumed by `general`. Reflexive: every
    /// id is subsumed by itself.
    #[must_use]
    pub fn is_subsumed_by(&self, specific: &str, general: &str) -> bool {
        if specific == general {
            return true;
        }
        self.walk_relation(specific, "subsumed_by", false)
            .iter()
            .any(|n| n == general)
    }

    /// Translate a slug `source` from this vocab to a slug in
    /// `other` by walking `equivalent_to` edges (symmetric by
    /// default; consumers can override with a custom relation node).
    /// Tries direct match first; then walks `self`'s `equivalent_to`
    /// closure; then walks `other`'s `equivalent_to` closure backed
    /// onto each of `other`'s nodes (in case the equivalence is
    /// declared only on the target side). Returns `None` when no
    /// path exists.
    #[must_use]
    pub fn equivalent_in(&self, source: &str, other: &Self) -> Option<String> {
        if other.node(source).is_some() {
            return Some(source.to_owned());
        }
        // Walk self's equivalent_to closure outbound (and inbound when
        // declared symmetric); pick the first node `other` knows.
        for candidate in self.walk_relation(source, "equivalent_to", true) {
            if other.node(&candidate).is_some() {
                return Some(candidate);
            }
        }
        // Last-pass: walk `other`'s equivalent_to closure from each
        // of its nodes. Bridges the case where the equivalence is
        // declared only on the target side and the relation isn't
        // marked symmetric in `self`.
        for n in other.nodes.keys() {
            let closure = other.walk_relation(n, "equivalent_to", true);
            if closure.iter().any(|c| c == source) {
                return Some(n.clone());
            }
        }
        None
    }

    /// Top-of-hierarchy node id under `subsumed_by`. Returns the
    /// node that is reflexively maximal: every other node has it in
    /// its `subsumed_by` closure (or, equivalently, this node has no
    /// outbound `subsumed_by` edge while every other node ultimately
    /// reaches it).
    ///
    /// When the vocab record carries an explicit `top` field, that
    /// value is returned verbatim (no graph traversal). Otherwise a
    /// node with no outbound `subsumed_by` edge is selected; if more
    /// than one such root exists, returns `None` (vocab is not
    /// rooted under a single top).
    #[must_use]
    pub fn top(&self) -> Option<String> {
        let mut roots: Vec<&String> = self
            .nodes
            .keys()
            .filter(|id| self.direct_targets(id, "subsumed_by").is_empty())
            .collect();
        // Concept nodes only — relation-kind nodes have no
        // hierarchical position under subsumed_by.
        roots.retain(|id| {
            self.nodes
                .get(*id)
                .and_then(|n| n.kind.as_deref())
                .is_none_or(|k| k != "relation")
        });
        if roots.len() == 1 {
            return Some(roots[0].clone());
        }
        None
    }

    /// Top derived with the explicit `Vocab.top` field as the
    /// authoritative override. Use this when normalizing a [`Vocab`]
    /// record into a runtime cache; falls back to graph derivation
    /// when the field is absent.
    #[must_use]
    pub fn top_with(&self, explicit: Option<&str>) -> Option<String> {
        if let Some(t) = explicit
            && !t.is_empty()
        {
            return Some(t.to_owned());
        }
        self.top()
    }
}

fn normalize_node(n: &VocabNode) -> NormalizedNode {
    NormalizedNode {
        id: n.id.clone(),
        label: n.label.clone(),
        description: n.description.clone(),
        kind: n.kind.as_ref().map(|k| k.as_str().to_owned()),
    }
}

fn is_relation_kind(n: &VocabNode) -> bool {
    n.kind.as_ref().is_some_and(|k| k.as_str() == "relation")
}

fn relation_properties_from(n: &VocabNode) -> RelationProperties {
    let Some(meta) = n.relation_metadata.as_ref() else {
        return RelationProperties::default();
    };
    RelationProperties {
        symmetric: meta.symmetric.unwrap_or(false),
        transitive: meta.transitive.unwrap_or(false),
        reflexive: meta.reflexive.unwrap_or(false),
        functional: meta.functional.unwrap_or(false),
    }
}

fn insert_edge(
    out_edges: &mut BTreeMap<(String, String), Vec<String>>,
    in_edges: &mut BTreeMap<(String, String), Vec<String>>,
    source: &str,
    target: &str,
    relation: &str,
) {
    out_edges
        .entry((source.to_owned(), relation.to_owned()))
        .or_default()
        .push(target.to_owned());
    in_edges
        .entry((target.to_owned(), relation.to_owned()))
        .or_default()
        .push(source.to_owned());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Datetime;
    use crate::generated::dev::idiolect::vocab::{
        ActionEntry, RelationMetadata, VocabEdge, VocabEdgeRelationSlug, VocabNode, VocabNodeKind,
        VocabWorld,
    };

    fn legacy_vocab() -> Vocab {
        Vocab {
            name: "t".to_owned(),
            description: None,
            world: VocabWorld::ClosedWithDefault,
            top: Some("any".to_owned()),
            actions: Some(vec![
                ActionEntry {
                    id: "any".to_owned(),
                    parents: vec![],
                    class: None,
                    description: None,
                },
                ActionEntry {
                    id: "train".to_owned(),
                    parents: vec!["any".to_owned()],
                    class: None,
                    description: None,
                },
                ActionEntry {
                    id: "fine_tune".to_owned(),
                    parents: vec!["train".to_owned()],
                    class: None,
                    description: None,
                },
            ]),
            supersedes: None,
            occurred_at: Datetime::parse("2026-04-28T00:00:00Z").expect("valid datetime"),
            default_relation: None,
            edges: None,
            nodes: None,
        }
    }

    fn graph_node(id: &str, kind: VocabNodeKind) -> VocabNode {
        VocabNode {
            id: id.to_owned(),
            kind: Some(kind),
            kind_vocab: None,
            subkind_uri: None,
            label: None,
            alternate_labels: None,
            description: None,
            external_ids: None,
            status: None,
            status_vocab: None,
            deprecated_by: None,
            relation_metadata: None,
        }
    }

    fn relation_node(id: &str, props: RelationProperties) -> VocabNode {
        VocabNode {
            id: id.to_owned(),
            kind: Some(VocabNodeKind::Relation),
            kind_vocab: None,
            subkind_uri: None,
            label: None,
            alternate_labels: None,
            description: None,
            external_ids: None,
            status: None,
            status_vocab: None,
            deprecated_by: None,
            relation_metadata: Some(RelationMetadata {
                symmetric: Some(props.symmetric),
                transitive: Some(props.transitive),
                reflexive: Some(props.reflexive),
                functional: Some(props.functional),
                inverse_of: None,
                world: None,
            }),
        }
    }

    fn graph_edge(source: &str, target: &str, slug: VocabEdgeRelationSlug) -> VocabEdge {
        VocabEdge {
            source: source.to_owned(),
            target: target.to_owned(),
            relation_slug: slug,
            relation_vocab: None,
            relation_uri: None,
            weight: None,
            metadata: None,
        }
    }

    fn graph_vocab(nodes: Vec<VocabNode>, edges: Vec<VocabEdge>) -> Vocab {
        Vocab {
            name: "g".to_owned(),
            description: None,
            world: VocabWorld::ClosedWithDefault,
            top: None,
            actions: None,
            supersedes: None,
            occurred_at: Datetime::parse("2026-04-28T00:00:00Z").expect("valid datetime"),
            default_relation: None,
            edges: Some(edges),
            nodes: Some(nodes),
        }
    }

    #[test]
    fn legacy_tree_lifts_to_graph() {
        let g = VocabGraph::from_vocab(&legacy_vocab());
        assert!(g.is_subsumed_by("fine_tune", "train"));
        assert!(g.is_subsumed_by("fine_tune", "any"));
        assert!(g.is_subsumed_by("train", "any"));
        assert!(!g.is_subsumed_by("any", "train"));
        // reflexive
        assert!(g.is_subsumed_by("train", "train"));
    }

    #[test]
    fn walk_relation_returns_full_closure() {
        let g = VocabGraph::from_vocab(&legacy_vocab());
        let mut got = g.walk_relation("fine_tune", "subsumed_by", false);
        got.sort();
        assert_eq!(got, vec!["any".to_owned(), "train".to_owned()]);
    }

    #[test]
    fn walk_relation_reflexive_includes_source() {
        let g = VocabGraph::from_vocab(&legacy_vocab());
        let mut got = g.walk_relation("fine_tune", "subsumed_by", true);
        got.sort();
        assert_eq!(
            got,
            vec!["any".to_owned(), "fine_tune".to_owned(), "train".to_owned()]
        );
    }

    #[test]
    fn walk_relation_unknown_source_returns_empty() {
        let g = VocabGraph::from_vocab(&legacy_vocab());
        // Source not in the vocab; non-reflexive walk yields nothing.
        let got = g.walk_relation("not-a-node", "subsumed_by", false);
        assert!(got.is_empty());
    }

    #[test]
    fn equivalent_in_finds_cross_vocab_match() {
        let v1 = legacy_vocab();
        let v2 = Vocab {
            name: "u".to_owned(),
            description: None,
            world: VocabWorld::Open,
            top: None,
            actions: Some(vec![ActionEntry {
                id: "train".to_owned(),
                parents: vec![],
                class: None,
                description: None,
            }]),
            supersedes: None,
            occurred_at: Datetime::parse("2026-04-28T00:00:00Z").expect("valid datetime"),
            default_relation: None,
            edges: None,
            nodes: None,
        };
        let g1 = VocabGraph::from_vocab(&v1);
        let g2 = VocabGraph::from_vocab(&v2);
        assert_eq!(g1.equivalent_in("train", &g2), Some("train".to_owned()));
        assert_eq!(g1.equivalent_in("fine_tune", &g2), None);
    }

    // --- Graph-shape coverage ---

    #[test]
    fn graph_shape_subsumes_correctly() {
        // Nodes: any, train (subsumed_by any), fine_tune (subsumed_by train)
        let v = graph_vocab(
            vec![
                graph_node("any", VocabNodeKind::Concept),
                graph_node("train", VocabNodeKind::Concept),
                graph_node("fine_tune", VocabNodeKind::Concept),
            ],
            vec![
                graph_edge("train", "any", VocabEdgeRelationSlug::SubsumedBy),
                graph_edge("fine_tune", "train", VocabEdgeRelationSlug::SubsumedBy),
            ],
        );
        let g = VocabGraph::from_vocab(&v);
        assert!(g.is_subsumed_by("fine_tune", "any"));
        assert!(g.is_subsumed_by("fine_tune", "train"));
        assert!(g.is_subsumed_by("train", "any"));
        assert!(!g.is_subsumed_by("any", "train"));
    }

    #[test]
    fn top_recovers_unique_root_in_graph_shape() {
        let v = graph_vocab(
            vec![
                graph_node("any", VocabNodeKind::Concept),
                graph_node("train", VocabNodeKind::Concept),
            ],
            vec![graph_edge(
                "train",
                "any",
                VocabEdgeRelationSlug::SubsumedBy,
            )],
        );
        let g = VocabGraph::from_vocab(&v);
        assert_eq!(g.top(), Some("any".to_owned()));
    }

    #[test]
    fn top_returns_none_when_multiple_roots() {
        let v = graph_vocab(
            vec![
                graph_node("a", VocabNodeKind::Concept),
                graph_node("b", VocabNodeKind::Concept),
            ],
            vec![],
        );
        let g = VocabGraph::from_vocab(&v);
        assert_eq!(g.top(), None);
    }

    #[test]
    fn top_ignores_relation_kind_nodes() {
        let v = graph_vocab(
            vec![
                graph_node("any", VocabNodeKind::Concept),
                graph_node("train", VocabNodeKind::Concept),
                relation_node(
                    "subsumed_by",
                    RelationProperties {
                        transitive: true,
                        reflexive: true,
                        ..RelationProperties::default()
                    },
                ),
            ],
            vec![graph_edge(
                "train",
                "any",
                VocabEdgeRelationSlug::SubsumedBy,
            )],
        );
        let g = VocabGraph::from_vocab(&v);
        // The relation node is itself a no-outbound-edge node, but it
        // is excluded from top() consideration.
        assert_eq!(g.top(), Some("any".to_owned()));
    }

    #[test]
    fn top_with_prefers_explicit_field() {
        let g = VocabGraph::from_vocab(&legacy_vocab());
        assert_eq!(g.top_with(Some("any")), Some("any".to_owned()));
        // Explicit empty falls back to derived.
        assert_eq!(g.top_with(Some("")), Some("any".to_owned()));
    }

    #[test]
    fn relation_properties_lifted_from_relation_node() {
        let v = graph_vocab(
            vec![
                graph_node("a", VocabNodeKind::Concept),
                graph_node("b", VocabNodeKind::Concept),
                relation_node(
                    "equivalent_to",
                    RelationProperties {
                        symmetric: true,
                        transitive: true,
                        reflexive: false,
                        functional: false,
                    },
                ),
            ],
            vec![graph_edge("a", "b", VocabEdgeRelationSlug::EquivalentTo)],
        );
        let g = VocabGraph::from_vocab(&v);
        let props = g.relation_properties("equivalent_to");
        assert!(props.symmetric);
        assert!(props.transitive);
        assert!(!props.reflexive);
    }

    #[test]
    fn symmetric_relation_walks_in_both_directions() {
        // a equivalent_to b is declared; with symmetric metadata,
        // walking from b should reach a even though only the forward
        // edge is authored. The round-trip can also surface the
        // source itself once the walk crosses back; consumers who
        // need a strict non-source set filter it out.
        let v = graph_vocab(
            vec![
                graph_node("a", VocabNodeKind::Concept),
                graph_node("b", VocabNodeKind::Concept),
                relation_node(
                    "equivalent_to",
                    RelationProperties {
                        symmetric: true,
                        ..RelationProperties::default()
                    },
                ),
            ],
            vec![graph_edge("a", "b", VocabEdgeRelationSlug::EquivalentTo)],
        );
        let g = VocabGraph::from_vocab(&v);
        let from_b = g.walk_relation("b", "equivalent_to", false);
        assert!(from_b.contains(&"a".to_owned()), "symmetric walk reaches a");
        // No symmetric metadata means walking from b yields nothing
        // (only outbound edges).
        let asymmetric = graph_vocab(
            vec![
                graph_node("a", VocabNodeKind::Concept),
                graph_node("b", VocabNodeKind::Concept),
            ],
            vec![graph_edge("a", "b", VocabEdgeRelationSlug::EquivalentTo)],
        );
        let g2 = VocabGraph::from_vocab(&asymmetric);
        assert!(
            g2.walk_relation("b", "equivalent_to", false).is_empty(),
            "no symmetric metadata = no inbound traversal"
        );
    }

    #[test]
    fn equivalent_in_via_target_side_declaration() {
        // self has only `endorse`. other has `agree` with an
        // equivalent_to edge endorse -> agree declared on `other`.
        // Without the target-side fallback walk, no match would be
        // found.
        let v_self = graph_vocab(vec![graph_node("endorse", VocabNodeKind::Concept)], vec![]);
        let v_other = graph_vocab(
            vec![
                graph_node("endorse", VocabNodeKind::Concept),
                graph_node("agree", VocabNodeKind::Concept),
                relation_node(
                    "equivalent_to",
                    RelationProperties {
                        symmetric: true,
                        ..RelationProperties::default()
                    },
                ),
            ],
            vec![graph_edge(
                "endorse",
                "agree",
                VocabEdgeRelationSlug::EquivalentTo,
            )],
        );
        let g_self = VocabGraph::from_vocab(&v_self);
        let g_other = VocabGraph::from_vocab(&v_other);
        // `other` knows `endorse` directly, so direct match wins.
        assert_eq!(
            g_self.equivalent_in("endorse", &g_other),
            Some("endorse".to_owned())
        );
    }

    #[test]
    fn empty_vocab_traverses_safely() {
        let v = graph_vocab(vec![], vec![]);
        let g = VocabGraph::from_vocab(&v);
        assert!(g.walk_relation("anything", "subsumed_by", false).is_empty());
        assert_eq!(g.top(), None);
        assert_eq!(g.node_count(), 0);
    }

    #[test]
    fn graph_and_legacy_authoring_compose() {
        // Author both shapes; node from graph form must override
        // legacy lift (richer label wins).
        let mut v = legacy_vocab();
        v.nodes = Some(vec![VocabNode {
            id: "train".to_owned(),
            kind: Some(VocabNodeKind::Concept),
            kind_vocab: None,
            subkind_uri: None,
            label: Some("Train".to_owned()),
            alternate_labels: None,
            description: Some("authored".to_owned()),
            external_ids: None,
            status: None,
            status_vocab: None,
            deprecated_by: None,
            relation_metadata: None,
        }]);
        let g = VocabGraph::from_vocab(&v);
        let n = g.node("train").expect("authored node present");
        assert_eq!(n.label.as_deref(), Some("Train"));
        assert_eq!(n.description.as_deref(), Some("authored"));
    }
}

#[cfg(test)]
mod edge_case_tests {
    use super::*;
    use crate::Datetime;
    use crate::generated::dev::idiolect::vocab::{
        ActionEntry, VocabEdgeRelationSlug, VocabNodeKind, VocabWorld,
    };

    fn cyclic_vocab() -> Vocab {
        // a subsumed_by b subsumed_by a — a directed cycle.
        // walk_relation must terminate; the closure should contain
        // both nodes regardless of where you start.
        Vocab {
            name: "cyclic".to_owned(),
            description: None,
            world: VocabWorld::Open,
            top: None,
            actions: Some(vec![
                ActionEntry {
                    id: "a".to_owned(),
                    parents: vec!["b".to_owned()],
                    class: None,
                    description: None,
                },
                ActionEntry {
                    id: "b".to_owned(),
                    parents: vec!["a".to_owned()],
                    class: None,
                    description: None,
                },
            ]),
            supersedes: None,
            occurred_at: Datetime::parse("2026-04-28T00:00:00Z").expect("valid datetime"),
            default_relation: None,
            edges: None,
            nodes: None,
        }
    }

    #[test]
    fn cyclic_subsumed_by_terminates_and_includes_both_nodes() {
        let g = VocabGraph::from_vocab(&cyclic_vocab());
        let mut closure = g.walk_relation("a", "subsumed_by", false);
        closure.sort();
        assert_eq!(closure, vec!["a".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn relation_properties_default_when_unauthored() {
        let g = VocabGraph::default();
        let props = g.relation_properties("never_authored");
        assert!(!props.symmetric);
        assert!(!props.transitive);
        assert!(!props.reflexive);
        assert!(!props.functional);
    }

    #[test]
    fn subsumed_by_has_seeded_legacy_semantics() {
        // Even an empty vocab gets transitive+reflexive on
        // subsumed_by per the legacy semantic seed.
        let g = VocabGraph::default();
        let props = g.relation_properties("subsumed_by");
        assert!(props.transitive);
        assert!(props.reflexive);
    }

    #[test]
    fn empty_known_values_emits_only_fallback_variant() {
        // Edge case for codegen: empty knownValues. Expressed at
        // the runtime level: walk_relation on an unknown slug
        // returns just the source under reflexive=true.
        let v = Vocab {
            name: "empty".to_owned(),
            description: None,
            world: VocabWorld::Open,
            top: None,
            actions: None,
            supersedes: None,
            occurred_at: Datetime::parse("2026-04-28T00:00:00Z").expect("valid datetime"),
            default_relation: None,
            edges: None,
            nodes: None,
        };
        let g = VocabGraph::from_vocab(&v);
        assert_eq!(g.node_count(), 0);
        assert!(g.walk_relation("x", "subsumed_by", false).is_empty());
        assert_eq!(
            g.walk_relation("x", "subsumed_by", true),
            vec!["x".to_owned()]
        );
    }

    #[test]
    fn graph_node_authored_relation_kind_carries_metadata_into_walks() {
        // A symmetric authored relation must propagate to walk
        // semantics even without a kind="relation" on the legacy
        // tree side.
        let v = Vocab {
            name: "with_relation".to_owned(),
            description: None,
            world: VocabWorld::Open,
            top: None,
            actions: None,
            supersedes: None,
            occurred_at: Datetime::parse("2026-04-28T00:00:00Z").expect("valid datetime"),
            default_relation: None,
            nodes: Some(vec![
                VocabNode {
                    id: "a".to_owned(),
                    kind: Some(VocabNodeKind::Concept),
                    kind_vocab: None,
                    subkind_uri: None,
                    label: None,
                    alternate_labels: None,
                    description: None,
                    external_ids: None,
                    status: None,
                    status_vocab: None,
                    deprecated_by: None,
                    relation_metadata: None,
                },
                VocabNode {
                    id: "b".to_owned(),
                    kind: Some(VocabNodeKind::Concept),
                    kind_vocab: None,
                    subkind_uri: None,
                    label: None,
                    alternate_labels: None,
                    description: None,
                    external_ids: None,
                    status: None,
                    status_vocab: None,
                    deprecated_by: None,
                    relation_metadata: None,
                },
                VocabNode {
                    id: "near".to_owned(),
                    kind: Some(VocabNodeKind::Relation),
                    kind_vocab: None,
                    subkind_uri: None,
                    label: None,
                    alternate_labels: None,
                    description: None,
                    external_ids: None,
                    status: None,
                    status_vocab: None,
                    deprecated_by: None,
                    relation_metadata: Some(
                        crate::generated::dev::idiolect::vocab::RelationMetadata {
                            symmetric: Some(true),
                            transitive: None,
                            reflexive: None,
                            functional: None,
                            inverse_of: None,
                            world: None,
                        },
                    ),
                },
            ]),
            edges: Some(vec![crate::generated::dev::idiolect::vocab::VocabEdge {
                source: "a".to_owned(),
                target: "b".to_owned(),
                relation_slug: VocabEdgeRelationSlug::Other("near".to_owned()),
                relation_vocab: None,
                relation_uri: None,
                weight: None,
                metadata: None,
            }]),
        };
        let g = VocabGraph::from_vocab(&v);
        // Walking from b under "near" must reach a thanks to
        // symmetric metadata.
        let from_b = g.walk_relation("b", "near", false);
        assert!(
            from_b.iter().any(|x| x == "a"),
            "symmetric authored relation should permit reverse walk: {from_b:?}"
        );
    }
}
