//! File-backed community lifecycle commands.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow, bail};
use idiolect_community::{
    ChangePacket, ChangeStatus, CommunitySigningKey, DecisionStatus, GovernanceDecision,
    MigrationFailure, MigrationRun, MigrationStatus, ReleaseArtifact, Review, ReviewStance,
    VerificationEvidence, VerificationOutcome, advance_migration, analyze_schema_change, artifact,
    check_workspace, create_migration_run, create_release, doctor_workspace, export_workspace,
    generate_signing_key, init_workspace, load_manifest, load_packet, load_release, record_review,
    record_verification, repair_workspace, save_json, save_packet, sign_release, verify_release,
};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const DEFAULT_WORKSPACE: &str = ".";

pub fn dispatch(command: &str, args: &[String]) -> Result<ExitCode> {
    match command {
        "init" => cmd_init(args),
        "check" => cmd_check(args),
        "propose" => cmd_propose(args),
        "preview" => cmd_preview(args),
        "review" => cmd_review(args),
        "verify-change" => cmd_verify_change(args),
        "keygen" => cmd_keygen(args),
        "release" => cmd_release(args),
        "migrate" => cmd_migrate(args),
        "export" => cmd_export(args),
        "doctor" => cmd_doctor(args),
        other => bail!("unknown community command: {other}"),
    }
}

fn cmd_init(args: &[String]) -> Result<ExitCode> {
    let parsed = ParsedArgs::parse(args, &[])?;
    parsed.ensure_only(&["name", "did", "workspace"])?;
    let name = parsed.required("name")?;
    let did = parsed.required("did")?;
    let root = parsed.workspace();
    let manifest = init_workspace(&root, name, did)?;
    println!(
        "Created {} for {}.\nNext: add Lexicons under {} and run `idiolect check --workspace {}`.",
        root.join("idiolect.toml").display(),
        manifest.community.name,
        root.join("lexicons").display(),
        root.display()
    );
    Ok(ExitCode::SUCCESS)
}

fn cmd_check(args: &[String]) -> Result<ExitCode> {
    let parsed = ParsedArgs::parse(args, &["json"])?;
    parsed.ensure_only(&["workspace"])?;
    let report = check_workspace(&parsed.workspace())?;
    print_doctor(&report, parsed.present("json"))?;
    Ok(if report.healthy {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn cmd_doctor(args: &[String]) -> Result<ExitCode> {
    let parsed = ParsedArgs::parse(args, &["json", "repair"])?;
    parsed.ensure_only(&["workspace"])?;
    let root = parsed.workspace();
    if parsed.present("repair") {
        let created = repair_workspace(&root)?;
        for path in created {
            println!("created {}", path.display());
        }
    }
    let report = doctor_workspace(&root, true)?;
    print_doctor(&report, parsed.present("json"))?;
    Ok(if report.healthy {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn cmd_propose(args: &[String]) -> Result<ExitCode> {
    let parsed = ParsedArgs::parse(args, &[])?;
    parsed.ensure_only(&[
        "workspace",
        "id",
        "title",
        "summary",
        "author",
        "old",
        "new",
        "protocol",
        "affected",
        "rollback",
        "out",
    ])?;
    let root = parsed.workspace();
    let manifest = load_manifest(&root.join("idiolect.toml"))?;
    let title = parsed.required("title")?.to_owned();
    let summary = parsed.required("summary")?.to_owned();
    let author = parsed.required("author")?.to_owned();
    let old_path = resolve_input(&root, parsed.required("old")?);
    let new_path = resolve_input(&root, parsed.required("new")?);
    let protocol = parsed.one("protocol").unwrap_or("atproto");
    let (source, target, consequences) =
        analyze_schema_change(&old_path, &new_path, protocol, &manifest.resources)?;
    let timestamp = now()?;
    let id = parsed.one("id").map_or_else(
        || {
            format!(
                "{}-{}",
                slug(&title),
                OffsetDateTime::now_utc().unix_timestamp()
            )
        },
        ToOwned::to_owned,
    );
    let verifications = vec![VerificationEvidence {
        kind: "schema-compatibility".to_owned(),
        outcome: VerificationOutcome::Verified,
        tool: format!("idiolect-community/{}", env!("CARGO_PKG_VERSION")),
        evidence: None,
        verified_at: timestamp.clone(),
    }];
    let packet = ChangePacket {
        format_version: 1,
        id: id.clone(),
        community: manifest.community.did,
        title,
        summary,
        author,
        status: ChangeStatus::Review,
        source,
        target,
        consequences,
        affected: parsed.one("affected").map(split_csv).unwrap_or_default(),
        rollback: parsed
            .one("rollback")
            .unwrap_or("Restore the source schema and replay retained records.")
            .to_owned(),
        reviews: Vec::new(),
        decision: GovernanceDecision {
            status: DecisionStatus::Pending,
            reason: "No governance reviews have been recorded.".to_owned(),
            deliberation: None,
        },
        verifications,
        created_at: timestamp.clone(),
        updated_at: timestamp,
    };
    let output = parsed.one("out").map_or_else(
        || root.join(".idiolect/changes").join(format!("{id}.json")),
        PathBuf::from,
    );
    save_packet(&output, &packet)?;
    print_change_summary(&packet);
    println!("\nChange packet: {}", output.display());
    Ok(ExitCode::SUCCESS)
}

fn cmd_preview(args: &[String]) -> Result<ExitCode> {
    let parsed = ParsedArgs::parse(args, &["json"])?;
    parsed.ensure_only(&[])?;
    let path = parsed
        .positionals
        .first()
        .ok_or_else(|| anyhow!("usage: idiolect preview <change-packet> [--json]"))?;
    let packet = load_packet(Path::new(path))?;
    if parsed.present("json") {
        println!("{}", serde_json::to_string_pretty(&packet)?);
    } else {
        print_change_summary(&packet);
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_review(args: &[String]) -> Result<ExitCode> {
    let parsed = ParsedArgs::parse(args, &[])?;
    parsed.ensure_only(&["workspace", "reviewer", "role", "stance", "comment"])?;
    let packet_path = parsed.positionals.first().ok_or_else(|| {
        anyhow!(
            "usage: idiolect review <packet> --reviewer DID --role ROLE --stance approve|reject|abstain"
        )
    })?;
    let root = parsed.workspace();
    let manifest = load_manifest(&root.join("idiolect.toml"))?;
    let mut packet = load_packet(Path::new(packet_path))?;
    let stance = match parsed.required("stance")? {
        "approve" => ReviewStance::Approve,
        "reject" => ReviewStance::Reject,
        "abstain" => ReviewStance::Abstain,
        other => bail!("unknown review stance `{other}`"),
    };
    let decision = record_review(
        &mut packet,
        &manifest,
        Review {
            reviewer: parsed.required("reviewer")?.to_owned(),
            role: parsed.required("role")?.to_owned(),
            stance,
            comment: parsed.one("comment").map(ToOwned::to_owned),
            reviewed_at: now()?,
        },
    )?;
    save_packet(Path::new(packet_path), &packet)?;
    println!("Decision: {:?}\n{}", decision.status, decision.reason);
    Ok(if decision.status == DecisionStatus::Rejected {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    })
}

fn cmd_verify_change(args: &[String]) -> Result<ExitCode> {
    let parsed = ParsedArgs::parse(args, &[])?;
    parsed.ensure_only(&["kind", "outcome", "tool", "evidence"])?;
    let packet_path = parsed.positionals.first().ok_or_else(|| {
        anyhow!(
            "usage: idiolect verify change <packet> --kind KIND --outcome verified|refuted|incomplete --tool TOOL"
        )
    })?;
    let outcome = match parsed.required("outcome")? {
        "verified" => VerificationOutcome::Verified,
        "refuted" => VerificationOutcome::Refuted,
        "incomplete" => VerificationOutcome::Incomplete,
        other => bail!("unknown verification outcome `{other}`"),
    };
    let mut packet = load_packet(Path::new(packet_path))?;
    record_verification(
        &mut packet,
        VerificationEvidence {
            kind: parsed.required("kind")?.to_owned(),
            outcome,
            tool: parsed.required("tool")?.to_owned(),
            evidence: parsed.one("evidence").map(ToOwned::to_owned),
            verified_at: now()?,
        },
    )?;
    save_packet(Path::new(packet_path), &packet)?;
    println!("Recorded {:?} verification on {}", outcome, packet.id);
    Ok(if outcome == VerificationOutcome::Verified {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn cmd_keygen(args: &[String]) -> Result<ExitCode> {
    let parsed = ParsedArgs::parse(args, &[])?;
    parsed.ensure_only(&["workspace", "out"])?;
    let root = parsed.workspace();
    let path = parsed
        .one("out")
        .map_or_else(|| root.join(".idiolect/keys/community.json"), PathBuf::from);
    if path.exists() {
        bail!("refusing to overwrite existing key {}", path.display());
    }
    let key = generate_signing_key();
    save_json(&path, &key)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("restrict permissions on {}", path.display()))?;
    }
    println!(
        "Generated ES256 key at {}. Keep this file private; release bundles contain only its public key.",
        path.display()
    );
    Ok(ExitCode::SUCCESS)
}

fn cmd_release(args: &[String]) -> Result<ExitCode> {
    if args.first().is_some_and(|arg| arg == "verify") {
        return cmd_release_verify(&args[1..]);
    }
    let parsed = ParsedArgs::parse(args, &[])?;
    parsed.ensure_only(&[
        "workspace",
        "version",
        "change",
        "artifact",
        "key",
        "signer",
        "out",
    ])?;
    let root = parsed.workspace();
    let manifest = load_manifest(&root.join("idiolect.toml"))?;
    let signer = parsed.required("signer")?;
    if !manifest.authorities.iter().any(|a| a.did == signer) {
        bail!("release signer {signer} is not a workspace authority");
    }
    let change_paths = parsed.many("change");
    if change_paths.is_empty() {
        bail!("at least one --change <packet.json> is required");
    }
    let mut packets = Vec::new();
    for path in &change_paths {
        packets.push(load_packet(Path::new(path))?);
    }
    let artifacts = load_artifacts(&parsed.many("artifact"))?;
    let version = parsed.required("version")?;
    let mut release = create_release(&manifest, version, &packets, artifacts)?;
    let key_path = parsed.required("key")?;
    let key: CommunitySigningKey = serde_json::from_slice(
        &fs::read(key_path).with_context(|| format!("read signing key {key_path}"))?,
    )?;
    sign_release(&mut release, signer, &key)?;
    let valid = verify_release(&release)?;
    if valid < manifest.release.min_signatures as usize {
        bail!(
            "release has {valid} valid signature(s); policy requires {}",
            manifest.release.min_signatures
        );
    }
    let output = parsed.one("out").map_or_else(
        || {
            root.join(&manifest.release.directory)
                .join(format!("{version}.json"))
        },
        PathBuf::from,
    );
    save_json(&output, &release)?;
    for (path, packet) in change_paths.iter().zip(packets.iter_mut()) {
        packet.status = ChangeStatus::Released;
        packet.updated_at.clone_from(&release.created_at);
        save_packet(Path::new(path), packet)?;
    }
    println!(
        "Released {} with {} change(s), {} artifact(s), and {valid} valid signature(s).\nBundle: {}\nDigest: {}",
        release.version,
        release.changes.len(),
        release.artifacts.len(),
        output.display(),
        release.digest
    );
    Ok(ExitCode::SUCCESS)
}

fn cmd_release_verify(args: &[String]) -> Result<ExitCode> {
    let path = args
        .first()
        .ok_or_else(|| anyhow!("usage: idiolect release verify <release.json>"))?;
    let release = load_release(Path::new(path))?;
    let valid = verify_release(&release)?;
    let required = release.manifest.release.min_signatures as usize;
    println!("Digest valid. {valid} distinct valid signer(s); policy requires {required}.");
    Ok(if valid >= required {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn cmd_migrate(args: &[String]) -> Result<ExitCode> {
    let Some(subcommand) = args.first() else {
        bail!("usage: idiolect migrate <plan|advance|status> ...");
    };
    match subcommand.as_str() {
        "plan" => cmd_migrate_plan(&args[1..]),
        "advance" => cmd_migrate_advance(&args[1..]),
        "status" => cmd_migrate_status(&args[1..]),
        other => bail!("unknown migrate subcommand `{other}`"),
    }
}

fn cmd_migrate_plan(args: &[String]) -> Result<ExitCode> {
    let parsed = ParsedArgs::parse(args, &[])?;
    parsed.ensure_only(&["workspace", "id", "change", "lens", "total", "out"])?;
    let root = parsed.workspace();
    let change = parsed.required("change")?;
    let id = parsed.one("id").map_or_else(
        || format!("migration-{}", OffsetDateTime::now_utc().unix_timestamp()),
        ToOwned::to_owned,
    );
    let total = parsed
        .one("total")
        .map(str::parse)
        .transpose()
        .context("parse --total")?;
    let run = create_migration_run(
        &id,
        change,
        parsed.one("lens").map(ToOwned::to_owned),
        total,
    )?;
    let output = parsed.one("out").map_or_else(
        || root.join(".idiolect/migrations").join(format!("{id}.json")),
        PathBuf::from,
    );
    save_json(&output, &run)?;
    println!("Planned migration {id} at {}", output.display());
    Ok(ExitCode::SUCCESS)
}

fn cmd_migrate_advance(args: &[String]) -> Result<ExitCode> {
    let parsed = ParsedArgs::parse(args, &[])?;
    parsed.ensure_only(&["processed", "failure", "cursor", "status"])?;
    let path = parsed
        .positionals
        .first()
        .ok_or_else(|| anyhow!("usage: idiolect migrate advance <run.json> --processed N"))?;
    let mut run: MigrationRun = serde_json::from_slice(&fs::read(path)?)?;
    let processed = parsed
        .one("processed")
        .unwrap_or("0")
        .parse()
        .context("parse --processed")?;
    let failures = parsed
        .many("failure")
        .into_iter()
        .map(|value| parse_failure(&value))
        .collect::<Result<Vec<_>>>()?;
    let status = parsed
        .one("status")
        .map(parse_migration_status)
        .transpose()?;
    advance_migration(
        &mut run,
        processed,
        &failures,
        parsed.one("cursor").map(ToOwned::to_owned),
        status,
    )?;
    save_json(Path::new(path), &run)?;
    print_migration(&run);
    Ok(if run.status == MigrationStatus::Failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn cmd_migrate_status(args: &[String]) -> Result<ExitCode> {
    let path = args
        .first()
        .ok_or_else(|| anyhow!("usage: idiolect migrate status <run.json>"))?;
    let run: MigrationRun = serde_json::from_slice(&fs::read(path)?)?;
    print_migration(&run);
    Ok(if run.status == MigrationStatus::Failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn cmd_export(args: &[String]) -> Result<ExitCode> {
    let parsed = ParsedArgs::parse(args, &[])?;
    parsed.ensure_only(&["workspace", "out", "release"])?;
    let root = parsed.workspace();
    let destination = PathBuf::from(parsed.required("out")?);
    let release = parsed
        .one("release")
        .map(|path| load_release(Path::new(path)))
        .transpose()?;
    let inventory = export_workspace(&root, &destination, release.as_ref())?;
    println!(
        "Exported {} files for {} to {}.\nInventory: {}",
        inventory.files.len(),
        inventory.community,
        destination.display(),
        destination.join("export-inventory.json").display()
    );
    Ok(ExitCode::SUCCESS)
}

fn print_doctor(report: &idiolect_community::DoctorReport, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    println!(
        "Workspace: {}",
        if report.healthy {
            "healthy"
        } else {
            "needs attention"
        }
    );
    for item in &report.diagnostics {
        println!("  {:?} [{}] {}", item.severity, item.code, item.message);
        if let Some(help) = &item.help {
            println!("    {help}");
        }
    }
    for (kind, count) in &report.inventory {
        println!("  {kind}: {count}");
    }
    Ok(())
}

fn print_change_summary(packet: &ChangePacket) {
    println!("{} ({})", packet.title, packet.id);
    println!("{}", packet.summary);
    println!(
        "Compatibility: {:?}; optic: {}",
        packet.consequences.compatibility,
        packet
            .consequences
            .optic_class
            .map_or_else(|| "not auto-derived".to_owned(), |kind| format!("{kind:?}"))
    );
    for message in &packet.consequences.messages {
        println!("  - {message}");
    }
    if !packet.consequences.changes.is_empty() {
        println!("Detected changes:");
        for change in &packet.consequences.changes {
            println!(
                "  - [{}] {}",
                if change.breaking {
                    "breaking"
                } else {
                    "compatible"
                },
                change.description
            );
        }
    }
    println!(
        "Decision: {:?} — {}",
        packet.decision.status, packet.decision.reason
    );
}

fn print_migration(run: &MigrationRun) {
    println!(
        "{}: {:?}; processed={}, failed={}, total={}",
        run.id,
        run.status,
        run.processed,
        run.failed,
        run.total
            .map_or_else(|| "unknown".to_owned(), |n| n.to_string())
    );
    if let Some(checkpoint) = run.checkpoints.last() {
        println!("Last checkpoint: {}", checkpoint.cursor);
    }
    for failure in &run.failures {
        println!("  failure {}: {}", failure.record, failure.reason);
    }
}

fn load_artifacts(paths: &[String]) -> Result<Vec<ReleaseArtifact>> {
    paths
        .iter()
        .map(|path| {
            let bytes = fs::read(path).with_context(|| format!("read artifact {path}"))?;
            let media_type = if Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            {
                Some("application/json".to_owned())
            } else {
                Some("application/octet-stream".to_owned())
            };
            Ok(artifact(path, &bytes, media_type))
        })
        .collect()
}

fn parse_failure(value: &str) -> Result<MigrationFailure> {
    let (record, reason) = value
        .split_once('=')
        .ok_or_else(|| anyhow!("--failure must be RECORD=REASON"))?;
    Ok(MigrationFailure {
        record: record.to_owned(),
        reason: reason.to_owned(),
    })
}

fn parse_migration_status(value: &str) -> Result<MigrationStatus> {
    Ok(match value {
        "planned" => MigrationStatus::Planned,
        "running" => MigrationStatus::Running,
        "paused" => MigrationStatus::Paused,
        "completed" => MigrationStatus::Completed,
        "failed" => MigrationStatus::Failed,
        "rolled-back" => MigrationStatus::RolledBack,
        other => bail!("unknown migration status `{other}`"),
    })
}

fn now() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .context("format current time")
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "change".to_owned()
    } else {
        out
    }
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn resolve_input(root: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

struct ParsedArgs {
    values: BTreeMap<String, Vec<String>>,
    switches: BTreeSet<String>,
    positionals: Vec<String>,
}

impl ParsedArgs {
    fn parse(args: &[String], switches: &[&str]) -> Result<Self> {
        let switch_names: BTreeSet<&str> = switches.iter().copied().collect();
        let mut values: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut present = BTreeSet::new();
        let mut positionals = Vec::new();
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            if let Some(name) = arg.strip_prefix("--") {
                if switch_names.contains(name) {
                    present.insert(name.to_owned());
                } else {
                    let value = iter
                        .next()
                        .ok_or_else(|| anyhow!("--{name} requires a value"))?;
                    values
                        .entry(name.to_owned())
                        .or_default()
                        .push(value.clone());
                }
            } else {
                positionals.push(arg.clone());
            }
        }
        Ok(Self {
            values,
            switches: present,
            positionals,
        })
    }

    fn one(&self, name: &str) -> Option<&str> {
        self.values
            .get(name)
            .and_then(|values| values.last())
            .map(String::as_str)
    }

    fn many(&self, name: &str) -> Vec<String> {
        self.values.get(name).cloned().unwrap_or_default()
    }

    fn required(&self, name: &str) -> Result<&str> {
        self.one(name)
            .ok_or_else(|| anyhow!("--{name} <value> required"))
    }

    fn present(&self, name: &str) -> bool {
        self.switches.contains(name)
    }

    fn workspace(&self) -> PathBuf {
        PathBuf::from(self.one("workspace").unwrap_or(DEFAULT_WORKSPACE))
    }

    fn ensure_only(&self, allowed: &[&str]) -> Result<()> {
        let allowed: BTreeSet<&str> = allowed.iter().copied().collect();
        if let Some(unknown) = self
            .values
            .keys()
            .find(|key| !allowed.contains(key.as_str()))
        {
            bail!("unknown flag --{unknown}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_flags_are_retained() {
        let args = [
            "--change".to_owned(),
            "a.json".to_owned(),
            "--change".to_owned(),
            "b.json".to_owned(),
        ];
        let parsed = ParsedArgs::parse(&args, &[]).unwrap();
        assert_eq!(parsed.many("change"), ["a.json", "b.json"]);
    }

    #[test]
    fn slugs_are_stable_and_readable() {
        assert_eq!(
            slug("Rename Member → Participant"),
            "rename-member-participant"
        );
    }
}
