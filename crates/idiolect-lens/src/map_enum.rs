//! `mapEnum`: cross-vocabulary enum-slug translation.
//!
//! Built atop [`idiolect_records::vocab::VocabGraph`], `map_enum`
//! takes a slug authored against one vocabulary and translates it to
//! the closest matching slug in another by following `equivalent_to`
//! edges. This is the engine that bridges, e.g., a Blacksky vote
//! stance vocabulary to idiolect's canonical `vote-stances-v1` so
//! the same vote record can be read under either dialect.
//!
//! The lens is intentionally thin: it does not know about specific
//! lexicons or fields. Authors point it at two `Vocab` records and a
//! slug; consumers wire it into wherever an open-enum field needs to
//! be re-anchored.

use idiolect_records::Vocab;
use idiolect_records::vocab::VocabGraph;

/// Translate `slug` from `from` to a slug `to` knows. Returns the
/// matched slug when:
///
/// - `to` already contains a node with the same id as `slug`, or
/// - some node reachable from `slug` via `equivalent_to` edges in
///   `from` is also a node in `to`.
///
/// Returns `None` when no translation can be found. Callers fall
/// back to passing the slug through verbatim under an open-enum
/// field, which is wire-compatible with the canonical default.
///
/// # Examples
///
/// ```ignore
/// // Pseudo-code; the real example needs full Vocab construction.
/// let acorn_vote = blacksky_vote_stances_vocab();
/// let idiolect_vote = idiolect_vote_stances_vocab();
/// // Acorn votes for `agree`; idiolect calls it `agree` too;
/// // map_enum returns Some("agree".into()).
/// assert_eq!(
///     map_enum(&acorn_vote, &idiolect_vote, "agree"),
///     Some("agree".to_owned()),
/// );
/// ```
#[must_use]
pub fn map_enum(from: &Vocab, to: &Vocab, slug: &str) -> Option<String> {
    let g_from = VocabGraph::from_vocab(from);
    let g_to = VocabGraph::from_vocab(to);
    g_from.equivalent_in(slug, &g_to)
}

/// Translate `slug` using already-built [`VocabGraph`] views. Useful
/// when the caller is translating many slugs across the same vocab
/// pair: building the graphs once amortizes the lift-from-tree cost.
#[must_use]
pub fn map_enum_graphs(from: &VocabGraph, to: &VocabGraph, slug: &str) -> Option<String> {
    from.equivalent_in(slug, to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use idiolect_records::Datetime;
    use idiolect_records::generated::dev::idiolect::vocab::{
        ActionEntry, VocabEdge, VocabEdgeRelationSlug, VocabNode, VocabNodeKind, VocabWorld,
    };

    fn vocab_with_nodes(name: &str, ids: &[&str]) -> Vocab {
        Vocab {
            name: name.to_owned(),
            description: None,
            world: VocabWorld::Open,
            top: None,
            actions: None,
            supersedes: None,
            occurred_at: Datetime::parse("2026-04-28T00:00:00Z").expect("valid datetime"),
            default_relation: None,
            edges: None,
            nodes: Some(
                ids.iter()
                    .map(|id| VocabNode {
                        id: (*id).to_owned(),
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
                    })
                    .collect(),
            ),
        }
    }

    fn vocab_with_legacy(name: &str, action_ids: &[&str]) -> Vocab {
        Vocab {
            name: name.to_owned(),
            description: None,
            world: VocabWorld::Open,
            top: None,
            actions: Some(
                action_ids
                    .iter()
                    .map(|id| ActionEntry {
                        id: (*id).to_owned(),
                        parents: vec![],
                        class: None,
                        description: None,
                    })
                    .collect(),
            ),
            supersedes: None,
            occurred_at: Datetime::parse("2026-04-28T00:00:00Z").expect("valid datetime"),
            default_relation: None,
            edges: None,
            nodes: None,
        }
    }

    #[test]
    fn matching_slug_translates_directly() {
        let v1 = vocab_with_nodes("a", &["agree", "disagree"]);
        let v2 = vocab_with_nodes("b", &["agree", "disagree", "pass"]);
        assert_eq!(map_enum(&v1, &v2, "agree"), Some("agree".to_owned()));
    }

    #[test]
    fn missing_slug_returns_none() {
        let v1 = vocab_with_nodes("a", &["foo"]);
        let v2 = vocab_with_nodes("b", &["bar"]);
        assert_eq!(map_enum(&v1, &v2, "foo"), None);
    }

    #[test]
    fn equivalent_to_edge_bridges_renames() {
        // v1 has `endorse` with an equivalent_to edge to `agree`;
        // v2 has only `agree`. map_enum walks the edge.
        let v1 = Vocab {
            name: "a".to_owned(),
            description: None,
            world: VocabWorld::Open,
            top: None,
            actions: None,
            supersedes: None,
            occurred_at: Datetime::parse("2026-04-28T00:00:00Z").expect("valid datetime"),
            default_relation: None,
            nodes: Some(vec![
                VocabNode {
                    id: "endorse".to_owned(),
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
                    id: "agree".to_owned(),
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
            ]),
            edges: Some(vec![VocabEdge {
                source: "endorse".to_owned(),
                target: "agree".to_owned(),
                relation_slug: VocabEdgeRelationSlug::EquivalentTo,
                relation_vocab: None,
                relation_uri: None,
                weight: None,
                metadata: None,
            }]),
        };
        let v2 = vocab_with_nodes("b", &["agree"]);
        assert_eq!(map_enum(&v1, &v2, "endorse"), Some("agree".to_owned()));
    }

    #[test]
    fn legacy_tree_works_as_source() {
        let v1 = vocab_with_legacy("a", &["agree"]);
        let v2 = vocab_with_nodes("b", &["agree"]);
        assert_eq!(map_enum(&v1, &v2, "agree"), Some("agree".to_owned()));
    }
}
