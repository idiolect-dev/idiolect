//! `idiolect-migrate` — streaming batch migration of JSON records
//! through a published lens.
//!
//! ```text
//! idiolect-migrate \
//!     --lens   AT_URI \
//!     --in     SOURCE_DIR \
//!     --out    TARGET_DIR \
//!    [--pds-url URL]
//! ```
//!
//! Walks every `*.json` file under `SOURCE_DIR`, runs each through
//! `migrate_record` against the live PDS, and writes the result to
//! `TARGET_DIR/<same-name>.json`. Files that fail migration are
//! logged to stderr and skipped; the binary's exit code reflects
//! the worst case (0 if everything succeeded, 1 if any file failed).
//!
//! The working set stays bounded: records stream one at a time.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow, bail};
use idiolect_lens::{PdsResolver, PdsSchemaLoader, ReqwestPdsClient};
use idiolect_migrate::migrate_record;
use panproto_schema::Protocol;

const DEFAULT_PDS_URL: &str = "https://jellybaby.us-east.host.bsky.network";

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();
    match run().await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}

async fn run() -> Result<ExitCode> {
    let mut lens_uri: Option<String> = None;
    let mut in_dir: Option<String> = None;
    let mut out_dir: Option<String> = None;
    let mut pds_url = DEFAULT_PDS_URL.to_owned();

    let mut iter = std::env::args().skip(1);
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--lens" => {
                lens_uri = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("--lens requires a value"))?,
                );
            }
            "--in" => {
                in_dir = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("--in requires a value"))?,
                );
            }
            "--out" => {
                out_dir = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("--out requires a value"))?,
                );
            }
            "--pds-url" => {
                pds_url = iter
                    .next()
                    .ok_or_else(|| anyhow!("--pds-url requires a value"))?;
            }
            "--help" | "-h" => {
                print_help();
                return Ok(ExitCode::from(0));
            }
            other => bail!("unknown flag: {other}"),
        }
    }

    let lens_uri = lens_uri.ok_or_else(|| anyhow!("--lens <at-uri> required"))?;
    let in_dir = PathBuf::from(in_dir.ok_or_else(|| anyhow!("--in <dir> required"))?);
    let out_dir = PathBuf::from(out_dir.ok_or_else(|| anyhow!("--out <dir> required"))?);

    std::fs::create_dir_all(&out_dir).with_context(|| format!("create {}", out_dir.display()))?;

    let client = ReqwestPdsClient::with_service_url(&pds_url);
    let resolver = PdsResolver::new(client.clone());
    let loader = PdsSchemaLoader::new(client);
    let protocol = Protocol::default();

    let mut ok: u32 = 0;
    let mut failed: u32 = 0;
    for entry in std::fs::read_dir(&in_dir).with_context(|| format!("read {}", in_dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        match migrate_one(&resolver, &loader, &protocol, &lens_uri, &path, &out_dir).await {
            Ok(out_path) => {
                ok += 1;
                eprintln!("ok    {} -> {}", path.display(), out_path.display());
            }
            Err(e) => {
                failed += 1;
                eprintln!("fail  {}: {e:#}", path.display());
            }
        }
    }
    eprintln!("\n{ok} succeeded, {failed} failed");
    Ok(if failed == 0 {
        ExitCode::from(0)
    } else {
        ExitCode::from(1)
    })
}

async fn migrate_one(
    resolver: &PdsResolver<ReqwestPdsClient>,
    loader: &PdsSchemaLoader<ReqwestPdsClient>,
    protocol: &Protocol,
    lens_uri: &str,
    src_path: &Path,
    out_dir: &Path,
) -> Result<PathBuf> {
    let bytes = std::fs::read(src_path)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let migrated = migrate_record(resolver, loader, protocol, lens_uri, value).await?;
    let file_name = src_path
        .file_name()
        .ok_or_else(|| anyhow!("no filename component"))?;
    let out_path = out_dir.join(file_name);
    let out_bytes = serde_json::to_vec_pretty(&migrated)?;
    std::fs::write(&out_path, out_bytes)?;
    Ok(out_path)
}

fn print_help() {
    println!(
        "idiolect-migrate {}\n\n\
         streaming batch migration of JSON records through a lens.\n\n\
         usage:\n  \
         idiolect-migrate --lens AT_URI --in SOURCE_DIR --out TARGET_DIR [--pds-url URL]",
        env!("CARGO_PKG_VERSION")
    );
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .try_init();
}
