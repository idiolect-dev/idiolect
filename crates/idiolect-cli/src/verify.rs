//! `idiolect verify` — run a shipped verification runner.
//!
//! ```text
//! idiolect verify roundtrip-test       --lens AT_URI [--corpus PATH]   [--pds-url URL] [--verifier-did DID]
//! idiolect verify property-test        --lens AT_URI  --corpus PATH    [--budget N]    [--pds-url URL] [--verifier-did DID]
//! idiolect verify static-check         --lens AT_URI                   [--pds-url URL] [--verifier-did DID]
//! idiolect verify coercion-law         --lens AT_URI  --vcs-url URL    --standard STD  [--version V] [--violation-threshold N] [--verifier-did DID]
//! ```
//!
//! Wraps each shipped `VerificationRunner` for command-line use.
//! Prints the resulting `Verification` record as JSON; falsifying /
//! inconclusive runs exit with a non-zero status so CI surfaces them.

#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::process::ExitCode;

use anyhow::{Context, Result, anyhow, bail};
use idiolect_lens::{LensError, PdsResolver, PdsSchemaLoader, ReqwestPdsClient};
use idiolect_records::generated::dev::idiolect::defs::LensRef;
use idiolect_records::generated::dev::idiolect::verification::{Verification, VerificationResult};
use idiolect_verify::{
    CoercionLawClient, CoercionLawRunner, CoercionLawViolation, PropertyTestRunner,
    RoundtripTestRunner, StaticCheckRunner, VerificationRunner, VerificationTarget,
};
use panproto_schema::Protocol;
use serde::Serialize;

use crate::util::now_datetime;

const DEFAULT_PDS_URL: &str = "https://jellybaby.us-east.host.bsky.network";
const DEFAULT_PROPERTY_BUDGET: u32 = 100;

pub async fn dispatch(args: &[String]) -> Result<ExitCode> {
    let Some(kind) = args.first() else {
        bail!(
            "usage: idiolect verify <kind> [flags]\n\
             kinds: roundtrip-test | property-test | static-check | coercion-law"
        );
    };
    let rest = &args[1..];
    match kind.as_str() {
        "roundtrip-test" => cmd_roundtrip(rest).await,
        "property-test" => cmd_property_test(rest).await,
        "static-check" => cmd_static_check(rest).await,
        "coercion-law" => cmd_coercion_law(rest).await,
        other => bail!(
            "unknown verify kind: {other}\n\
             kinds: roundtrip-test | property-test | static-check | coercion-law"
        ),
    }
}

// -----------------------------------------------------------------
// shared
// -----------------------------------------------------------------

/// Common flag set for runners that talk to a PDS.
struct PdsFlags {
    lens: String,
    pds_url: String,
    verifier_did: String,
}

fn parse_pds_flags(args: &[String]) -> Result<(PdsFlags, Vec<String>)> {
    let mut lens: Option<String> = None;
    let mut pds_url = DEFAULT_PDS_URL.to_owned();
    let mut verifier_did = "did:plc:unknown".to_owned();
    let mut leftover = Vec::new();

    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--lens" | "--lens_uri" => {
                lens = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("--lens requires a value"))?
                        .clone(),
                );
            }
            "--pds-url" => {
                pds_url.clone_from(
                    iter.next()
                        .ok_or_else(|| anyhow!("--pds-url requires a value"))?,
                );
            }
            "--verifier-did" => {
                verifier_did.clone_from(
                    iter.next()
                        .ok_or_else(|| anyhow!("--verifier-did requires a value"))?,
                );
            }
            other => {
                leftover.push(other.to_owned());
                if let Some(value) = iter.next() {
                    leftover.push(value.clone());
                }
            }
        }
    }
    let lens = lens.ok_or_else(|| anyhow!("--lens <at-uri> required"))?;
    Ok((
        PdsFlags {
            lens,
            pds_url,
            verifier_did,
        },
        leftover,
    ))
}

fn target_from(pds: &PdsFlags) -> Result<VerificationTarget> {
    Ok(VerificationTarget {
        lens: LensRef {
            uri: Some(pds.lens.parse().context("parse --lens at-uri")?),
            cid: None,
            direction: None,
        },
        verifier: pds
            .verifier_did
            .parse()
            .context("parse --verifier-did")?,
        occurred_at: now_datetime()?,
        tool_override: None,
    })
}

fn pds_resolver_loader(pds_url: &str) -> (PdsResolver<ReqwestPdsClient>, PdsSchemaLoader<ReqwestPdsClient>) {
    let client = ReqwestPdsClient::with_service_url(pds_url);
    (PdsResolver::new(client.clone()), PdsSchemaLoader::new(client))
}

fn print_and_exit(verification: &Verification) -> Result<ExitCode> {
    let json = serde_json::to_string_pretty(verification)?;
    println!("{json}");
    Ok(match verification.result {
        VerificationResult::Holds => ExitCode::from(0),
        VerificationResult::Falsified
        | VerificationResult::Inconclusive
        | VerificationResult::Other(_) => ExitCode::from(1),
    })
}

/// Read a corpus file. The file may be a JSON array of record bodies
/// or JSON Lines (one record per line).
fn load_corpus(path: &str) -> Result<Vec<serde_json::Value>> {
    let bytes = std::fs::read(path).with_context(|| format!("read corpus {path}"))?;
    if let Ok(serde_json::Value::Array(items)) =
        serde_json::from_slice::<serde_json::Value>(&bytes)
    {
        return Ok(items);
    }
    let mut items = Vec::new();
    for (i, line) in std::str::from_utf8(&bytes)
        .context("corpus is not UTF-8")?
        .lines()
        .enumerate()
    {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(trimmed)
            .with_context(|| format!("parse corpus line {}", i + 1))?;
        items.push(value);
    }
    if items.is_empty() {
        bail!("corpus at {path} parsed to zero records");
    }
    Ok(items)
}

// -----------------------------------------------------------------
// roundtrip-test
// -----------------------------------------------------------------

async fn cmd_roundtrip(args: &[String]) -> Result<ExitCode> {
    let (pds, rest) = parse_pds_flags(args)?;
    let mut corpus_path: Option<String> = None;
    let mut iter = rest.iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--corpus" => {
                corpus_path = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("--corpus requires a value"))?
                        .clone(),
                );
            }
            other => bail!("unknown flag: {other}"),
        }
    }
    let corpus = match corpus_path.as_deref() {
        Some(p) => load_corpus(p)?,
        None => vec![serde_json::json!({ "text": "" })],
    };
    let (resolver, loader) = pds_resolver_loader(&pds.pds_url);
    let runner = RoundtripTestRunner::new(resolver, loader, Protocol::default(), corpus);
    let target = target_from(&pds)?;
    let verification = runner
        .run(&target)
        .await
        .context("RoundtripTestRunner::run")?;
    print_and_exit(&verification)
}

// -----------------------------------------------------------------
// property-test
// -----------------------------------------------------------------

async fn cmd_property_test(args: &[String]) -> Result<ExitCode> {
    let (pds, rest) = parse_pds_flags(args)?;
    let mut corpus_path: Option<String> = None;
    let mut budget = DEFAULT_PROPERTY_BUDGET;
    let mut iter = rest.iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--corpus" => {
                corpus_path = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("--corpus requires a value"))?
                        .clone(),
                );
            }
            "--budget" => {
                budget = iter
                    .next()
                    .ok_or_else(|| anyhow!("--budget requires a value"))?
                    .parse()
                    .context("parse --budget")?;
            }
            other => bail!("unknown flag: {other}"),
        }
    }
    let corpus = load_corpus(corpus_path.as_deref().ok_or_else(|| {
        anyhow!("--corpus <path> required for property-test (drives the generator)")
    })?)?;

    let (resolver, loader) = pds_resolver_loader(&pds.pds_url);
    // Generator returns corpus[idx % len] for the i-th case, so the
    // CLI's "corpus file" doubles as a generator pool.
    let corpus_clone = corpus.clone();
    let runner = PropertyTestRunner::new(
        resolver,
        loader,
        Protocol::default(),
        budget,
        move |idx: u32| corpus_clone[(idx as usize) % corpus_clone.len()].clone(),
    );
    let target = target_from(&pds)?;
    let verification = runner
        .run(&target)
        .await
        .context("PropertyTestRunner::run")?;
    print_and_exit(&verification)
}

// -----------------------------------------------------------------
// static-check
// -----------------------------------------------------------------

async fn cmd_static_check(args: &[String]) -> Result<ExitCode> {
    let (pds, rest) = parse_pds_flags(args)?;
    if let Some(flag) = rest.first() {
        bail!("unknown flag: {flag}");
    }
    let (resolver, loader) = pds_resolver_loader(&pds.pds_url);
    let runner = StaticCheckRunner::new(resolver, loader, Protocol::default());
    let target = target_from(&pds)?;
    let verification = runner
        .run(&target)
        .await
        .context("StaticCheckRunner::run")?;
    print_and_exit(&verification)
}

// -----------------------------------------------------------------
// coercion-law
// -----------------------------------------------------------------

/// Reqwest-backed `CoercionLawClient` calling the
/// `dev.panproto.translate.verifyCoercionLaws` xrpc method on a
/// panproto-vcs endpoint.
struct ReqwestCoercionLawClient {
    http: reqwest::Client,
    vcs_url: String,
}

#[derive(Serialize)]
struct VerifyCoercionRequest<'a> {
    lens: &'a str,
    standard: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<&'a str>,
}

impl CoercionLawClient for ReqwestCoercionLawClient {
    async fn verify_coercion_laws(
        &self,
        lens_uri: &str,
        standard: &str,
        version: Option<&str>,
    ) -> Result<Vec<CoercionLawViolation>, LensError> {
        let url = format!(
            "{}/xrpc/dev.panproto.translate.verifyCoercionLaws",
            self.vcs_url
        );
        let resp = self
            .http
            .post(&url)
            .json(&VerifyCoercionRequest {
                lens: lens_uri,
                standard,
                version,
            })
            .send()
            .await
            .map_err(|e| LensError::Transport(format!("verifyCoercionLaws: {e}")))?;
        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LensError::Transport(format!("verifyCoercionLaws body: {e}")))?;
        if !status.is_success() {
            return Err(LensError::Transport(format!(
                "verifyCoercionLaws returned {status}: {body}"
            )));
        }
        let raw = body
            .get("violations")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        let entries: Vec<serde_json::Value> = serde_json::from_value(raw)
            .map_err(|e| LensError::Transport(format!("decode violations: {e}")))?;
        Ok(entries
            .into_iter()
            .map(|v| CoercionLawViolation {
                law: v
                    .get("law")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_owned(),
                detail: v
                    .get("detail")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_owned(),
            })
            .collect())
    }
}

async fn cmd_coercion_law(args: &[String]) -> Result<ExitCode> {
    let (pds, rest) = parse_pds_flags(args)?;
    let mut vcs_url: Option<String> = None;
    let mut standard: Option<String> = None;
    let mut version: Option<String> = None;
    let mut violation_threshold: Option<u32> = None;
    let mut iter = rest.iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--vcs-url" => {
                vcs_url = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("--vcs-url requires a value"))?
                        .clone(),
                );
            }
            "--standard" => {
                standard = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("--standard requires a value"))?
                        .clone(),
                );
            }
            "--version" => {
                version = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("--version requires a value"))?
                        .clone(),
                );
            }
            "--violation-threshold" => {
                violation_threshold = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("--violation-threshold requires a value"))?
                        .parse()
                        .context("parse --violation-threshold")?,
                );
            }
            other => bail!("unknown flag: {other}"),
        }
    }
    let vcs_url = vcs_url.ok_or_else(|| anyhow!("--vcs-url required"))?;
    let standard = standard.ok_or_else(|| anyhow!("--standard required"))?;

    let client = ReqwestCoercionLawClient {
        http: reqwest::Client::new(),
        vcs_url,
    };
    let runner = CoercionLawRunner::new(client, standard, version, violation_threshold);
    let target = target_from(&pds)?;
    let verification = runner
        .run(&target)
        .await
        .context("CoercionLawRunner::run")?;
    // pds is unused here (coercion-law uses a vcs URL); silence the
    // borrow-checker by referencing pds_url anyway.
    let _ = pds.pds_url;
    print_and_exit(&verification)
}
