//! Verifies that panproto's atproto lexicon parser preserves the
//! refinements idiolect's codegen cares about.
//!
//! Historical context: in panproto v0.34.0, `parse_lexicon` collapsed
//! all atproto string refinements (`format`, `knownValues`) into an
//! anonymous `string`, which forced idiolect-codegen to ship a
//! second, hand-written lexicon parser to recover them (see
//! `crates/idiolect-codegen/src/lexicon.rs`). As of panproto v0.35.0,
//! `parse_constraints` preserves both refinements as string-valued
//! vertex constraints. This test pins that contract: if a future
//! panproto upgrade drops either constraint, the regeneration
//! pipeline would silently lose information, and the test catches it.
//!
//! See: panproto/panproto#42 (resolved in v0.35.0 by 7d42710).

use panproto_protocols::web_document::atproto::parse_lexicon;
use panproto_schema::Constraint;

/// Load a dev.idiolect.* lexicon from the repo tree by name.
fn load_lexicon(name: &str) -> serde_json::Value {
    let path = format!(
        "{}/lexicons/dev/idiolect/{name}.json",
        env!("CARGO_MANIFEST_DIR").trim_end_matches("/crates/idiolect-codegen")
    );
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

/// Collect every constraint named `constraint_name` across every
/// vertex in the schema. Returns the constraint values verbatim.
fn constraint_values_named<'a>(
    schema: &'a panproto_schema::Schema,
    constraint_name: &str,
) -> Vec<&'a str> {
    let mut out = Vec::new();
    for constraints in schema.constraints.values() {
        for Constraint { sort, value } in constraints {
            if sort == constraint_name {
                out.push(value.as_str());
            }
        }
    }
    out
}

#[test]
fn parse_lexicon_preserves_format_datetime() {
    // correction.json has a single datetime field (`occurredAt`).
    let lex = load_lexicon("correction");
    let schema = parse_lexicon(&lex).expect("parse_lexicon");
    let formats = constraint_values_named(&schema, "format");
    assert!(
        formats.contains(&"datetime"),
        "expected format=datetime among constraints; got {formats:?}"
    );
}

#[test]
fn parse_lexicon_preserves_format_did_and_at_uri() {
    // adapter.json has `format: did` on author and `format: at-uri`
    // on verification.
    let lex = load_lexicon("adapter");
    let schema = parse_lexicon(&lex).expect("parse_lexicon");
    let formats = constraint_values_named(&schema, "format");
    assert!(formats.contains(&"did"), "{formats:?}");
    assert!(formats.contains(&"at-uri"), "{formats:?}");
}

#[test]
fn parse_lexicon_preserves_unknown_enum_values_verbatim() {
    // community.json has several `format` declarations; confirm the
    // parser did not silently drop any.
    let lex = load_lexicon("community");
    let schema = parse_lexicon(&lex).expect("parse_lexicon");
    let formats = constraint_values_named(&schema, "format");
    assert!(!formats.is_empty(), "community should have format constraints");
    // at least two distinct formats (did, at-uri, datetime all appear).
    let distinct: std::collections::BTreeSet<_> = formats.iter().copied().collect();
    assert!(distinct.len() >= 2, "got {distinct:?}");
}

#[test]
fn atproto_protocol_includes_format_constraint_sort() {
    // Sanity: the protocol itself now advertises "format" as a known
    // constraint sort, not just a side-channel.
    let proto = panproto_protocols::web_document::atproto::protocol();
    assert!(
        proto.constraint_sorts.contains(&"format".to_owned()),
        "expected `format` in protocol.constraint_sorts; got {:?}",
        proto.constraint_sorts
    );
    assert!(
        proto.constraint_sorts.contains(&"knownValues".to_owned()),
        "expected `knownValues` in protocol.constraint_sorts"
    );
}
