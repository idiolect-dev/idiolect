//! `idiolect` — command-line tool.
//!
//! Wraps the library crates so operators and end users don't need to
//! write Rust for common operations. Hand-rolled subcommand parser
//! (no clap) to keep compile time and binary size small and matches
//! the convention idiolect-codegen already uses.
//!
//! # Subcommands
//!
//! ```text
//! idiolect resolve <did>                      # DID -> pds, handle
//! idiolect fetch <at-uri>                     # fetch record as json
//! idiolect orchestrator stats [--url URL]
//! idiolect orchestrator bounties [--open] [--requester DID] [--url URL]
//! idiolect orchestrator adapters [--framework NAME] [--url URL]
//! idiolect orchestrator verifications --lens AT_URI [--kind KIND] [--url URL]
//! idiolect encounter record --lens AT_URI --source-schema AT_URI
//!                           [--target-schema AT_URI] [--vocab AT_URI]
//!                           [--kind KIND] [--visibility V] [--text-only]
//! idiolect version
//! idiolect help
//! ```
//!
//! `--url` defaults to `http://localhost:8787`. Responses are
//! pretty-printed JSON.

use std::env;
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow, bail};
use idiolect_identity::{Did, IdentityResolver, ReqwestIdentityResolver};
use idiolect_lens::{RecordFetcher, ReqwestPdsClient, fetcher_for_did};
use tracing_subscriber::EnvFilter;

mod encounter;
mod generated;
mod oauth;
mod publish;
mod util;
mod verify;

const DEFAULT_ORCHESTRATOR_URL: &str = "http://localhost:8787";

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();

    match parse_and_run().await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}

async fn parse_and_run() -> Result<ExitCode> {
    let mut args = env::args().skip(1);
    let Some(subcommand) = args.next() else {
        print_help();
        return Ok(ExitCode::from(0));
    };

    match subcommand.as_str() {
        "resolve" => cmd_resolve(&args.collect::<Vec<_>>()).await,
        "fetch" => cmd_fetch(&args.collect::<Vec<_>>()).await,
        "orchestrator" | "orch" => {
            let nested: Vec<String> = args.collect();
            cmd_orchestrator(&nested).await
        }
        "encounter" => {
            let nested: Vec<String> = args.collect();
            let Some(sub) = nested.first() else {
                bail!("usage: idiolect encounter record ...");
            };
            match sub.as_str() {
                "record" => encounter::cmd_encounter_record(&nested[1..]).await,
                other => bail!("unknown encounter subcommand: {other}"),
            }
        }
        "oauth" => {
            let nested: Vec<String> = args.collect();
            oauth::dispatch(&nested).await
        }
        "publish" => {
            let nested: Vec<String> = args.collect();
            publish::dispatch(&nested).await
        }
        "verify" => {
            let nested: Vec<String> = args.collect();
            verify::dispatch(&nested).await
        }
        "version" | "--version" | "-V" => {
            println!("idiolect {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::from(0))
        }
        "help" | "--help" | "-h" => {
            print_help();
            Ok(ExitCode::from(0))
        }
        other => bail!("unknown subcommand: {other} (try `idiolect help`)"),
    }
}

// -----------------------------------------------------------------
// resolve
// -----------------------------------------------------------------

async fn cmd_resolve(args: &[String]) -> Result<ExitCode> {
    let did_str = args
        .first()
        .ok_or_else(|| anyhow!("usage: idiolect resolve <did>"))?;
    let did = Did::parse(did_str).context("parse DID")?;
    let resolver = ReqwestIdentityResolver::new();
    let doc = resolver.resolve(&did).await.context("resolve DID")?;

    let out = serde_json::json!({
        "did": did.as_str(),
        "method": format!("{:?}", did.method()),
        "handle": doc.handle(),
        "pds_url": doc.pds_url(),
        "also_known_as": doc.also_known_as,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&out).expect("serialize resolve output")
    );
    Ok(ExitCode::from(0))
}

// -----------------------------------------------------------------
// fetch
// -----------------------------------------------------------------

async fn cmd_fetch(args: &[String]) -> Result<ExitCode> {
    let uri = args
        .first()
        .ok_or_else(|| anyhow!("usage: idiolect fetch <at-uri>"))?;
    let parsed = idiolect_lens::AtUri::parse(uri).context("parse at-uri")?;

    let resolver = ReqwestIdentityResolver::new();
    let fetcher: RecordFetcher<ReqwestPdsClient> = fetcher_for_did(&resolver, parsed.did())
        .await
        .context("resolve PDS for DID")?;

    // fetch_at_uri requires a typed R:Record. Here we want the raw
    // body; bypass the typed helper and talk to the underlying PDS
    // client directly.
    use idiolect_lens::PdsClient;
    let body = fetcher
        .client()
        .get_record(
            parsed.did().as_str(),
            parsed.collection().as_str(),
            parsed.rkey(),
        )
        .await
        .context("fetch record")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&body).expect("serialize fetched body")
    );
    Ok(ExitCode::from(0))
}

// -----------------------------------------------------------------
// orchestrator
// -----------------------------------------------------------------

/// Parse `idiolect orchestrator <subcommand...> [--flag value ...] [--url URL]`
/// into (path-segments, flag-map, base-url).
///
/// Subcommand path segments are every leading arg that does not start
/// with `--`. Flags are `--name value` pairs; `--url` is intercepted
/// and used as the base URL rather than a query flag.
fn parse_orchestrator_args(
    args: &[String],
) -> Result<(
    Vec<String>,
    std::collections::HashMap<String, String>,
    String,
)> {
    let mut path = Vec::new();
    let mut flags = std::collections::HashMap::new();
    let mut url = DEFAULT_ORCHESTRATOR_URL.to_owned();

    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        if let Some(rest) = arg.strip_prefix("--") {
            let value = iter
                .next()
                .ok_or_else(|| anyhow!("--{rest} requires a value"))?
                .clone();
            if rest == "url" {
                url = value;
            } else {
                flags.insert(rest.to_owned(), value);
            }
        } else {
            path.push(arg.clone());
        }
    }
    Ok((path, flags, url))
}

async fn cmd_orchestrator(args: &[String]) -> Result<ExitCode> {
    let (path_segments, flags, base_url) = parse_orchestrator_args(args)?;

    // The `stats` subcommand is hand-written — no spec entry, no
    // default / flag binding in the catalog-query taxonomy.
    let path_str = if path_segments == ["stats"] {
        "/v1/stats".to_owned()
    } else {
        let refs: Vec<&str> = path_segments.iter().map(String::as_str).collect();
        crate::generated::dispatch(&refs, &flags)?
    };

    let url = format!("{}{path_str}", base_url.trim_end_matches('/'));
    let resp = reqwest::get(&url)
        .await
        .with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.context("parse response")?;
    if !status.is_success() {
        eprintln!("orchestrator returned {status}: {body}");
        return Ok(ExitCode::from(1));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&body).expect("pretty-print json")
    );
    Ok(ExitCode::from(0))
}

// -----------------------------------------------------------------
// help
// -----------------------------------------------------------------

fn print_help() {
    println!(
        "idiolect {version}\n\
         \n\
         usage: idiolect <subcommand> [args]\n\
         \n\
         top-level subcommands:\n  \
         resolve <did>                          resolve a DID to its PDS url + handle\n  \
         fetch <at-uri>                         fetch a record's body as json\n  \
         orchestrator stats [--url URL]         catalog counts by kind\n  \
         orchestrator <sub> [flags] [--url URL] query a running orchestrator\n  \
         encounter record [flags]               compose an encounter record from structured prompts\n  \
         version                                print version\n  \
         help                                   show this help\n\
         \n\
         {orch_help}\n\
         orchestrator URL defaults to {default}.\n\
         resolve/fetch use plc.directory for `did:plc:*` lookups.",
        version = env!("CARGO_PKG_VERSION"),
        default = DEFAULT_ORCHESTRATOR_URL,
        orch_help = crate::generated::help_text(),
    );
}

// -----------------------------------------------------------------
// tracing
// -----------------------------------------------------------------

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .try_init();
}
