//! `idiolect encounter record` — compose a `dev.idiolect.encounter`
//! record from structured prompts.
//!
//! The default flow walks the operator through a structured
//! `ThPurpose`: pick an `action` (either free-typed or from a
//! vocabulary fetched via `--vocab <at-uri>`), an optional material
//! scope, and an optional actor. The `--text-only` flag accepts a
//! single free-text purpose and records it under action=unresolved,
//! with the text preserved in annotations so future corrections can
//! fill in the structure.
//!
//! The subcommand only *emits* the json — it does not publish. Pipe
//! the output into `idiolect fetch` / an oauth session / any atproto
//! record creator.

use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use anyhow::{Context, Result, anyhow, bail};
use idiolect_records::Vocab;
use idiolect_records::generated::defs::{
    LensRef, MaterialSpec, Purpose, SchemaRef, Visibility, VocabRef,
};
use idiolect_records::generated::encounter::EncounterKind;
use std::process::ExitCode;

/// Entry point: `idiolect encounter record [--lens URI] [--source-schema URI]
/// [--target-schema URI] [--vocab AT_URI] [--kind KIND] [--visibility V]
/// [--text-only]`.
pub async fn cmd_encounter_record(args: &[String]) -> Result<ExitCode> {
    let flags = parse_flags(args)?;
    let text_only = flags.contains_key("text-only");

    let lens_uri = flags
        .get("lens")
        .cloned()
        .ok_or_else(|| anyhow!("--lens is required"))?;
    let source_schema_uri = flags
        .get("source-schema")
        .cloned()
        .ok_or_else(|| anyhow!("--source-schema is required"))?;
    let target_schema_uri = flags.get("target-schema").cloned();
    let kind = parse_kind(flags.get("kind").map_or("invocation-log", String::as_str))?;
    let visibility = parse_visibility(
        flags
            .get("visibility")
            .map_or("public-detailed", String::as_str),
    )?;

    let purpose = if text_only {
        prompt_text_only_purpose()?
    } else {
        prompt_structured_purpose(flags.get("vocab").cloned()).await?
    };

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .context("format occurred_at")?;
    let body = serde_json::json!({
        "$type": "dev.idiolect.encounter",
        "lens": LensRef { uri: Some(lens_uri), cid: None, direction: None },
        "sourceSchema": SchemaRef { uri: Some(source_schema_uri), cid: None, language: None },
        "targetSchema": target_schema_uri.map(|u| SchemaRef { uri: Some(u), cid: None, language: None }),
        "purpose": purpose,
        "kind": encounter_kind_wire(kind),
        "visibility": visibility_wire(visibility),
        "occurredAt": now,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&body).expect("serialize encounter")
    );
    Ok(ExitCode::from(0))
}

fn parse_flags(args: &[String]) -> Result<HashMap<String, String>> {
    let mut flags = HashMap::new();
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        let Some(rest) = arg.strip_prefix("--") else {
            bail!("unexpected positional argument: {arg}");
        };
        if rest == "text-only" {
            flags.insert(rest.to_owned(), String::new());
            continue;
        }
        let value = iter
            .next()
            .ok_or_else(|| anyhow!("--{rest} requires a value"))?;
        flags.insert(rest.to_owned(), value.clone());
    }
    Ok(flags)
}

fn parse_kind(raw: &str) -> Result<EncounterKind> {
    match raw {
        "invocation-log" => Ok(EncounterKind::InvocationLog),
        "curated" => Ok(EncounterKind::Curated),
        "roundtrip-verified" => Ok(EncounterKind::RoundtripVerified),
        "production" => Ok(EncounterKind::Production),
        "adversarial" => Ok(EncounterKind::Adversarial),
        other => bail!("unknown --kind: {other}"),
    }
}

const fn encounter_kind_wire(k: EncounterKind) -> &'static str {
    match k {
        EncounterKind::InvocationLog => "invocation-log",
        EncounterKind::Curated => "curated",
        EncounterKind::RoundtripVerified => "roundtrip-verified",
        EncounterKind::Production => "production",
        EncounterKind::Adversarial => "adversarial",
    }
}

fn parse_visibility(raw: &str) -> Result<Visibility> {
    match raw {
        "public-detailed" => Ok(Visibility::PublicDetailed),
        "public-minimal" => Ok(Visibility::PublicMinimal),
        "public-aggregate-only" => Ok(Visibility::PublicAggregateOnly),
        "community-scoped" => Ok(Visibility::CommunityScoped),
        "private" => Ok(Visibility::Private),
        other => bail!("unknown --visibility: {other}"),
    }
}

const fn visibility_wire(v: Visibility) -> &'static str {
    match v {
        Visibility::PublicDetailed => "public-detailed",
        Visibility::PublicMinimal => "public-minimal",
        Visibility::PublicAggregateOnly => "public-aggregate-only",
        Visibility::CommunityScoped => "community-scoped",
        Visibility::Private => "private",
    }
}

/// Walk the operator through an action/material/actor triple.
/// `vocab_uri`, if provided, is fetched via `idiolect_lens` and the
/// action list is presented as a numbered menu.
async fn prompt_structured_purpose(vocab_uri: Option<String>) -> Result<Purpose> {
    let vocab_ref = vocab_uri.as_ref().map(|u| VocabRef {
        uri: Some(u.clone()),
        cid: None,
    });

    let action = if let Some(uri) = vocab_uri.as_deref() {
        let vocab = fetch_vocabulary(uri).await?;
        prompt_action_from_vocabulary(&vocab)?
    } else {
        prompt_free_action()?
    };

    let material_scope =
        prompt_optional("material scope (e.g. classroom_materials) [blank to skip]")?;
    let actor = prompt_optional("actor (e.g. students) [blank to skip]")?;

    Ok(Purpose {
        action,
        material: material_scope.map(|scope| MaterialSpec {
            scope: Some(scope),
            uri: None,
        }),
        actor,
        vocabulary: vocab_ref,
    })
}

/// `--text-only` path: take one free-text line, stash it into
/// `material.uri` so the text is not lost, and mark the structured
/// action as `unresolved` for future corrections.
fn prompt_text_only_purpose() -> Result<Purpose> {
    let text = prompt_required("purpose (free text)")?;
    Ok(Purpose {
        action: "unresolved".to_owned(),
        material: Some(MaterialSpec {
            scope: Some(format!("text:{text}")),
            uri: None,
        }),
        actor: None,
        vocabulary: None,
    })
}

async fn fetch_vocabulary(uri: &str) -> Result<Vocab> {
    use idiolect_identity::{Did, ReqwestIdentityResolver};
    use idiolect_lens::{PdsClient, fetcher_for_did, parse_at_uri};

    let parsed = parse_at_uri(uri).context("parse vocabulary at-uri")?;
    let did = Did::parse(parsed.did()).context("parse DID component")?;
    let resolver = ReqwestIdentityResolver::new();
    let fetcher = fetcher_for_did(&resolver, &did)
        .await
        .context("resolve PDS for vocabulary DID")?;
    let body = fetcher
        .client()
        .get_record(parsed.did(), parsed.collection(), parsed.rkey())
        .await
        .context("fetch vocabulary record")?;
    serde_json::from_value(body).context("decode vocabulary record")
}

fn prompt_action_from_vocabulary(vocab: &Vocab) -> Result<String> {
    let mut stderr = io::stderr().lock();
    writeln!(
        stderr,
        "vocabulary: {} ({} actions)",
        vocab.name,
        vocab.actions.len()
    )?;
    for (i, entry) in vocab.actions.iter().enumerate() {
        writeln!(stderr, "  {:>3}. {}", i + 1, entry.id)?;
    }
    stderr.flush().ok();

    let line = prompt_required("action number or id")?;
    if let Ok(n) = line.parse::<usize>()
        && n >= 1
        && n <= vocab.actions.len()
    {
        return Ok(vocab.actions[n - 1].id.clone());
    }
    // fall through: treat the line as an action id and look it up.
    if let Some(hit) = vocab.actions.iter().find(|e| e.id == line) {
        return Ok(hit.id.clone());
    }
    bail!("no action matching {line:?} in vocabulary {}", vocab.name);
}

fn prompt_free_action() -> Result<String> {
    prompt_required("action (no vocabulary supplied)")
}

fn prompt_required(label: &str) -> Result<String> {
    let line = read_line(label)?;
    if line.is_empty() {
        bail!("{label} is required");
    }
    Ok(line)
}

fn prompt_optional(label: &str) -> Result<Option<String>> {
    let line = read_line(label)?;
    Ok(if line.is_empty() { None } else { Some(line) })
}

fn read_line(label: &str) -> Result<String> {
    let mut stderr = io::stderr().lock();
    write!(stderr, "{label}: ")?;
    stderr.flush()?;
    let stdin = io::stdin();
    let mut line = String::new();
    stdin.lock().read_line(&mut line).context("read stdin")?;
    Ok(line.trim().to_owned())
}
