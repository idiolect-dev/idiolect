//! `idiolect-codegen` — lexicon-driven codegen binary.
//!
//! Walks every `.json` file under `lexicons/dev/<family>/` (currently
//! `dev/idiolect/` and the vendored `dev/panproto/` tree), parses each
//! through both the in-house typed view and panproto's
//! `atproto::parse_lexicon`, and (by default) writes:
//!
//! - Rust types to `crates/idiolect-records/src/generated/`
//! - TypeScript types to `packages/schema/src/generated/`
//!
//! This binary exposes a small set of subcommands aimed at appview
//! developers:
//!
//! | subcommand   | purpose                                           |
//! |--------------|---------------------------------------------------|
//! | `generate`   | emit the generated trees (default)                |
//! | `check`      | drift gate: non-zero exit if disk differs         |
//! | `example`    | print a bundled record fixture                    |
//! | `list`       | print nsid + record kind for every lexicon        |
//! | `doctor`     | sanity-check the workspace layout and fixtures    |

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow, bail};
use panproto_protocols::web_document::atproto::parse_lexicon as panproto_parse_lexicon;
use panproto_schema::Schema;

use idiolect_codegen::{Example, emit, lexicon};

fn main() -> ExitCode {
    match run() {
        Ok(exit) => exit,
        Err(err) => {
            eprintln!("idiolect-codegen: {err:#}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ExitCode> {
    let args: Vec<OsString> = env::args_os().collect();
    // legacy compat: `idiolect-codegen --check` keeps working as
    // a synonym for `idiolect-codegen check`.
    let cmd = parse_cli(&args)?;

    let repo_root = resolve_repo_root(cmd.repo_root.as_deref())?;

    match cmd.kind {
        CommandKind::Generate => cmd_generate(&repo_root, false),
        CommandKind::Check => cmd_generate(&repo_root, true),
        CommandKind::Example { nsid } => cmd_example(&repo_root, &nsid),
        CommandKind::List => cmd_list(&repo_root),
        CommandKind::Doctor => cmd_doctor(&repo_root),
        CommandKind::CheckCompat { baseline } => cmd_check_compat(&repo_root, &baseline),
        CommandKind::Help => {
            print_help();
            Ok(ExitCode::SUCCESS)
        }
    }
}

// ---------- subcommands ----------

#[allow(clippy::too_many_lines)]
fn cmd_generate(repo_root: &Path, check_only: bool) -> Result<ExitCode> {
    let lexicons_dir = repo_root.join("lexicons/dev");
    let rust_out = repo_root.join("crates/idiolect-records/src/generated");
    let ts_out = repo_root.join("packages/schema/src/generated");

    let (docs, examples, schemas) = load_workspace(repo_root)?;
    if docs.is_empty() {
        bail!("no lexicons found under {}", lexicons_dir.display());
    }

    let rust_files: Vec<EmittedFile> = emit::emit_rust(&docs, &examples)
        .context("rust emitter")?
        .into_iter()
        .map(|mut f| {
            f.contents = idiolect_codegen::rustfmt_source(&f.contents);
            EmittedFile::from(f)
        })
        .collect();
    let ts_files: Vec<EmittedFile> = emit::emit_typescript(&docs, &examples)
        .context("typescript emitter")?
        .into_iter()
        .map(EmittedFile::from)
        .collect();

    if check_only {
        let rust_drift = check_drift(&rust_out, &rust_files)?;
        let ts_drift = check_drift(&ts_out, &ts_files)?;
        if rust_drift || ts_drift {
            eprintln!(
                "idiolect-codegen: generated sources are out of sync with lexicons. \
                 run `cargo run -p idiolect-codegen` and commit the result."
            );
            return Ok(ExitCode::from(1));
        }
        println!(
            "idiolect-codegen: generated sources match lexicons \
             ({} rust, {} typescript, {} panproto schemas, {} fixtures).",
            rust_files.len(),
            ts_files.len(),
            schemas.len(),
            examples.len(),
        );
        return Ok(ExitCode::SUCCESS);
    }

    write_generated(&rust_out, &rust_files)?;
    write_generated(&ts_out, &ts_files)?;

    // Spec-driven codegen. Runs after idiolect-records so the
    // generated Rust types the spec references are in place.
    let orch_lex_path = repo_root.join("orchestrator-spec/lexicon.json");
    let orch_spec_path = repo_root.join("orchestrator-spec/queries.json");
    let (orch_generated_count, cli_generated_count) = if orch_spec_path.exists() {
        let spec =
            idiolect_codegen::spec_driven::orchestrator::load_spec(&orch_lex_path, &orch_spec_path)
                .with_context(|| format!("load {}", orch_spec_path.display()))?;
        let orch_src = repo_root.join("crates/idiolect-orchestrator/src");
        let orch_written = idiolect_codegen::spec_driven::orchestrator::emit_all(&spec, &orch_src)
            .context("orchestrator codegen")?;
        let lexicons_root = repo_root.join("lexicons");
        let xrpc_written =
            idiolect_codegen::spec_driven::orchestrator::emit_xrpc_lexicons(&spec, &lexicons_root)
                .context("orchestrator xrpc lexicon emission")?;
        let cli_src = repo_root.join("crates/idiolect-cli/src");
        let cli_written =
            idiolect_codegen::spec_driven::cli::emit(&spec, &cli_src).context("cli codegen")?;
        (orch_written.len() + xrpc_written.len(), cli_written.len())
    } else {
        (0, 0)
    };

    let obs_lex_path = repo_root.join("observer-spec/lexicon.json");
    let obs_spec_path = repo_root.join("observer-spec/methods.json");
    let obs_generated_count = if obs_spec_path.exists() {
        let spec =
            idiolect_codegen::spec_driven::observer::load_spec(&obs_lex_path, &obs_spec_path)
                .with_context(|| format!("load {}", obs_spec_path.display()))?;
        let obs_src = repo_root.join("crates/idiolect-observer/src");
        let written = idiolect_codegen::spec_driven::observer::emit(&spec, &obs_src)
            .context("observer codegen")?;
        written.len()
    } else {
        0
    };

    let verify_lex_path = repo_root.join("verify-spec/lexicon.json");
    let verify_spec_path = repo_root.join("verify-spec/runners.json");
    let verify_generated_count = if verify_spec_path.exists() {
        let spec =
            idiolect_codegen::spec_driven::verify::load_spec(&verify_lex_path, &verify_spec_path)
                .with_context(|| format!("load {}", verify_spec_path.display()))?;
        let verify_src = repo_root.join("crates/idiolect-verify/src");
        let written = idiolect_codegen::spec_driven::verify::emit(&spec, &verify_src)
            .context("verify codegen")?;
        written.len()
    } else {
        0
    };

    println!(
        "idiolect-codegen: wrote {} rust files to {} and {} typescript files to {} \
         (fixtures: {}, orchestrator: {}, cli: {}, observer: {}, verify: {}).",
        rust_files.len(),
        rust_out.display(),
        ts_files.len(),
        ts_out.display(),
        examples.len(),
        orch_generated_count,
        cli_generated_count,
        obs_generated_count,
        verify_generated_count,
    );
    Ok(ExitCode::SUCCESS)
}

fn cmd_example(repo_root: &Path, nsid_or_kind: &str) -> Result<ExitCode> {
    let (docs, examples, _) = load_workspace(repo_root)?;
    // accept either a full nsid or a short kind ("encounter").
    let wanted = normalise_nsid(&docs, nsid_or_kind)?;
    let Some(ex) = examples.iter().find(|e| e.nsid == wanted) else {
        bail!(
            "no example fixture for {}. bundled fixtures: {}",
            wanted,
            examples
                .iter()
                .map(|e| e.nsid.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    };
    print!("{}", ex.json);
    if !ex.json.ends_with('\n') {
        println!();
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_list(repo_root: &Path) -> Result<ExitCode> {
    let (docs, examples, _) = load_workspace(repo_root)?;
    let fixture_nsids: std::collections::HashSet<&str> =
        examples.iter().map(|e| e.nsid.as_str()).collect();
    let mut max_nsid_width = 0usize;
    for d in &docs {
        max_nsid_width = max_nsid_width.max(d.nsid.len());
    }
    println!(
        "{:<width$}  KIND        FIXTURE",
        "NSID",
        width = max_nsid_width
    );
    for d in &docs {
        let kind = match d.defs.get("main") {
            Some(lexicon::Def::Record(_)) => "record",
            _ => "(defs)",
        };
        let fixture = if fixture_nsids.contains(d.nsid.as_str()) {
            "yes"
        } else {
            "-"
        };
        println!(
            "{:<width$}  {:<10}  {}",
            d.nsid,
            kind,
            fixture,
            width = max_nsid_width
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_doctor(repo_root: &Path) -> Result<ExitCode> {
    let lexicons_root = repo_root.join("lexicons/dev");
    let idiolect_dir = lexicons_root.join("idiolect");
    let panproto_dir = lexicons_root.join("panproto");
    let rust_out = repo_root.join("crates/idiolect-records/src/generated");
    let ts_out = repo_root.join("packages/schema/src/generated");

    println!("idiolect-codegen doctor");
    println!("  repo root:      {}", repo_root.display());
    report_path("lexicons root", &lexicons_root);
    report_path("dev.idiolect", &idiolect_dir);
    report_path("dev.panproto", &panproto_dir);
    report_path("rust out", &rust_out);
    report_path("ts out", &ts_out);

    let (docs, examples, schemas) = load_workspace(repo_root)?;
    println!("  lexicons:       {}", docs.len());
    let record_count = docs
        .iter()
        .filter(|d| matches!(d.defs.get("main"), Some(lexicon::Def::Record(_))))
        .count();
    println!("  record defs:    {record_count}");
    println!("  fixtures:       {}", examples.len());
    println!("  panproto ok:    {}", schemas.len());

    // warn about first-party (`dev.idiolect.*`) records lacking fixtures;
    // the vendored `dev.panproto.*` tree gets a separate report line and
    // isn't required to ship fixtures alongside its schemas.
    let have: std::collections::HashSet<&str> = examples.iter().map(|e| e.nsid.as_str()).collect();
    let missing: Vec<&str> = docs
        .iter()
        .filter(|d| matches!(d.defs.get("main"), Some(lexicon::Def::Record(_))))
        .filter(|d| lexicon::family_of_nsid(&d.nsid) == Some("idiolect"))
        .map(|d| d.nsid.as_str())
        .filter(|n| !have.contains(n))
        .collect();
    if missing.is_empty() {
        println!("  all first-party records have bundled fixtures.");
    } else {
        println!(
            "  warn: {} first-party record(s) lack fixtures: {}",
            missing.len(),
            missing.join(", ")
        );
    }

    let vendored = docs
        .iter()
        .filter(|d| lexicon::family_of_nsid(&d.nsid) != Some("idiolect"))
        .count();
    if vendored > 0 {
        println!("  vendored:       {vendored} (dev.panproto.*)");
    }
    Ok(ExitCode::SUCCESS)
}

// ---------- shared loader ----------

/// Load every lexicon + fixture + panproto schema from the workspace.
/// Walks `lexicons/dev/<family>/**/*.json` (skipping `examples/`
/// subtrees) so both the first-party `dev.idiolect.*` records and the
/// vendored `dev.panproto.*` tree come through the same pipeline.
/// Returned lexicons are sorted by nsid for stable codegen output.
#[allow(clippy::type_complexity)]
/// Compare the current lexicon tree against a baseline copy and
/// classify every diff through `panproto_check::classify`. Exit 1 if
/// any schema's diff is not backward-compatible; 0 otherwise.
///
/// The baseline is a directory shaped like `lexicons/dev/` — usually
/// produced by a CI step that extracts `lexicons/dev/` from the merge
/// base commit.
#[allow(clippy::too_many_lines)]
fn cmd_check_compat(repo_root: &Path, baseline_root: &Path) -> Result<ExitCode> {
    use panproto_check::{classify, diff as schema_diff};
    use panproto_protocols::web_document::atproto::protocol as atproto_protocol;

    let current_root = repo_root.join("lexicons/dev");
    if !current_root.is_dir() {
        bail!(
            "current lexicons root does not exist: {}",
            current_root.display()
        );
    }
    if !baseline_root.is_dir() {
        bail!(
            "baseline lexicons root does not exist: {}",
            baseline_root.display()
        );
    }

    let current_files = discover_lexicons(&current_root)?;
    let baseline_files = discover_lexicons(baseline_root)?;

    fn parse_tree(files: &[PathBuf]) -> Result<BTreeMap<String, Schema>> {
        let mut out = BTreeMap::new();
        for path in files {
            let raw =
                fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
            let json: serde_json::Value = serde_json::from_str(&raw)
                .with_context(|| format!("parsing json in {}", path.display()))?;
            let doc = lexicon::parse(&json)
                .with_context(|| format!("parsing lexicon {}", path.display()))?;
            let schema = panproto_parse_lexicon(&json).map_err(|e| {
                anyhow!("panproto parse_lexicon failed for {}: {e}", path.display())
            })?;
            out.insert(doc.nsid, schema);
        }
        Ok(out)
    }

    let current = parse_tree(&current_files)?;
    let baseline = parse_tree(&baseline_files)?;
    let protocol = atproto_protocol();

    let mut any_breaking = false;
    let mut total_breaking = 0usize;
    let mut total_non_breaking = 0usize;
    let mut removed_nsids: Vec<String> = Vec::new();
    let mut added_nsids: Vec<String> = Vec::new();

    println!("idiolect check-compat");
    println!("  current:  {}", current_root.display());
    println!("  baseline: {}", baseline_root.display());

    // Every nsid in baseline must still exist in current unless the
    // project is intentionally removing a lexicon (always breaking).
    for nsid in baseline.keys() {
        if !current.contains_key(nsid) {
            removed_nsids.push(nsid.clone());
        }
    }
    for nsid in current.keys() {
        if !baseline.contains_key(nsid) {
            added_nsids.push(nsid.clone());
        }
    }

    // Classify every shared nsid.
    for (nsid, new_schema) in &current {
        let Some(old_schema) = baseline.get(nsid) else {
            continue;
        };
        let diff = schema_diff(old_schema, new_schema);
        let report = classify(&diff, &protocol);
        total_breaking += report.breaking.len();
        total_non_breaking += report.non_breaking.len();
        if !report.compatible {
            any_breaking = true;
            println!("  ✗ {nsid}: BREAKING");
            for change in &report.breaking {
                println!("      - {change:?}");
            }
        } else if !report.non_breaking.is_empty() {
            println!(
                "  ✓ {nsid}: compatible ({} non-breaking change(s))",
                report.non_breaking.len()
            );
        }
    }

    if !removed_nsids.is_empty() {
        any_breaking = true;
        println!("  ✗ removed lexicons (BREAKING):");
        for nsid in &removed_nsids {
            println!("      - {nsid}");
        }
    }
    if !added_nsids.is_empty() {
        println!("  + added lexicons (non-breaking):");
        for nsid in &added_nsids {
            println!("      - {nsid}");
        }
    }

    println!(
        "  summary: breaking={} non_breaking={} removed={} added={}",
        total_breaking,
        total_non_breaking,
        removed_nsids.len(),
        added_nsids.len(),
    );

    if any_breaking {
        println!("  result: BREAKING CHANGES DETECTED");
        Ok(ExitCode::from(1))
    } else {
        println!("  result: backward-compatible");
        Ok(ExitCode::from(0))
    }
}

type WorkspaceLoad = (
    Vec<lexicon::LexiconDoc>,
    Vec<Example>,
    BTreeMap<String, Schema>,
);

fn load_workspace(repo_root: &Path) -> Result<WorkspaceLoad> {
    let lexicons_root = repo_root.join("lexicons/dev");
    let lexicon_files = discover_lexicons(&lexicons_root)?;

    let mut docs = Vec::with_capacity(lexicon_files.len());
    let mut schemas: BTreeMap<String, Schema> = BTreeMap::new();
    for path in &lexicon_files {
        let raw =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let json: serde_json::Value = serde_json::from_str(&raw)
            .with_context(|| format!("parsing json in {}", path.display()))?;
        let doc =
            lexicon::parse(&json).with_context(|| format!("parsing lexicon {}", path.display()))?;
        let schema = panproto_parse_lexicon(&json)
            .map_err(|e| anyhow!("panproto parse_lexicon failed for {}: {e}", path.display()))?;
        schemas.insert(doc.nsid.clone(), schema);
        docs.push(doc);
    }
    docs.sort_by(|a, b| a.nsid.cmp(&b.nsid));

    let examples = discover_all_examples(&lexicons_root, repo_root, &docs)?;
    Ok((docs, examples, schemas))
}

// ---------- cli parsing ----------

struct Cli {
    kind: CommandKind,
    repo_root: Option<PathBuf>,
}

enum CommandKind {
    Generate,
    Check,
    Example {
        nsid: String,
    },
    List,
    Doctor,
    /// Diff the lexicon tree against a `--baseline <dir>` copy and
    /// report breaking / non-breaking changes. Exit code 1 on any
    /// breaking change; 0 otherwise.
    CheckCompat {
        baseline: PathBuf,
    },
    Help,
}

fn parse_cli(args: &[OsString]) -> Result<Cli> {
    let mut iter = args.iter().skip(1);
    let mut repo_root = None;
    let mut kind: Option<CommandKind> = None;
    let mut positional: Vec<String> = Vec::new();

    let mut baseline: Option<PathBuf> = None;
    while let Some(raw) = iter.next() {
        let s = raw
            .to_str()
            .ok_or_else(|| anyhow!("non-utf8 argument"))?
            .to_owned();
        match s.as_str() {
            "--check" => {
                // legacy compat
                if kind.is_none() {
                    kind = Some(CommandKind::Check);
                }
            }
            "--repo-root" => {
                let v = iter
                    .next()
                    .ok_or_else(|| anyhow!("--repo-root requires a value"))?;
                repo_root = Some(PathBuf::from(v));
            }
            "--baseline" => {
                let v = iter
                    .next()
                    .ok_or_else(|| anyhow!("--baseline requires a value"))?;
                baseline = Some(PathBuf::from(v));
            }
            "-h" | "--help" => kind = Some(CommandKind::Help),
            other if other.starts_with('-') => bail!("unknown flag: {other}"),
            other => {
                if kind.is_none() {
                    kind = Some(parse_subcommand(other)?);
                } else {
                    positional.push(other.to_owned());
                }
            }
        }
    }

    let kind = match kind {
        // `example <nsid>`: subcommand parsing left the nsid empty; the
        // actual value landed in `positional` on the next iteration.
        Some(CommandKind::Example { .. }) => {
            let nsid = positional
                .first()
                .ok_or_else(|| anyhow!("`example` requires an nsid or record kind argument"))?
                .clone();
            CommandKind::Example { nsid }
        }
        Some(CommandKind::CheckCompat { .. }) => {
            let baseline =
                baseline.ok_or_else(|| anyhow!("`check-compat` requires --baseline <path>"))?;
            CommandKind::CheckCompat { baseline }
        }
        Some(other) => other,
        None => CommandKind::Generate,
    };

    Ok(Cli { kind, repo_root })
}

fn parse_subcommand(s: &str) -> Result<CommandKind> {
    Ok(match s {
        "generate" | "gen" => CommandKind::Generate,
        "check" => CommandKind::Check,
        "example" | "ex" => CommandKind::Example {
            nsid: String::new(),
        },
        "list" | "ls" => CommandKind::List,
        "doctor" | "dr" => CommandKind::Doctor,
        "check-compat" => CommandKind::CheckCompat {
            baseline: PathBuf::new(),
        },
        "help" => CommandKind::Help,
        other => bail!("unknown subcommand: {other}"),
    })
}

fn print_help() {
    println!(
        "idiolect-codegen — lexicon-driven codegen for dev.idiolect.*\n\
         \n\
         usage: idiolect-codegen [--repo-root PATH] <subcommand> [args]\n\
         \n\
         subcommands:\n  \
         generate          emit rust + typescript sources (default)\n  \
         check             drift gate: non-zero exit if disk differs\n  \
         example <nsid>    print a bundled record fixture to stdout\n  \
         list              list every lexicon with its kind and fixture status\n  \
         doctor            sanity-check workspace layout and fixtures\n  \
         check-compat --baseline PATH   classify lexicon changes vs baseline; exit 1 on breaking changes\n  \
         help              show this help\n\
         \n\
         example nsids may be shortened to the kind, e.g. `example encounter`."
    );
}

// ---------- discovery helpers ----------

fn resolve_repo_root(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p.to_owned());
    }
    let start = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .or_else(|| env::current_dir().ok())
        .ok_or_else(|| anyhow!("cannot determine starting directory"))?;
    let mut cur: &Path = start.as_path();
    loop {
        if cur.join("lexicons/dev").is_dir() {
            return Ok(cur.to_owned());
        }
        match cur.parent() {
            Some(parent) => cur = parent,
            None => bail!(
                "could not find workspace root (no `lexicons/dev` \
                 dir in any ancestor of {})",
                start.display()
            ),
        }
    }
}

/// Walk every `.json` lexicon under `lexicons/dev/<family>/...`,
/// skipping any directory named `examples` (that subtree is for
/// fixtures, not schemas). Recurses so `dev/panproto/schema/` and
/// `dev/panproto/vcs/` both get picked up.
fn discover_lexicons(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk_lexicons(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk_lexicons(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            // fixtures live under `examples/`; skip that subtree on the
            // schema-discovery pass.
            if path.file_name().and_then(|s| s.to_str()) == Some("examples") {
                continue;
            }
            walk_lexicons(&path, out)?;
        } else if file_type.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
            out.push(path);
        }
    }
    Ok(())
}

/// Collect every fixture under `lexicons/dev/<family>/examples/*.json`.
/// Fixture filename resolution is scoped by family: a fixture
/// `examples/lens.json` under `dev/panproto/` matches an nsid whose
/// last dot-segment is `lens` AND whose family is `panproto`. That
/// keeps lookups unambiguous if two families ever export a same-named
/// record.
fn discover_all_examples(
    lexicons_root: &Path,
    repo_root: &Path,
    docs: &[lexicon::LexiconDoc],
) -> Result<Vec<Example>> {
    let mut out = Vec::new();
    if !lexicons_root.is_dir() {
        return Ok(out);
    }
    for entry in fs::read_dir(lexicons_root)
        .with_context(|| format!("reading {}", lexicons_root.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let family_dir = entry.path();
        let examples_dir = family_dir.join("examples");
        if !examples_dir.is_dir() {
            continue;
        }
        let family = family_dir
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("non-utf8 family dir: {}", family_dir.display()))?;
        collect_examples_for_family(&examples_dir, repo_root, family, docs, &mut out)?;
    }
    out.sort_by(|a, b| a.nsid.cmp(&b.nsid));
    Ok(out)
}

fn collect_examples_for_family(
    examples_dir: &Path,
    repo_root: &Path,
    family: &str,
    docs: &[lexicon::LexiconDoc],
    out: &mut Vec<Example>,
) -> Result<()> {
    // build a (family, last-segment) → nsid map, e.g.
    //   ("idiolect", "encounter")  → "dev.idiolect.encounter"
    //   ("panproto", "lens")       → "dev.panproto.schema.lens"
    let family_kind_to_nsid: BTreeMap<String, String> = docs
        .iter()
        .filter(|d| lexicon::family_of_nsid(&d.nsid) == Some(family))
        .map(|d| {
            let last = d
                .nsid
                .rsplit('.')
                .next()
                .unwrap_or(d.nsid.as_str())
                .to_owned();
            (last, d.nsid.clone())
        })
        .collect();

    for entry in
        fs::read_dir(examples_dir).with_context(|| format!("reading {}", examples_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !(path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json")) {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("non-utf8 fixture filename: {}", path.display()))?
            .to_owned();
        let Some(nsid) = family_kind_to_nsid.get(&stem).cloned() else {
            bail!(
                "fixture {} has no matching lexicon in the `{}` family (expected one of: {})",
                path.display(),
                family,
                family_kind_to_nsid
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        };
        let json = fs::read_to_string(&path)
            .with_context(|| format!("reading fixture {}", path.display()))?;
        let relative = path
            .strip_prefix(repo_root)
            .with_context(|| {
                format!(
                    "fixture {} is outside the workspace root {}",
                    path.display(),
                    repo_root.display()
                )
            })?
            .to_string_lossy()
            .replace('\\', "/");
        out.push(Example {
            nsid,
            repo_relative_path: relative,
            json,
        });
    }
    Ok(())
}

fn normalise_nsid(docs: &[lexicon::LexiconDoc], needle: &str) -> Result<String> {
    if docs.iter().any(|d| d.nsid == needle) {
        return Ok(needle.to_owned());
    }
    let candidates: Vec<&str> = docs
        .iter()
        .map(|d| d.nsid.as_str())
        .filter(|n| n.rsplit('.').next() == Some(needle))
        .collect();
    match candidates.as_slice() {
        [only] => Ok((*only).to_owned()),
        [] => bail!("no lexicon matches {needle}"),
        many => bail!("ambiguous kind {needle}: {}", many.join(", ")),
    }
}

fn report_path(label: &str, path: &Path) {
    let state = if path.exists() { "ok" } else { "MISSING" };
    println!("  {label:<14}  {} ({})", path.display(), state);
}

// ---------- write + drift ----------

fn write_generated(out_dir: &Path, files: &[EmittedFile]) -> Result<()> {
    fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;

    // remove stale `@generated` files the current pass didn't produce.
    let keep: std::collections::HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    for entry in fs::read_dir(out_dir).with_context(|| format!("reading {}", out_dir.display()))? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if entry.file_type()?.is_file() && !keep.contains(name_str.as_ref()) {
            let victim = entry.path();
            let head = fs::read_to_string(&victim).unwrap_or_default();
            if head.starts_with("// @generated by idiolect-codegen") {
                fs::remove_file(&victim)
                    .with_context(|| format!("removing stale {}", victim.display()))?;
            }
        }
    }

    for f in files {
        let full = out_dir.join(&f.path);
        let existing = fs::read_to_string(&full).ok();
        if existing.as_deref() == Some(f.contents.as_str()) {
            continue;
        }
        fs::write(&full, &f.contents).with_context(|| format!("writing {}", full.display()))?;
    }
    Ok(())
}

fn check_drift(out_dir: &Path, files: &[EmittedFile]) -> Result<bool> {
    let mut drift = false;
    for f in files {
        let full = out_dir.join(&f.path);
        let existing = fs::read_to_string(&full).ok();
        match existing {
            Some(content) if content == f.contents => {}
            Some(_) => {
                eprintln!("drift: {}", full.display());
                drift = true;
            }
            None => {
                eprintln!("missing: {}", full.display());
                drift = true;
            }
        }
    }

    if out_dir.is_dir() {
        let emitted: std::collections::HashSet<&str> =
            files.iter().map(|f| f.path.as_str()).collect();
        for entry in
            fs::read_dir(out_dir).with_context(|| format!("reading {}", out_dir.display()))?
        {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !emitted.contains(name_str.as_ref()) {
                let head = fs::read_to_string(entry.path()).unwrap_or_default();
                if head.starts_with("// @generated by idiolect-codegen") {
                    eprintln!("stale: {}", entry.path().display());
                    drift = true;
                }
            }
        }
    }
    Ok(drift)
}

/// Uniform emitted-file view so `write_generated` and `check_drift`
/// don't need to care which emitter produced the output.
struct EmittedFile {
    path: String,
    contents: String,
}

impl From<idiolect_codegen::target::EmittedFile> for EmittedFile {
    fn from(f: idiolect_codegen::target::EmittedFile) -> Self {
        Self {
            path: f.path,
            contents: f.contents,
        }
    }
}
