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
//!   `walk_relation(source, "subsumed_by", reflexive: true)` —
//!   the legacy ancestor query against the `actions` tree, lifted to
//!   the graph form.

use std::collections::{BTreeMap, BTreeSet};

use crate::generated::dev::idiolect::vocab::{Vocab, VocabEdge, VocabNode};

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
    /// Every node id present in the vocab, with its synthesized or
    /// authored node body. Legacy `actionEntry`s are lifted to nodes
    /// with `kind = "concept"` and a description copied through.
    nodes: BTreeMap<String, NormalizedNode>,
    /// Edges keyed by `(source_id, relation_slug)` for O(1) outbound
    /// traversal. Multiple edges with the same source and relation
    /// stack into a `Vec` of targets.
    out_edges: BTreeMap<(String, String), Vec<String>>,
}

/// Normalized node view. Holds the same fields as [`VocabNode`] plus
/// a synthesized id from the legacy form's `id`.
#[derive(Debug, Clone)]
pub struct NormalizedNode {
    /// Node identifier within the vocab.
    pub id: String,
    /// Optional human-readable label (or `id` when unset).
    pub label: Option<String>,
    /// Optional description.
    pub description: Option<String>,
    /// Node kind slug (`concept`, `relation`, `instance`, `type`, or a
    /// community-extended value). `None` for legacy entries.
    pub kind: Option<String>,
}

impl VocabGraph {
    /// Build a normalized view over `vocab`. Lifts legacy tree-shape
    /// `actionEntry`s into nodes with `subsumed_by` edges so callers
    /// do not have to dispatch.
    #[must_use]
    pub fn from_vocab(vocab: &Vocab) -> Self {
        let mut nodes: BTreeMap<String, NormalizedNode> = BTreeMap::new();
        let mut out_edges: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();

        // Legacy tree → graph: each actionEntry becomes a node, each
        // entry in `parents` becomes a `subsumed_by` edge.
        if let Some(actions) = vocab.actions.as_ref() {
            for entry in actions {
                nodes.entry(entry.id.clone()).or_insert_with(|| NormalizedNode {
                    id: entry.id.clone(),
                    label: None,
                    description: entry.description.clone(),
                    kind: Some("concept".to_owned()),
                });
                for parent in &entry.parents {
                    out_edges
                        .entry((entry.id.clone(), "subsumed_by".to_owned()))
                        .or_default()
                        .push(parent.clone());
                    // Ensure the parent is also a node (parent may not
                    // appear as its own actionEntry — e.g., when it's
                    // declared elsewhere or implicit).
                    nodes.entry(parent.clone()).or_insert_with(|| NormalizedNode {
                        id: parent.clone(),
                        label: None,
                        description: None,
                        kind: Some("concept".to_owned()),
                    });
                }
            }
        }

        // Graph shape: ingest nodes and edges directly. Authored nodes
        // override legacy lifts of the same id (so a graph-form node
        // with a richer label wins over a legacy entry with no label).
        if let Some(authored_nodes) = vocab.nodes.as_ref() {
            for n in authored_nodes {
                nodes.insert(n.id.clone(), normalize_node(n));
            }
        }
        if let Some(authored_edges) = vocab.edges.as_ref() {
            for e in authored_edges {
                out_edges
                    .entry((e.source.clone(), normalize_relation_slug(e)))
                    .or_default()
                    .push(e.target.clone());
            }
        }

        Self { nodes, out_edges }
    }

    /// Look up a node by id.
    #[must_use]
    pub fn node(&self, id: &str) -> Option<&NormalizedNode> {
        self.nodes.get(id)
    }

    /// Iterate over every node, in stable id order.
    pub fn nodes(&self) -> impl Iterator<Item = &NormalizedNode> {
        self.nodes.values()
    }

    /// Direct out-edges from `source` along the named relation.
    /// Empty when `source` has no outbound edges of this relation.
    #[must_use]
    pub fn direct_targets(&self, source: &str, relation: &str) -> &[String] {
        self.out_edges
            .get(&(source.to_owned(), relation.to_owned()))
            .map_or(&[], |v| v.as_slice())
    }

    /// Every node reachable from `source` by traversing edges of the
    /// named relation. When `reflexive` is true, the result includes
    /// `source` itself. Result order is BFS-discovery, deduplicated.
    ///
    /// This is the canonical traversal primitive: subsumption queries,
    /// equivalence chasing, and SKOS-style broader/narrower walks all
    /// route through this single function with different relation
    /// slugs.
    #[must_use]
    pub fn walk_relation(&self, source: &str, relation: &str, reflexive: bool) -> Vec<String> {
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

    /// Translate a slug `source` from this vocab to a target slug in
    /// `other` by following `equivalent_to` edges, then resolving the
    /// match to a node `other` knows about. Returns the matched slug
    /// when present in either side's `equivalent_to` reach. Useful as
    /// the engine for [`mapEnum`](crate)-style cross-vocab translation.
    #[must_use]
    pub fn equivalent_in(&self, source: &str, other: &Self) -> Option<String> {
        if other.node(source).is_some() {
            return Some(source.to_owned());
        }
        let candidates = self.walk_relation(source, "equivalent_to", true);
        candidates.into_iter().find(|c| other.node(c).is_some())
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

fn normalize_relation_slug(e: &VocabEdge) -> String {
    e.relation_slug.as_str().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Datetime;
    use crate::generated::dev::idiolect::vocab::{ActionEntry, VocabWorld};

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
    fn equivalent_in_finds_cross_vocab_match() {
        // Two vocabs that share a node id directly.
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
}
