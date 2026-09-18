//! Panproto-backed schema consequence analysis.

use std::fs;
use std::path::Path;

use panproto_check::{Classification, classify, diff};
use panproto_expr::limits::ResourceLimits;
use panproto_lens::auto_lens::{AutoLensConfig, auto_generate};
use panproto_lens::optic::OpticKind;
use panproto_protocols::parse_schema_document_within;
use sha2::{Digest, Sha256};

use crate::error::{CommunityError, CommunityResult};
use crate::model::{
    ChangeImpact, Compatibility, ConsequenceReport, OpticClass, ResourcePolicy, SchemaEndpoint,
};

/// Compare two schema documents through Panproto's canonical protocol
/// registry and translate the formal result into a community consequence
/// report.
///
/// The same shared budget is passed through both document parses. This means
/// the operation is bounded as a whole rather than resetting an allowance for
/// each input file.
///
/// # Errors
///
/// Returns an error when either document cannot be read or parsed within the
/// resource policy, when the protocol is unknown, or when analysis fails.
#[allow(clippy::too_many_lines)]
pub fn analyze_schema_change(
    source_path: &Path,
    target_path: &Path,
    protocol_name: &str,
    policy: &ResourcePolicy,
) -> CommunityResult<(SchemaEndpoint, SchemaEndpoint, ConsequenceReport)> {
    let source_bytes = read_bounded(source_path, policy.max_schema_bytes)?;
    let target_bytes = read_bounded(target_path, policy.max_schema_bytes)?;
    let total = source_bytes.len().saturating_add(target_bytes.len()) as u64;
    if total > policy.max_schema_bytes {
        return Err(CommunityError::Analysis(format!(
            "combined schema input is {total} bytes; workspace limit is {}",
            policy.max_schema_bytes
        )));
    }
    let source_json: serde_json::Value = serde_json::from_slice(&source_bytes)?;
    let target_json: serde_json::Value = serde_json::from_slice(&target_bytes)?;

    let limits = ResourceLimits {
        input_bytes: policy.max_schema_bytes,
        bundle_entries: u64::from(policy.max_documents),
        steps: policy.max_search_steps,
        ..ResourceLimits::defaults()
    };
    let budget = limits.budget();
    let source_schema = parse_schema_document_within(protocol_name, &source_json, &budget)
        .map_err(|e| CommunityError::Analysis(format!("source: {e}")))?;
    let target_schema = parse_schema_document_within(protocol_name, &target_json, &budget)
        .map_err(|e| CommunityError::Analysis(format!("target: {e}")))?;
    let descriptor = panproto_protocols::registry::descriptor(protocol_name)
        .ok_or_else(|| CommunityError::Analysis(format!("unknown protocol `{protocol_name}`")))?;
    let protocol = (descriptor.protocol)();

    let forward_diff = diff(&source_schema, &target_schema);
    let forward = classify(&forward_diff, &protocol);
    let reverse = classify(&diff(&target_schema, &source_schema), &protocol);
    let compatibility = match forward.classification {
        Classification::FullyCompatible => Compatibility::FullyCompatible,
        Classification::BackwardCompatible => Compatibility::BackwardCompatible,
        Classification::Breaking => Compatibility::Breaking,
    };

    let mut changes = Vec::with_capacity(forward.breaking.len() + forward.non_breaking.len());
    changes.extend(forward.breaking.iter().map(|change| ChangeImpact {
        description: humanize_debug(change),
        breaking: true,
    }));
    changes.extend(forward.non_breaking.iter().map(|change| ChangeImpact {
        description: humanize_debug(change),
        breaking: false,
    }));

    let generated = auto_generate(
        &source_schema,
        &target_schema,
        &protocol,
        &AutoLensConfig::default(),
    );
    let optic_class = generated
        .as_ref()
        .ok()
        .map(|result| optic(result.chain.composed_optic_kind()));
    let requires_manual_chain = !forward.breaking.is_empty() && generated.is_err();
    let requires_complement = matches!(optic_class, Some(OpticClass::Lens | OpticClass::Affine));
    let data_loss_risk = requires_complement || requires_manual_chain;

    let existing_records_readable = forward.compatible;
    let old_readers_accept_new_records = reverse.compatible;
    let mut messages = Vec::new();
    if changes.is_empty() {
        messages.push("The two schemas have no compatibility-relevant differences.".to_owned());
    } else if existing_records_readable {
        messages.push("Existing records remain readable under the proposed schema.".to_owned());
    } else {
        messages.push(
            "Some existing records may be rejected by the proposed schema; migration is required."
                .to_owned(),
        );
    }
    if old_readers_accept_new_records {
        messages
            .push("Older readers can accept records written to the proposed schema.".to_owned());
    } else {
        messages.push(
            "Records written to the proposed schema may not be accepted by older readers."
                .to_owned(),
        );
    }
    match optic_class {
        Some(OpticClass::Iso) => messages.push(
            "Panproto derived a reversible isomorphism; no complement data is required.".to_owned(),
        ),
        Some(OpticClass::Prism) => messages.push(
            "Panproto classified the migration as a prism; review which target variants are in scope."
                .to_owned(),
        ),
        Some(OpticClass::Lens) => messages.push(
            "Faithful reversal requires preserving complement data that the target view does not retain."
                .to_owned(),
        ),
        Some(OpticClass::Affine) => messages.push(
            "The migration is partial and complement-bearing; a steward should review failures and retained data."
                .to_owned(),
        ),
        Some(OpticClass::Traversal) => messages.push(
            "The migration may touch multiple values per record; test coverage and cardinality effects."
                .to_owned(),
        ),
        None => messages.push(
            "Panproto could not derive a complete migration chain; the community must author and verify one."
                .to_owned(),
        ),
    }

    let source = SchemaEndpoint {
        protocol: descriptor.name.to_owned(),
        path: source_path.display().to_string(),
        digest: digest(&source_bytes),
    };
    let target = SchemaEndpoint {
        protocol: descriptor.name.to_owned(),
        path: target_path.display().to_string(),
        digest: digest(&target_bytes),
    };
    Ok((
        source,
        target,
        ConsequenceReport {
            compatibility,
            optic_class,
            existing_records_readable,
            old_readers_accept_new_records,
            requires_complement,
            requires_manual_chain,
            data_loss_risk,
            changes,
            messages,
        },
    ))
}

fn read_bounded(path: &Path, max: u64) -> CommunityResult<Vec<u8>> {
    let metadata = fs::metadata(path).map_err(|e| CommunityError::io(path, e))?;
    if metadata.len() > max {
        return Err(CommunityError::Analysis(format!(
            "{} is {} bytes; workspace limit is {max}",
            path.display(),
            metadata.len()
        )));
    }
    fs::read(path).map_err(|e| CommunityError::io(path, e))
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn optic(kind: OpticKind) -> OpticClass {
    match kind {
        OpticKind::Iso => OpticClass::Iso,
        OpticKind::Lens => OpticClass::Lens,
        OpticKind::Prism => OpticClass::Prism,
        OpticKind::Affine => OpticClass::Affine,
        OpticKind::Traversal => OpticClass::Traversal,
    }
}

fn humanize_debug(value: &impl std::fmt::Debug) -> String {
    let raw = format!("{value:?}");
    raw.replace(['{', '}'], "")
        .replace('_', " ")
        .replace("  ", " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_atproto_documents_are_explained() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("demo.json");
        fs::write(
            &path,
            r#"{"lexicon":1,"id":"dev.example.note","defs":{"main":{"type":"record","key":"tid","record":{"type":"object","required":["text"],"properties":{"text":{"type":"string"}}}}}}"#,
        )
        .unwrap();
        let (_, _, report) =
            analyze_schema_change(&path, &path, "atproto", &ResourcePolicy::default()).unwrap();
        assert_eq!(report.compatibility, Compatibility::FullyCompatible);
        assert_eq!(report.optic_class, Some(OpticClass::Iso));
        assert!(!report.data_loss_risk);
    }
}
