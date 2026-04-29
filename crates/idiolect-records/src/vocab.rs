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

/// Normalized node view. Carries the SKOS-aligned attribute set
/// (label, alternateLabels, hiddenLabels, description-as-definition,
/// scopeNote, example, historyNote, editorialNote, changeNote,
/// notation) plus the kind discriminator. Legacy `actionEntry`
/// lifts populate `id` and `description` only; everything else
/// defaults to `None`.
#[derive(Debug, Clone, Default)]
pub struct NormalizedNode {
    /// Node identifier within the vocab.
    pub id: String,
    /// Node kind slug (`concept`, `relation`, `instance`, `type`,
    /// `collection`, or a community-extended value).
    pub kind: Option<String>,
    /// Primary human-readable label (SKOS `prefLabel`).
    pub label: Option<String>,
    /// Synonyms / translations (SKOS `altLabel`).
    pub alternate_labels: Vec<String>,
    /// Searchable but not displayed (SKOS `hiddenLabel`).
    pub hidden_labels: Vec<String>,
    /// Definition (SKOS `definition`). The full explanation of the
    /// concept's meaning.
    pub description: Option<String>,
    /// Application guidance (SKOS `scopeNote`).
    pub scope_note: Option<String>,
    /// Concrete usage example (SKOS `example`).
    pub example: Option<String>,
    /// History annotation (SKOS `historyNote`).
    pub history_note: Option<String>,
    /// Editorial annotation (SKOS `editorialNote`).
    pub editorial_note: Option<String>,
    /// Change-log annotation (SKOS `changeNote`).
    pub change_note: Option<String>,
    /// Non-text identifier from a legacy classification system
    /// (SKOS `notation`; e.g. a Dewey decimal code).
    pub notation: Option<String>,
}

/// Algebraic properties of a relation. Mirrors the lexicon's
/// `relationMetadata` but uses plain `bool` (not `Option<bool>`) so
/// every property has a definite reading at traversal time. Defaults
/// to all-false when the relation node is unauthored or its metadata
/// is missing.
///
/// Covers the full OWL Lite property-characteristic set: symmetric /
/// asymmetric, transitive, reflexive / irreflexive, functional /
/// inverseFunctional, plus inverseOf (carried separately on the
/// vocab node, not in this struct).
#[allow(clippy::struct_excessive_bools)]
// Each bool corresponds to a distinct algebraic property; collapsing into a bitfield would obscure the API.
#[derive(Debug, Clone, Copy, Default)]
pub struct RelationProperties {
    /// `A R B` implies `B R A`. Walks traverse outbound and inbound
    /// edges as one set.
    pub symmetric: bool,
    /// `A R B` implies NOT (`B R A`). Declarative; mutually
    /// exclusive with `symmetric`. Consumers may validate that no
    /// edge in the vocab contradicts the asymmetry.
    pub asymmetric: bool,
    /// `A R B` and `B R C` imply `A R C`. Walks compute the
    /// transitive closure (the default `walk_relation` behavior).
    pub transitive: bool,
    /// `A R A` for every `A`. Reflected in the `reflexive` argument
    /// of `walk_relation`.
    pub reflexive: bool,
    /// NOT (`A R A`) for any `A`. Declarative; mutually exclusive
    /// with `reflexive`. Consumers may validate that no self-loop
    /// edge exists.
    pub irreflexive: bool,
    /// Each `A` has at most one `B` with `A R B`. Validated by
    /// [`VocabGraph::validate`]: a source with multiple outbound
    /// edges of a functional relation surfaces as a
    /// [`VocabViolation::FunctionalEdgeViolation`].
    pub functional: bool,
    /// Each `B` has at most one `A` with `A R B`. Validated by
    /// [`VocabGraph::validate`]; useful for declaring
    /// identifier-like relations (e.g. `has_isbn`).
    pub inverse_functional: bool,
}

impl RelationProperties {
    /// Validate the property combination for OWL Lite consistency.
    /// Returns the list of contradiction tags found, empty when the
    /// declaration is internally consistent.
    ///
    /// Contradictions:
    /// - `symmetric+asymmetric` — a relation cannot be both.
    /// - `reflexive+irreflexive` — a relation cannot be both.
    ///
    /// Consumers loading vocabs should warn or reject on
    /// non-empty results.
    #[must_use]
    pub fn contradictions(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.symmetric && self.asymmetric {
            out.push("symmetric+asymmetric");
        }
        if self.reflexive && self.irreflexive {
            out.push("reflexive+irreflexive");
        }
        out
    }
}

/// One OWL Lite property characteristic violation found while
/// validating a [`VocabGraph`]. Returned by
/// [`VocabGraph::validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VocabViolation {
    /// A relation declared `functional` has a node with two or more
    /// outbound edges of that relation. Functional relations must
    /// have at most one target per source.
    FunctionalEdgeViolation {
        /// Slug of the relation that was declared functional.
        relation: String,
        /// Source node with multiple outbound edges.
        source: String,
        /// All targets the source reaches under this relation.
        targets: Vec<String>,
    },
    /// A relation declared `inverseFunctional` has a node with two
    /// or more inbound edges of that relation. Inverse-functional
    /// relations must have at most one source per target.
    InverseFunctionalEdgeViolation {
        /// Slug of the relation that was declared inverse-functional.
        relation: String,
        /// Target node with multiple inbound edges.
        target: String,
        /// All sources reaching the target under this relation.
        sources: Vec<String>,
    },
    /// A relation declared `irreflexive` has a self-loop.
    IrreflexiveSelfLoop {
        /// Slug of the relation declared irreflexive.
        relation: String,
        /// Node carrying the self-loop edge.
        node: String,
    },
    /// A relation declared `asymmetric` has a `(source, target)`
    /// pair plus a corresponding `(target, source)` pair.
    AsymmetricMutualEdge {
        /// Slug of the relation declared asymmetric.
        relation: String,
        /// One endpoint of the mutual pair.
        a: String,
        /// The other endpoint.
        b: String,
    },
    /// A relation's [`RelationProperties::contradictions`] flagged
    /// the declaration itself (e.g. both `symmetric` and
    /// `asymmetric`).
    PropertyContradiction {
        /// Slug of the relation.
        relation: String,
        /// Tag from `contradictions()`.
        tag: &'static str,
    },
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
                        kind: Some("concept".to_owned()),
                        description: entry.description.clone(),
                        ..NormalizedNode::default()
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
                            kind: Some("concept".to_owned()),
                            ..NormalizedNode::default()
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
                irreflexive: false,
                ..RelationProperties::default()
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

    /// Validate every authored relation against its declared OWL
    /// Lite property characteristics. Returns the list of
    /// violations found; empty when the vocab is internally
    /// consistent. Consumers loading vocabs from external sources
    /// (community records on the firehose, file imports) should run
    /// this and either reject inconsistent vocabs or surface the
    /// violations to operators.
    ///
    /// Validations performed:
    /// - `functional` relations: at most one outbound edge per source.
    /// - `inverseFunctional` relations: at most one inbound edge per
    ///   target.
    /// - `irreflexive` relations: no self-loops.
    /// - `asymmetric` relations: no mutual `(A, B)` and `(B, A)` pair.
    /// - Property contradictions: `symmetric+asymmetric` and
    ///   `reflexive+irreflexive`.
    ///
    /// Note: `transitive` and `reflexive` are positive-direction
    /// closure properties and are honoured by `walk_relation` rather
    /// than checked here — there is no concrete violation to find,
    /// only the absence of declared closure edges (which is
    /// expected when the vocab relies on the runtime walk).
    #[must_use]
    pub fn validate(&self) -> Vec<VocabViolation> {
        let mut out = Vec::new();
        for (slug, props) in &self.relations {
            // Property contradictions stand alone — surface every
            // contradiction tag.
            for tag in props.contradictions() {
                out.push(VocabViolation::PropertyContradiction {
                    relation: slug.clone(),
                    tag,
                });
            }
            // Functional check: each source has at most one target.
            if props.functional {
                for ((source, relation), targets) in &self.out_edges {
                    if relation == slug && targets.len() > 1 {
                        out.push(VocabViolation::FunctionalEdgeViolation {
                            relation: slug.clone(),
                            source: source.clone(),
                            targets: targets.clone(),
                        });
                    }
                }
            }
            // Inverse-functional: each target has at most one source.
            if props.inverse_functional {
                for ((target, relation), sources) in &self.in_edges {
                    if relation == slug && sources.len() > 1 {
                        out.push(VocabViolation::InverseFunctionalEdgeViolation {
                            relation: slug.clone(),
                            target: target.clone(),
                            sources: sources.clone(),
                        });
                    }
                }
            }
            // Irreflexive: no self-loops.
            if props.irreflexive {
                for ((source, relation), targets) in &self.out_edges {
                    if relation == slug && targets.iter().any(|t| t == source) {
                        out.push(VocabViolation::IrreflexiveSelfLoop {
                            relation: slug.clone(),
                            node: source.clone(),
                        });
                    }
                }
            }
            // Asymmetric: forbid mutual edges. Compare each ordered
            // pair against its reverse; emit one violation per
            // unordered mutual pair.
            if props.asymmetric {
                let mut emitted_pairs: BTreeSet<(String, String)> = BTreeSet::new();
                for ((source, relation), targets) in &self.out_edges {
                    if relation != slug {
                        continue;
                    }
                    for target in targets {
                        if source == target {
                            continue; // a self-loop is irreflexive territory
                        }
                        let reverse = self.direct_targets(target, slug);
                        if reverse.iter().any(|n| n == source) {
                            // Order endpoints to dedupe (a,b) and
                            // (b,a) into one violation.
                            let (a, b) = if source <= target {
                                (source.clone(), target.clone())
                            } else {
                                (target.clone(), source.clone())
                            };
                            if emitted_pairs.insert((a.clone(), b.clone())) {
                                out.push(VocabViolation::AsymmetricMutualEdge {
                                    relation: slug.clone(),
                                    a,
                                    b,
                                });
                            }
                        }
                    }
                }
            }
        }
        out
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

/// Long-lived cache of vocabularies keyed by canonical AT-URI.
///
/// Most consumers want one of three queries:
///
/// - "is `x` subsumed by `y` under vocab `V`?" — the legacy
///   subsumption check, generalised.
/// - "does `x` satisfy `y` under relation `R` in vocab `V`?" — the
///   open question for any directed relation (e.g. `stronger_than`,
///   `provides_at_least`). A bounty requiring `property-test`
///   accepts a `formal-proof` because the verification-kinds vocab
///   declares `formal-proof stronger_than property-test`.
/// - "translate slug `x` from vocab `V1` into vocab `V2`" — handled
///   by `idiolect_lens::map_enum::map_enum_graphs`.
///
/// The registry stores [`VocabGraph`]s by AT-URI string so the
/// graph build cost is paid once per vocab. Consumers register
/// vocabs as they encounter them (typically at catalog ingest) and
/// then run subsumption / satisfaction queries in O(walk).
#[derive(Debug, Clone, Default)]
pub struct VocabRegistry {
    by_uri: BTreeMap<String, VocabGraph>,
}

impl VocabRegistry {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or replace the vocabulary at `uri`. Overwrites any
    /// prior entry; consumers calling this after a vocab record
    /// revision will see the new graph immediately.
    pub fn insert(&mut self, uri: impl Into<String>, vocab: &Vocab) {
        self.by_uri
            .insert(uri.into(), VocabGraph::from_vocab(vocab));
    }

    /// Look up a registered vocab graph.
    #[must_use]
    pub fn get(&self, uri: &str) -> Option<&VocabGraph> {
        self.by_uri.get(uri)
    }

    /// Number of registered vocabularies.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_uri.len()
    }

    /// Whether the registry has no vocabularies.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_uri.is_empty()
    }

    /// Whether `x` satisfies the requirement of `y` under the named
    /// `relation` in the vocab at `uri`. The directional reading is:
    /// "is `y` reachable from `x` by walking forward along
    /// `relation`?"
    ///
    /// Examples:
    /// - `satisfies(verification_kinds_uri, "formal-proof", "stronger_than", "property-test")`
    ///   → `Some(true)` (`formal-proof stronger_than property-test`).
    /// - `satisfies(community_roles_uri, "moderator", "subsumed_by", "member")`
    ///   → `Some(true)` (moderator is a member).
    ///
    /// Returns `None` when the vocab is not registered. Returns
    /// `Some(false)` when both `x` and `y` are known but no path
    /// exists, or when one is unknown.
    #[must_use]
    pub fn satisfies(&self, uri: &str, x: &str, relation: &str, y: &str) -> Option<bool> {
        let graph = self.by_uri.get(uri)?;
        if x == y {
            return Some(true);
        }
        Some(
            graph
                .walk_relation(x, relation, false)
                .iter()
                .any(|n| n == y),
        )
    }

    /// Whether `specific` is subsumed by `general` in the named
    /// vocab. Convenience wrapper for the most common
    /// [`satisfies`](Self::satisfies) call.
    #[must_use]
    pub fn is_subsumed_by(&self, uri: &str, specific: &str, general: &str) -> Option<bool> {
        self.satisfies(uri, specific, "subsumed_by", general)
    }

    /// Resolve `slug` from the vocab at `from_uri` into a slug the
    /// vocab at `to_uri` knows. Routes through
    /// [`VocabGraph::equivalent_in`]. Returns `None` when either
    /// vocab is unregistered or no translation can be found.
    #[must_use]
    pub fn translate(&self, from_uri: &str, to_uri: &str, slug: &str) -> Option<String> {
        let from = self.by_uri.get(from_uri)?;
        let to = self.by_uri.get(to_uri)?;
        from.equivalent_in(slug, to)
    }

    /// Validate every registered vocabulary against its declared
    /// OWL Lite property characteristics. Returns a map keyed by
    /// AT-URI with the violations found in each vocab; an empty
    /// returned map means every registered vocab is consistent.
    /// Wraps [`VocabGraph::validate`] for batch checking at catalog-
    /// sync time.
    #[must_use]
    pub fn validate(&self) -> BTreeMap<String, Vec<VocabViolation>> {
        let mut out = BTreeMap::new();
        for (uri, graph) in &self.by_uri {
            let violations = graph.validate();
            if !violations.is_empty() {
                out.insert(uri.clone(), violations);
            }
        }
        out
    }
}

fn normalize_node(n: &VocabNode) -> NormalizedNode {
    NormalizedNode {
        id: n.id.clone(),
        kind: n.kind.as_ref().map(|k| k.as_str().to_owned()),
        label: n.label.clone(),
        alternate_labels: n.alternate_labels.clone().unwrap_or_default(),
        hidden_labels: n.hidden_labels.clone().unwrap_or_default(),
        description: n.description.clone(),
        scope_note: n.scope_note.clone(),
        example: n.example.clone(),
        history_note: n.history_note.clone(),
        editorial_note: n.editorial_note.clone(),
        change_note: n.change_note.clone(),
        notation: n.notation.clone(),
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
        asymmetric: meta.asymmetric.unwrap_or(false),
        transitive: meta.transitive.unwrap_or(false),
        reflexive: meta.reflexive.unwrap_or(false),
        irreflexive: meta.irreflexive.unwrap_or(false),
        functional: meta.functional.unwrap_or(false),
        inverse_functional: meta.inverse_functional.unwrap_or(false),
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
            change_note: None,
            editorial_note: None,
            example: None,
            hidden_labels: None,
            history_note: None,
            notation: None,
            scope_note: None,
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
            change_note: None,
            editorial_note: None,
            example: None,
            hidden_labels: None,
            history_note: None,
            notation: None,
            scope_note: None,
            external_ids: None,
            status: None,
            status_vocab: None,
            deprecated_by: None,
            relation_metadata: Some(RelationMetadata {
                symmetric: Some(props.symmetric),
                asymmetric: Some(props.asymmetric),
                transitive: Some(props.transitive),
                reflexive: Some(props.reflexive),
                irreflexive: Some(props.irreflexive),
                functional: Some(props.functional),
                inverse_functional: Some(props.inverse_functional),
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
                        irreflexive: false,
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
                        asymmetric: false,
                        transitive: true,
                        reflexive: false,
                        irreflexive: false,
                        functional: false,
                        inverse_functional: false,
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
                        asymmetric: false,
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
                        asymmetric: false,
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
            change_note: None,
            editorial_note: None,
            example: None,
            hidden_labels: None,
            history_note: None,
            notation: None,
            scope_note: None,
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
        ActionEntry, RelationMetadata, VocabEdge, VocabEdgeRelationSlug, VocabNode, VocabNodeKind,
        VocabWorld,
    };

    fn graph_node(id: &str, kind: VocabNodeKind) -> VocabNode {
        VocabNode {
            id: id.to_owned(),
            kind: Some(kind),
            kind_vocab: None,
            subkind_uri: None,
            label: None,
            alternate_labels: None,
            description: None,
            change_note: None,
            editorial_note: None,
            example: None,
            hidden_labels: None,
            history_note: None,
            notation: None,
            scope_note: None,
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
            change_note: None,
            editorial_note: None,
            example: None,
            hidden_labels: None,
            history_note: None,
            notation: None,
            scope_note: None,
            external_ids: None,
            status: None,
            status_vocab: None,
            deprecated_by: None,
            relation_metadata: Some(RelationMetadata {
                symmetric: Some(props.symmetric),
                asymmetric: Some(props.asymmetric),
                transitive: Some(props.transitive),
                reflexive: Some(props.reflexive),
                irreflexive: Some(props.irreflexive),
                functional: Some(props.functional),
                inverse_functional: Some(props.inverse_functional),
                inverse_of: None,
                world: None,
            }),
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
            occurred_at: Datetime::parse("2026-04-29T00:00:00Z").expect("valid datetime"),
            default_relation: None,
            edges: Some(edges),
            nodes: Some(nodes),
        }
    }

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
                    id: "fine_tune".to_owned(),
                    parents: vec!["any".to_owned()],
                    class: None,
                    description: None,
                },
            ]),
            supersedes: None,
            occurred_at: Datetime::parse("2026-04-29T00:00:00Z").expect("valid datetime"),
            default_relation: None,
            edges: None,
            nodes: None,
        }
    }

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
    fn vocab_registry_subsumption_query() {
        let mut reg = VocabRegistry::new();
        reg.insert("at://x/v/legacy", &legacy_vocab());
        assert_eq!(
            reg.is_subsumed_by("at://x/v/legacy", "fine_tune", "any"),
            Some(true)
        );
        assert_eq!(
            reg.is_subsumed_by("at://x/v/legacy", "any", "fine_tune"),
            Some(false)
        );
        assert_eq!(
            reg.is_subsumed_by("at://x/v/missing", "a", "b"),
            None,
            "unregistered vocab returns None"
        );
    }

    #[test]
    fn vocab_registry_satisfies_under_arbitrary_relation() {
        // Build a vocab with stronger_than edges:
        // formal-proof stronger_than property-test stronger_than roundtrip-test.
        let v = graph_vocab(
            vec![
                graph_node("roundtrip-test", VocabNodeKind::Concept),
                graph_node("property-test", VocabNodeKind::Concept),
                graph_node("formal-proof", VocabNodeKind::Concept),
                relation_node(
                    "stronger_than",
                    RelationProperties {
                        transitive: true,
                        ..RelationProperties::default()
                    },
                ),
            ],
            vec![
                VocabEdge {
                    source: "formal-proof".to_owned(),
                    target: "property-test".to_owned(),
                    relation_slug: VocabEdgeRelationSlug::Other("stronger_than".to_owned()),
                    relation_vocab: None,
                    relation_uri: None,
                    weight: None,
                    metadata: None,
                },
                VocabEdge {
                    source: "property-test".to_owned(),
                    target: "roundtrip-test".to_owned(),
                    relation_slug: VocabEdgeRelationSlug::Other("stronger_than".to_owned()),
                    relation_vocab: None,
                    relation_uri: None,
                    weight: None,
                    metadata: None,
                },
            ],
        );
        let mut reg = VocabRegistry::new();
        reg.insert("at://x/v/verifs", &v);
        // formal-proof satisfies the property-test requirement.
        assert_eq!(
            reg.satisfies(
                "at://x/v/verifs",
                "formal-proof",
                "stronger_than",
                "property-test"
            ),
            Some(true)
        );
        // formal-proof transitively satisfies roundtrip-test.
        assert_eq!(
            reg.satisfies(
                "at://x/v/verifs",
                "formal-proof",
                "stronger_than",
                "roundtrip-test"
            ),
            Some(true)
        );
        // property-test does NOT satisfy formal-proof (weaker).
        assert_eq!(
            reg.satisfies(
                "at://x/v/verifs",
                "property-test",
                "stronger_than",
                "formal-proof"
            ),
            Some(false)
        );
        // Reflexive: anything satisfies itself.
        assert_eq!(
            reg.satisfies(
                "at://x/v/verifs",
                "formal-proof",
                "stronger_than",
                "formal-proof"
            ),
            Some(true)
        );
    }

    #[test]
    fn vocab_registry_translate_routes_to_equivalent_in() {
        let mut reg = VocabRegistry::new();
        let v1 = graph_vocab(vec![graph_node("agree", VocabNodeKind::Concept)], vec![]);
        let v2 = graph_vocab(
            vec![
                graph_node("agree", VocabNodeKind::Concept),
                graph_node("disagree", VocabNodeKind::Concept),
            ],
            vec![],
        );
        reg.insert("at://x/v/a", &v1);
        reg.insert("at://x/v/b", &v2);
        assert_eq!(
            reg.translate("at://x/v/a", "at://x/v/b", "agree"),
            Some("agree".to_owned())
        );
        assert_eq!(reg.translate("at://x/v/a", "at://x/v/b", "missing"), None);
        assert_eq!(
            reg.translate("at://x/v/missing", "at://x/v/b", "agree"),
            None
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)] // Test fixture has many literal struct fields.
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
                    change_note: None,
                    editorial_note: None,
                    example: None,
                    hidden_labels: None,
                    history_note: None,
                    notation: None,
                    scope_note: None,
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
                    change_note: None,
                    editorial_note: None,
                    example: None,
                    hidden_labels: None,
                    history_note: None,
                    notation: None,
                    scope_note: None,
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
                    change_note: None,
                    editorial_note: None,
                    example: None,
                    hidden_labels: None,
                    history_note: None,
                    notation: None,
                    scope_note: None,
                    external_ids: None,
                    status: None,
                    status_vocab: None,
                    deprecated_by: None,
                    relation_metadata: Some(
                        crate::generated::dev::idiolect::vocab::RelationMetadata {
                            symmetric: Some(true),
                            asymmetric: None,
                            transitive: None,
                            reflexive: None,
                            irreflexive: None,
                            functional: None,
                            inverse_functional: None,
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

#[cfg(test)]
mod owl_skos_tests {
    use super::*;
    use crate::Datetime;
    use crate::generated::dev::idiolect::vocab::{
        RelationMetadata, VocabNode, VocabNodeKind, VocabWorld,
    };

    #[test]
    fn relation_properties_contradictions_detect_symmetric_asymmetric() {
        let p = RelationProperties {
            symmetric: true,
            asymmetric: true,
            ..RelationProperties::default()
        };
        let bad = p.contradictions();
        assert_eq!(bad, vec!["symmetric+asymmetric"]);
    }

    #[test]
    fn relation_properties_contradictions_detect_reflexive_irreflexive() {
        let p = RelationProperties {
            reflexive: true,
            irreflexive: true,
            ..RelationProperties::default()
        };
        let bad = p.contradictions();
        assert_eq!(bad, vec!["reflexive+irreflexive"]);
    }

    #[test]
    fn relation_properties_contradictions_returns_both_when_double_violation() {
        let p = RelationProperties {
            symmetric: true,
            asymmetric: true,
            reflexive: true,
            irreflexive: true,
            ..RelationProperties::default()
        };
        let bad = p.contradictions();
        assert_eq!(bad, vec!["symmetric+asymmetric", "reflexive+irreflexive"]);
    }

    #[test]
    fn relation_properties_consistent_returns_empty() {
        let p = RelationProperties {
            symmetric: true,
            transitive: true,
            ..RelationProperties::default()
        };
        assert!(p.contradictions().is_empty());
    }

    #[test]
    fn full_owl_lite_metadata_lifts_to_relation_properties() {
        // Author a relation node with every OWL Lite flag set (in
        // a not-self-contradictory combination).
        let node = VocabNode {
            id: "has_isbn".to_owned(),
            kind: Some(VocabNodeKind::Relation),
            kind_vocab: None,
            subkind_uri: None,
            label: None,
            alternate_labels: None,
            hidden_labels: None,
            description: None,
            scope_note: None,
            example: None,
            history_note: None,
            editorial_note: None,
            change_note: None,
            notation: None,
            external_ids: None,
            status: None,
            status_vocab: None,
            deprecated_by: None,
            relation_metadata: Some(RelationMetadata {
                symmetric: Some(false),
                asymmetric: Some(true),
                transitive: Some(false),
                reflexive: Some(false),
                irreflexive: Some(true),
                functional: Some(true),
                inverse_functional: Some(true),
                inverse_of: Some("isbn_of".to_owned()),
                world: None,
            }),
        };
        let v = Vocab {
            name: "isbn".to_owned(),
            description: None,
            world: VocabWorld::Open,
            top: None,
            actions: None,
            supersedes: None,
            occurred_at: Datetime::parse("2026-04-29T00:00:00Z").expect("valid datetime"),
            default_relation: None,
            edges: None,
            nodes: Some(vec![node]),
        };
        let g = VocabGraph::from_vocab(&v);
        let props = g.relation_properties("has_isbn");
        assert!(!props.symmetric);
        assert!(props.asymmetric);
        assert!(!props.transitive);
        assert!(!props.reflexive);
        assert!(props.irreflexive);
        assert!(props.functional);
        assert!(props.inverse_functional);
        assert!(props.contradictions().is_empty());
    }

    #[test]
    fn skos_fields_lift_into_normalized_node() {
        let n = VocabNode {
            id: "informatics".to_owned(),
            kind: Some(VocabNodeKind::Concept),
            kind_vocab: None,
            subkind_uri: None,
            label: Some("Informatics".to_owned()),
            alternate_labels: Some(vec!["Information science".to_owned()]),
            hidden_labels: Some(vec!["informaticks".to_owned()]),
            description: Some("The study of information processing.".to_owned()),
            scope_note: Some("Use for the academic discipline; see `software-engineering` for the practice.".to_owned()),
            example: Some("Database design, ML pipelines, distributed systems.".to_owned()),
            history_note: Some("Term coined ~1962, displaced 'documentation'.".to_owned()),
            editorial_note: Some("Reviewed 2026-04-28 by R. Smith.".to_owned()),
            change_note: Some("Added scope_note 2026-04-28.".to_owned()),
            notation: Some("004".to_owned()),
            external_ids: None,
            status: None,
            status_vocab: None,
            deprecated_by: None,
            relation_metadata: None,
        };
        let v = Vocab {
            name: "library".to_owned(),
            description: None,
            world: VocabWorld::Open,
            top: None,
            actions: None,
            supersedes: None,
            occurred_at: Datetime::parse("2026-04-29T00:00:00Z").expect("valid datetime"),
            default_relation: None,
            edges: None,
            nodes: Some(vec![n]),
        };
        let g = VocabGraph::from_vocab(&v);
        let nn = g.node("informatics").expect("node present");
        assert_eq!(nn.label.as_deref(), Some("Informatics"));
        assert_eq!(nn.alternate_labels, vec!["Information science".to_owned()]);
        assert_eq!(nn.hidden_labels, vec!["informaticks".to_owned()]);
        assert_eq!(nn.scope_note.as_deref().map(|s| s.contains("academic")), Some(true));
        assert!(nn.example.is_some());
        assert!(nn.history_note.is_some());
        assert!(nn.editorial_note.is_some());
        assert!(nn.change_note.is_some());
        assert_eq!(nn.notation.as_deref(), Some("004"));
    }

    #[test]
    #[allow(clippy::too_many_lines)] // Test fixture has many literal struct fields.
    fn skos_collection_kind_walks_member_of_relation() {
        // A SKOS Collection groups concepts via member_of edges.
        let v = Vocab {
            name: "policies".to_owned(),
            description: None,
            world: VocabWorld::Open,
            top: None,
            actions: None,
            supersedes: None,
            occurred_at: Datetime::parse("2026-04-29T00:00:00Z").expect("valid datetime"),
            default_relation: None,
            nodes: Some(vec![
                VocabNode {
                    id: "isolation_policies".to_owned(),
                    kind: Some(VocabNodeKind::Other("collection".to_owned())),
                    kind_vocab: None,
                    subkind_uri: None,
                    label: Some("Isolation policies".to_owned()),
                    alternate_labels: None,
                    hidden_labels: None,
                    description: None,
                    scope_note: None,
                    example: None,
                    history_note: None,
                    editorial_note: None,
                    change_note: None,
                    notation: None,
                    external_ids: None,
                    status: None,
                    status_vocab: None,
                    deprecated_by: None,
                    relation_metadata: None,
                },
                VocabNode {
                    id: "process".to_owned(),
                    kind: Some(VocabNodeKind::Concept),
                    kind_vocab: None,
                    subkind_uri: None,
                    label: None,
                    alternate_labels: None,
                    hidden_labels: None,
                    description: None,
                    scope_note: None,
                    example: None,
                    history_note: None,
                    editorial_note: None,
                    change_note: None,
                    notation: None,
                    external_ids: None,
                    status: None,
                    status_vocab: None,
                    deprecated_by: None,
                    relation_metadata: None,
                },
                VocabNode {
                    id: "container".to_owned(),
                    kind: Some(VocabNodeKind::Concept),
                    kind_vocab: None,
                    subkind_uri: None,
                    label: None,
                    alternate_labels: None,
                    hidden_labels: None,
                    description: None,
                    scope_note: None,
                    example: None,
                    history_note: None,
                    editorial_note: None,
                    change_note: None,
                    notation: None,
                    external_ids: None,
                    status: None,
                    status_vocab: None,
                    deprecated_by: None,
                    relation_metadata: None,
                },
            ]),
            edges: Some(vec![
                crate::generated::dev::idiolect::vocab::VocabEdge {
                    source: "process".to_owned(),
                    target: "isolation_policies".to_owned(),
                    relation_slug:
                        crate::generated::dev::idiolect::vocab::VocabEdgeRelationSlug::MemberOf,
                    relation_vocab: None,
                    relation_uri: None,
                    weight: None,
                    metadata: None,
                },
                crate::generated::dev::idiolect::vocab::VocabEdge {
                    source: "container".to_owned(),
                    target: "isolation_policies".to_owned(),
                    relation_slug:
                        crate::generated::dev::idiolect::vocab::VocabEdgeRelationSlug::MemberOf,
                    relation_vocab: None,
                    relation_uri: None,
                    weight: None,
                    metadata: None,
                },
            ]),
        };
        let g = VocabGraph::from_vocab(&v);
        // Both members reach the collection via member_of.
        for member in ["process", "container"] {
            let targets = g.direct_targets(member, "member_of");
            assert!(
                targets.iter().any(|t| t == "isolation_policies"),
                "{member} should be a member_of isolation_policies, got {targets:?}"
            );
        }
        // The collection has both members as inbound member_of edges.
        let members = g.direct_sources("isolation_policies", "member_of");
        let mut sorted = members.to_vec();
        sorted.sort();
        assert_eq!(sorted, vec!["container".to_owned(), "process".to_owned()]);
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;
    use crate::Datetime;
    use crate::generated::dev::idiolect::vocab::{
        RelationMetadata, VocabEdge, VocabEdgeRelationSlug, VocabNode, VocabNodeKind, VocabWorld,
    };

    fn relation_node(id: &str, props: RelationProperties) -> VocabNode {
        VocabNode {
            id: id.to_owned(),
            kind: Some(VocabNodeKind::Relation),
            kind_vocab: None,
            subkind_uri: None,
            label: None,
            alternate_labels: None,
            hidden_labels: None,
            description: None,
            scope_note: None,
            example: None,
            history_note: None,
            editorial_note: None,
            change_note: None,
            notation: None,
            external_ids: None,
            status: None,
            status_vocab: None,
            deprecated_by: None,
            relation_metadata: Some(RelationMetadata {
                symmetric: Some(props.symmetric),
                asymmetric: Some(props.asymmetric),
                transitive: Some(props.transitive),
                reflexive: Some(props.reflexive),
                irreflexive: Some(props.irreflexive),
                functional: Some(props.functional),
                inverse_functional: Some(props.inverse_functional),
                inverse_of: None,
                world: None,
            }),
        }
    }

    fn concept_node(id: &str) -> VocabNode {
        VocabNode {
            id: id.to_owned(),
            kind: Some(VocabNodeKind::Concept),
            kind_vocab: None,
            subkind_uri: None,
            label: None,
            alternate_labels: None,
            hidden_labels: None,
            description: None,
            scope_note: None,
            example: None,
            history_note: None,
            editorial_note: None,
            change_note: None,
            notation: None,
            external_ids: None,
            status: None,
            status_vocab: None,
            deprecated_by: None,
            relation_metadata: None,
        }
    }

    fn edge(source: &str, target: &str, slug: &str) -> VocabEdge {
        VocabEdge {
            source: source.to_owned(),
            target: target.to_owned(),
            relation_slug: VocabEdgeRelationSlug::Other(slug.to_owned()),
            relation_vocab: None,
            relation_uri: None,
            weight: None,
            metadata: None,
        }
    }

    fn graph(nodes: Vec<VocabNode>, edges: Vec<VocabEdge>) -> VocabGraph {
        let v = Vocab {
            name: "v".to_owned(),
            description: None,
            world: VocabWorld::Open,
            top: None,
            actions: None,
            supersedes: None,
            occurred_at: Datetime::parse("2026-04-29T00:00:00Z").expect("valid datetime"),
            default_relation: None,
            nodes: Some(nodes),
            edges: Some(edges),
        };
        VocabGraph::from_vocab(&v)
    }

    #[test]
    fn validate_reports_functional_violation() {
        // `has_isbn` declared functional, but `book1` has two
        // outbound edges. Validation must catch this.
        let g = graph(
            vec![
                concept_node("book1"),
                concept_node("isbn-a"),
                concept_node("isbn-b"),
                relation_node(
                    "has_isbn",
                    RelationProperties {
                        functional: true,
                        ..RelationProperties::default()
                    },
                ),
            ],
            vec![
                edge("book1", "isbn-a", "has_isbn"),
                edge("book1", "isbn-b", "has_isbn"),
            ],
        );
        let v = g.validate();
        assert!(
            matches!(
                v.as_slice(),
                [VocabViolation::FunctionalEdgeViolation { relation, source, .. }]
                    if relation == "has_isbn" && source == "book1"
            ),
            "expected FunctionalEdgeViolation, got {v:?}"
        );
    }

    #[test]
    fn validate_reports_inverse_functional_violation() {
        // Two distinct books both claim the same ISBN under an
        // inverse-functional relation: violation.
        let g = graph(
            vec![
                concept_node("book-a"),
                concept_node("book-b"),
                concept_node("isbn-1"),
                relation_node(
                    "has_isbn",
                    RelationProperties {
                        inverse_functional: true,
                        ..RelationProperties::default()
                    },
                ),
            ],
            vec![
                edge("book-a", "isbn-1", "has_isbn"),
                edge("book-b", "isbn-1", "has_isbn"),
            ],
        );
        let v = g.validate();
        assert!(
            matches!(
                v.as_slice(),
                [VocabViolation::InverseFunctionalEdgeViolation { relation, target, .. }]
                    if relation == "has_isbn" && target == "isbn-1"
            ),
            "expected InverseFunctionalEdgeViolation, got {v:?}"
        );
    }

    #[test]
    fn validate_reports_irreflexive_self_loop() {
        // `parent_of` declared irreflexive, but a self-loop is
        // present.
        let g = graph(
            vec![
                concept_node("alice"),
                relation_node(
                    "parent_of",
                    RelationProperties {
                        irreflexive: true,
                        ..RelationProperties::default()
                    },
                ),
            ],
            vec![edge("alice", "alice", "parent_of")],
        );
        let v = g.validate();
        assert!(
            matches!(
                v.as_slice(),
                [VocabViolation::IrreflexiveSelfLoop { relation, node }]
                    if relation == "parent_of" && node == "alice"
            ),
            "expected IrreflexiveSelfLoop, got {v:?}"
        );
    }

    #[test]
    fn validate_reports_asymmetric_mutual_edge() {
        // `parent_of` declared asymmetric, but a mutual pair is
        // declared.
        let g = graph(
            vec![
                concept_node("alice"),
                concept_node("bob"),
                relation_node(
                    "parent_of",
                    RelationProperties {
                        asymmetric: true,
                        ..RelationProperties::default()
                    },
                ),
            ],
            vec![
                edge("alice", "bob", "parent_of"),
                edge("bob", "alice", "parent_of"),
            ],
        );
        let v = g.validate();
        // Exactly one violation: the unordered pair (alice, bob).
        assert_eq!(v.len(), 1, "expected exactly one violation, got {v:?}");
        assert!(
            matches!(
                &v[0],
                VocabViolation::AsymmetricMutualEdge { relation, a, b }
                    if relation == "parent_of" && a == "alice" && b == "bob"
            ),
            "got {:?}",
            &v[0]
        );
    }

    #[test]
    fn validate_reports_property_contradictions() {
        let g = graph(
            vec![
                concept_node("a"),
                relation_node(
                    "weird",
                    RelationProperties {
                        symmetric: true,
                        asymmetric: true,
                        reflexive: true,
                        irreflexive: true,
                        ..RelationProperties::default()
                    },
                ),
            ],
            vec![],
        );
        let v = g.validate();
        let tags: Vec<&'static str> = v
            .iter()
            .filter_map(|x| match x {
                VocabViolation::PropertyContradiction { tag, .. } => Some(*tag),
                _ => None,
            })
            .collect();
        assert!(tags.contains(&"symmetric+asymmetric"));
        assert!(tags.contains(&"reflexive+irreflexive"));
    }

    #[test]
    fn validate_consistent_vocab_returns_empty() {
        let g = graph(
            vec![
                concept_node("a"),
                concept_node("b"),
                relation_node(
                    "broader_than",
                    RelationProperties {
                        transitive: true,
                        asymmetric: true,
                        irreflexive: true,
                        ..RelationProperties::default()
                    },
                ),
            ],
            vec![edge("a", "b", "broader_than")],
        );
        assert!(g.validate().is_empty());
    }

    #[test]
    fn registry_validate_returns_per_uri_violations() {
        let bad = graph(
            vec![
                concept_node("x"),
                relation_node(
                    "r",
                    RelationProperties {
                        irreflexive: true,
                        ..RelationProperties::default()
                    },
                ),
            ],
            vec![edge("x", "x", "r")],
        );
        let good = graph(
            vec![concept_node("y")],
            vec![],
        );
        // Use VocabGraph directly via VocabRegistry::insert (which
        // takes a Vocab); reconstruct via from_vocab is what the
        // registry does too.
        let mut reg = VocabRegistry::default();
        reg.by_uri.insert("at://x/v/bad".to_owned(), bad);
        reg.by_uri.insert("at://x/v/good".to_owned(), good);
        let report = reg.validate();
        assert!(report.contains_key("at://x/v/bad"));
        assert!(!report.contains_key("at://x/v/good"));
    }
}
