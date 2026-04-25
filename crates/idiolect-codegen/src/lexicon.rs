//! Typed, opinionated view of an idiolect lexicon.
//!
//! `panproto_schema::Schema` is the canonical graph representation, but
//! it drops a few fields we need for richer codegen (notably the
//! `ATProto` `"format"` hint on strings, and `"knownValues"`). We read
//! the raw lexicon json alongside the panproto graph to recover those.
//!
//! This module is deliberately idiolect-flavoured: it understands the
//! subset of lexicon features idiolect uses. Extending it to cover more
//! of the lexicon spec is a contribution-to-panproto task, not a
//! hand-written-types task.

use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

/// Parsed, typed representation of one `dev.idiolect.*` lexicon document.
#[derive(Debug, Clone)]
pub struct LexiconDoc {
    /// Full nsid (e.g. `dev.idiolect.encounter`).
    pub nsid: String,
    /// Top-level description on the lexicon object.
    pub description: Option<String>,
    /// Every def keyed by its def name (`main`, `wantLens`, ...).
    pub defs: BTreeMap<String, Def>,
}

/// One def inside a lexicon's `defs` map, classified by kind.
#[derive(Debug, Clone)]
pub enum Def {
    /// `{"type": "record", ...}` at `defs.main`.
    Record(RecordDef),
    /// `{"type": "object", ...}`.
    Object(ObjectDef),
    /// `{"type": "string", "enum": [...]}` — a closed enum.
    StringEnum(StringEnumDef),
    /// `{"type": "union", "refs": [...]}`.
    Union(UnionDef),
}

/// A record def (`{"type": "record", ...}` at `defs.main`).
#[derive(Debug, Clone)]
pub struct RecordDef {
    /// Record-level description (the outer `description` field).
    pub description: Option<String>,
    /// The `key` strategy (`tid`, `literal:<s>`, etc.), if specified.
    pub key: Option<String>,
    /// The record's body — the nested object def under `"record"`.
    pub body: ObjectDef,
}

/// An object def (`{"type": "object", ...}` — either top-level or inline).
#[derive(Debug, Clone)]
pub struct ObjectDef {
    /// Description on the object itself.
    pub description: Option<String>,
    /// Names of properties that are required.
    pub required: Vec<String>,
    /// Properties in source order.
    pub properties: Vec<(String, Prop)>,
}

/// One property of an object.
#[derive(Debug, Clone)]
pub struct Prop {
    /// Description of the property.
    pub description: Option<String>,
    /// Shape of the property's value.
    pub ty: PropType,
}

/// The shape of a property's value.
#[derive(Debug, Clone)]
pub enum PropType {
    /// Plain string.
    String,
    /// String constrained to the atproto `datetime` format.
    StringDatetime,
    /// Integer (signed, arbitrary width — rendered as `i64`).
    Integer,
    /// Boolean.
    Boolean,
    /// Floating-point number.
    Number,
    /// Content-addressed link (atproto `cid-link`).
    CidLink,
    /// Raw byte string.
    Bytes,
    /// Binary blob reference.
    Blob,
    /// Opaque value whose shape is not declared.
    Unknown,
    /// `{"type": "ref", "ref": "target"}` pointing to another def.
    Ref(RefTarget),
    /// `{"type": "array", "items": {...}}`.
    Array(Box<Self>),
    /// Inline closed enum — `{"type": "string", "enum": [...]}`.
    InlineStringEnum(Vec<String>),
    /// Inline union — `{"type": "union", "refs": [...]}`.
    InlineUnion(Vec<RefTarget>),
    /// Inline object — `{"type": "object", "properties": {...}}`
    /// defined at the property site rather than via ref.
    InlineObject(Box<ObjectDef>),
}

/// A resolved ref target, i.e. pointer to another def.
#[derive(Debug, Clone)]
pub struct RefTarget {
    /// Nsid of the lexicon the target lives in, e.g. `dev.idiolect.defs`.
    pub nsid: String,
    /// Def name within that lexicon, e.g. `visibility` or `wantLens`.
    pub def_name: String,
}

/// A closed string enum def.
#[derive(Debug, Clone)]
pub struct StringEnumDef {
    /// Description of the enum.
    pub description: Option<String>,
    /// Enumerated values, in source order.
    pub values: Vec<String>,
}

/// A union def — one of several named refs, tagged by `$type`.
#[derive(Debug, Clone)]
pub struct UnionDef {
    /// Description of the union.
    pub description: Option<String>,
    /// The union variants.
    pub variants: Vec<RefTarget>,
}

/// Parse one lexicon json document into the opinionated model.
///
/// # Errors
///
/// Returns an error if the json is missing required structure
/// (`id`, `defs`) or contains unsupported constructs.
pub fn parse(json: &Value) -> Result<LexiconDoc> {
    let nsid = json
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("lexicon missing id"))?
        .to_string();
    let description = json
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let defs_obj = json
        .get("defs")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("{nsid}: missing defs"))?;

    let mut defs = BTreeMap::new();
    for (def_name, def_json) in defs_obj {
        let parsed =
            parse_def(&nsid, def_name, def_json).with_context(|| format!("{nsid}#{def_name}"))?;
        defs.insert(def_name.clone(), parsed);
    }

    Ok(LexiconDoc {
        nsid,
        description,
        defs,
    })
}

fn parse_def(current_nsid: &str, def_name: &str, def_json: &Value) -> Result<Def> {
    let ty = def_json
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("def missing type"))?;
    let description = def_json
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_owned);

    match ty {
        "record" => {
            let key = def_json
                .get("key")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let record_body = def_json
                .get("record")
                .ok_or_else(|| anyhow!("record def {def_name} missing record body"))?;
            let body = parse_object(current_nsid, record_body)?;
            Ok(Def::Record(RecordDef {
                description,
                key,
                body,
            }))
        }
        "object" => {
            let obj = parse_object(current_nsid, def_json)?;
            Ok(Def::Object(ObjectDef {
                description: description.or(obj.description),
                ..obj
            }))
        }
        "string" => {
            let Some(enum_values) = def_json.get("enum").and_then(Value::as_array) else {
                bail!(
                    "top-level string def {def_name} has no enum (idiolect only uses closed enums here)"
                );
            };
            let values = enum_values
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect::<Vec<_>>();
            Ok(Def::StringEnum(StringEnumDef {
                description,
                values,
            }))
        }
        "union" => {
            let variants = parse_union_refs(current_nsid, def_json)?;
            Ok(Def::Union(UnionDef {
                description,
                variants,
            }))
        }
        other => bail!("unsupported def kind: {other}"),
    }
}

fn parse_object(current_nsid: &str, obj_json: &Value) -> Result<ObjectDef> {
    let description = obj_json
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let required = obj_json
        .get("required")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();

    let properties = obj_json
        .get("properties")
        .and_then(Value::as_object)
        .map(|o| {
            o.iter()
                .map(|(name, def)| {
                    let prop = parse_prop(current_nsid, def)
                        .with_context(|| format!("property {name}"))?;
                    Ok((name.clone(), prop))
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();

    Ok(ObjectDef {
        description,
        required,
        properties,
    })
}

fn parse_prop(current_nsid: &str, def: &Value) -> Result<Prop> {
    let description = def
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let ty = parse_prop_type(current_nsid, def)?;
    Ok(Prop { description, ty })
}

fn parse_prop_type(current_nsid: &str, def: &Value) -> Result<PropType> {
    let ty = def
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("prop missing type"))?;

    match ty {
        "string" => {
            if let Some(enum_values) = def.get("enum").and_then(Value::as_array) {
                let values = enum_values
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
                return Ok(PropType::InlineStringEnum(values));
            }
            if def.get("format").and_then(Value::as_str) == Some("datetime") {
                return Ok(PropType::StringDatetime);
            }
            Ok(PropType::String)
        }
        "integer" => Ok(PropType::Integer),
        "boolean" => Ok(PropType::Boolean),
        "number" => Ok(PropType::Number),
        "cid-link" => Ok(PropType::CidLink),
        "bytes" => Ok(PropType::Bytes),
        "blob" => Ok(PropType::Blob),
        "unknown" => Ok(PropType::Unknown),
        "ref" => {
            let target = def
                .get("ref")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("ref def missing ref target"))?;
            Ok(PropType::Ref(resolve_ref(current_nsid, target)))
        }
        "array" => {
            let items = def
                .get("items")
                .ok_or_else(|| anyhow!("array def missing items"))?;
            let inner = parse_prop_type(current_nsid, items).context("array items")?;
            Ok(PropType::Array(Box::new(inner)))
        }
        "union" => {
            let variants = parse_union_refs(current_nsid, def)?;
            Ok(PropType::InlineUnion(variants))
        }
        "object" => {
            let obj = parse_object(current_nsid, def)?;
            Ok(PropType::InlineObject(Box::new(obj)))
        }
        other => bail!("unsupported property type: {other}"),
    }
}

fn parse_union_refs(current_nsid: &str, def: &Value) -> Result<Vec<RefTarget>> {
    let refs = def
        .get("refs")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("union def missing refs"))?;
    Ok(refs
        .iter()
        .filter_map(Value::as_str)
        .map(|s| resolve_ref(current_nsid, s))
        .collect())
}

/// Split a lexicon ref like `dev.idiolect.defs#visibility` or `#wantLens`
/// into `(nsid, def_name)`. Local refs (`#foo`) resolve against the
/// current lexicon's nsid.
#[must_use]
pub fn resolve_ref(current_nsid: &str, raw: &str) -> RefTarget {
    if let Some(local) = raw.strip_prefix('#') {
        return RefTarget {
            nsid: current_nsid.to_owned(),
            def_name: local.to_owned(),
        };
    }
    if let Some((nsid, frag)) = raw.split_once('#') {
        return RefTarget {
            nsid: nsid.to_owned(),
            def_name: frag.to_owned(),
        };
    }
    // bare nsid refers to its `main` def
    RefTarget {
        nsid: raw.to_owned(),
        def_name: "main".to_owned(),
    }
}

/// Derive the dotted-path module location for an nsid.
///
/// Returns each NSID segment, snake-cased and Rust-keyword-escaped
/// where necessary (raw-identifier prefix for segments like `pub` or
/// `crate`). The path mirrors the lexicon's position under
/// `lexicons/`, so `dev.idiolect.encounter` becomes
/// `["dev", "idiolect", "encounter"]` and emits
/// `dev/idiolect/encounter.rs`; `dev.panproto.schema.lens` becomes
/// `["dev", "panproto", "schema", "lens"]` and emits
/// `dev/panproto/schema/lens.rs`.
///
/// The last element is the file stem (no `.rs` extension); preceding
/// elements are directory components. An empty input returns an empty
/// vec — callers should reject before reaching emit.
#[must_use]
pub fn module_path_for_nsid(nsid: &str) -> Vec<String> {
    // Snake-case each segment for filesystem and module-name use. Do
    // not raw-identifier-escape here: the path also serves as a
    // cross-language file location (`dev/idiolect/encounter.ts` etc.),
    // and `r#pub` is not a valid filename. Callers that need the Rust
    // `r#` form (the `pub mod ...` declaration in `mod.rs`) reach for
    // `is_rust_keyword` and add the prefix at the use site.
    nsid.split('.')
        .filter(|s| !s.is_empty())
        .map(to_snake)
        .collect()
}

/// True if `s` (already snake-cased) is a Rust strict keyword that
/// would collide with module / identifier syntax. Use to decide
/// whether a `pub mod` declaration needs the `r#` raw-identifier
/// prefix; the path itself stays plain so it works as a filename.
#[must_use]
pub fn is_rust_keyword(s: &str) -> bool {
    is_rust_reserved(s)
}

/// Convenience: the leaf module name (file stem) for an nsid. Useful
/// for type-name derivation: `pascal_case` of the result yields the
/// main record/def type for the lexicon.
#[must_use]
pub fn leaf_module_name_for_nsid(nsid: &str) -> String {
    module_path_for_nsid(nsid).pop().unwrap_or_default()
}

/// Stable type-name token for an nsid.
///
/// Idiolect's generated types have historically disambiguated cross
/// -family modules with a family prefix: `dev.panproto.schema.lens`
/// becomes `PanprotoLens`, not `Lens`. The lexicon-tree file layout
/// (see `module_path_for_nsid`) is enough to disambiguate at the
/// path level, but the type names are part of every consumer's
/// import surface, so we keep the prefix to avoid a workspace-wide
/// rename. `dev.idiolect.*` still maps to the bare leaf segment.
#[must_use]
pub fn module_name_for_nsid(nsid: &str) -> String {
    let parts: Vec<&str> = nsid.split('.').filter(|s| !s.is_empty()).collect();
    let last = parts.last().copied().unwrap_or(nsid);
    let last_snake = to_snake(last);
    if parts.len() >= 3 && parts[0] == "dev" && parts[1] == "idiolect" {
        last_snake
    } else if parts.len() >= 3 && parts[0] == "dev" {
        format!("{}_{}", parts[1], last_snake)
    } else {
        last_snake
    }
}

fn is_rust_reserved(s: &str) -> bool {
    matches!(
        s,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
            | "try"
    )
}

/// Derive the atproto family an nsid belongs to — the second dotted segment.
///
/// Returns the family name (e.g. `idiolect`, `panproto`) for an nsid like
/// `dev.<family>.<...>`. Used to route fixtures to the family that owns
/// them and to scope filename → nsid lookups so a fixture named
/// `lens.json` under `lexicons/dev/panproto/examples/` resolves to
/// `dev.panproto.schema.lens`, not some future `dev.idiolect.lens`.
#[must_use]
pub fn family_of_nsid(nsid: &str) -> Option<&str> {
    let mut it = nsid.split('.');
    let first = it.next()?;
    if first != "dev" {
        return None;
    }
    it.next()
}

fn to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == ' ' {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{family_of_nsid, leaf_module_name_for_nsid, module_path_for_nsid};

    #[test]
    fn module_path_mirrors_nsid_segments() {
        assert_eq!(
            module_path_for_nsid("dev.idiolect.encounter"),
            vec!["dev", "idiolect", "encounter"],
        );
        assert_eq!(
            module_path_for_nsid("dev.panproto.schema.lens"),
            vec!["dev", "panproto", "schema", "lens"],
        );
    }

    #[test]
    fn module_path_snake_cases_camel_segments() {
        assert_eq!(
            module_path_for_nsid("dev.panproto.vcs.refUpdate"),
            vec!["dev", "panproto", "vcs", "ref_update"],
        );
        assert_eq!(
            module_path_for_nsid("dev.panproto.schema.lensAttestation"),
            vec!["dev", "panproto", "schema", "lens_attestation"],
        );
    }

    #[test]
    fn module_path_keeps_keyword_segments_plain() {
        // Path segments stay plain so they double as filenames.
        // Callers that need the Rust `pub mod r#pub;` form check
        // `is_rust_keyword(seg)` and add the prefix at the use site.
        let path = module_path_for_nsid("pub.layers.annotation.defs");
        assert_eq!(path, vec!["pub", "layers", "annotation", "defs"]);
        assert!(super::is_rust_keyword(&path[0]));
        assert!(!super::is_rust_keyword(&path[1]));
    }

    #[test]
    fn leaf_module_name_returns_last_segment() {
        assert_eq!(
            leaf_module_name_for_nsid("dev.idiolect.encounter"),
            "encounter",
        );
        assert_eq!(
            leaf_module_name_for_nsid("dev.panproto.schema.lens"),
            "lens",
        );
    }

    #[test]
    fn shared_last_segment_does_not_collide() {
        // Issue #21: two NSIDs with identical last segments must not
        // both want to write `defs.rs`. Under the lexicon-tree layout
        // they land in distinct parent directories, which is exactly
        // what the codegen needs to avoid the collision.
        let a = module_path_for_nsid("pub.layers.annotation.defs");
        let b = module_path_for_nsid("pub.layers.corpus.defs");
        assert_ne!(a, b);
        assert_eq!(a.last(), b.last());
    }

    #[test]
    fn family_extraction_matches() {
        assert_eq!(family_of_nsid("dev.idiolect.encounter"), Some("idiolect"));
        assert_eq!(family_of_nsid("dev.panproto.schema.lens"), Some("panproto"));
        assert_eq!(family_of_nsid("com.example.foo"), None);
    }
}
