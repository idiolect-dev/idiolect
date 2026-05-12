//! `idiolect publish` — author a record from a local JSON file.
//!
//! ```text
//! idiolect publish <kind> --record <path> [--rkey RKEY] [--did DID]
//! ```
//!
//! `<kind>` is the unqualified record kind (`encounter`,
//! `recommendation`, `verification`, ...). The CLI maps it onto the
//! corresponding `dev.idiolect.*` NSID, validates the JSON body
//! against the typed `Record` impl, and POSTs
//! `com.atproto.repo.createRecord` to the configured PDS using the
//! Bearer access token saved by `idiolect oauth login`.
//!
//! When `--did` is omitted the CLI picks the first stored session.

use std::process::ExitCode;

use anyhow::{Context, Result, anyhow, bail};
use idiolect_records::{
    Adapter, Belief, Bounty, Community, Correction, Deliberation, DeliberationOutcome,
    DeliberationStatement, DeliberationVote, Dialect, Encounter, Nsid, Observation, Record,
    Recommendation, Retrospection, Verification, Vocab, decode_record,
};
use serde::Serialize;

use crate::oauth::CliSession;

// `dispatch` does its own arg-parsing inline. Splitting it out adds
// more shapes than it removes; allow the line-count lint here.
#[allow(clippy::too_many_lines)]
pub async fn dispatch(args: &[String]) -> Result<ExitCode> {
    // First positional arg is the kind; remainder are flags.
    let Some(kind) = args.first() else {
        bail!(
            "usage: idiolect publish <kind> --record <path> [--rkey RKEY] [--did DID]\n\
             kinds: {}",
            shipped_kinds().join(", ")
        );
    };
    let rest = &args[1..];

    let mut record_path: Option<String> = None;
    let mut rkey: Option<String> = None;
    let mut did_override: Option<String> = None;

    let mut iter = rest.iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--record" => {
                record_path = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("--record requires a value"))?
                        .clone(),
                );
            }
            "--rkey" => {
                rkey = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("--rkey requires a value"))?
                        .clone(),
                );
            }
            "--did" => {
                did_override = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("--did requires a value"))?
                        .clone(),
                );
            }
            other => bail!("unknown flag: {other}"),
        }
    }
    let record_path = record_path.ok_or_else(|| anyhow!("--record <path> required"))?;

    let nsid = nsid_for_kind(kind)?;

    // Parse + validate the record body through `decode_record`. This
    // surfaces a structured error pointing at the first invalid
    // field, before we round-trip to wire form.
    let bytes = std::fs::read(&record_path)
        .with_context(|| format!("read record file {record_path}"))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).context("parse record JSON")?;
    let _any = decode_record(&nsid, value.clone()).map_err(|e| {
        anyhow!(
            "record body did not validate against {}: {e}",
            nsid.as_str()
        )
    })?;

    // Load the publisher session.
    let session = match did_override {
        Some(did) => CliSession::load(&did).with_context(|| format!("load session for {did}"))?,
        None => first_session()?,
    };

    // Re-encode the value with a `$type` field spliced in. ATProto
    // record bodies carry $type; the typed Rust structs don't emit
    // one (the NSID is implicit in their type), so we add it here.
    let mut payload = value;
    if let serde_json::Value::Object(ref mut map) = payload {
        map.insert(
            "$type".to_owned(),
            serde_json::Value::String(nsid.as_str().to_owned()),
        );
    } else {
        bail!("record body must be a JSON object");
    }

    let rkey = rkey.unwrap_or_else(tid_now);

    let http = reqwest::Client::new();
    let url = format!("{}/xrpc/com.atproto.repo.createRecord", session.pds_url);
    #[derive(Serialize)]
    struct CreateRecordRequest<'a> {
        repo: &'a str,
        collection: &'a str,
        rkey: &'a str,
        record: serde_json::Value,
    }
    let response = http
        .post(&url)
        .bearer_auth(&session.access_jwt)
        .json(&CreateRecordRequest {
            repo: &session.did,
            collection: nsid.as_str(),
            rkey: &rkey,
            record: payload,
        })
        .send()
        .await
        .context("createRecord request")?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("createRecord returned {status}: {body}");
    }

    let envelope: serde_json::Value = response
        .json()
        .await
        .context("decode createRecord response")?;
    let uri = envelope
        .get("uri")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_owned();
    let cid = envelope
        .get("cid")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_owned();
    let out = serde_json::json!({ "uri": uri, "cid": cid });
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(ExitCode::from(0))
}

fn shipped_kinds() -> Vec<&'static str> {
    vec![
        Adapter::NSID,
        Belief::NSID,
        Bounty::NSID,
        Community::NSID,
        Correction::NSID,
        Deliberation::NSID,
        DeliberationOutcome::NSID,
        DeliberationStatement::NSID,
        DeliberationVote::NSID,
        Dialect::NSID,
        Encounter::NSID,
        Observation::NSID,
        Recommendation::NSID,
        Retrospection::NSID,
        Verification::NSID,
        Vocab::NSID,
    ]
    .into_iter()
    .map(short_name)
    .collect()
}

/// Accept either an unqualified kind (`encounter`) or a fully-
/// qualified NSID (`dev.idiolect.encounter`). Return the canonical
/// `Nsid`.
fn nsid_for_kind(input: &str) -> Result<Nsid> {
    let qualified = if input.contains('.') {
        input.to_owned()
    } else {
        format!("dev.idiolect.{input}")
    };
    Nsid::parse(&qualified)
        .with_context(|| format!("parse NSID `{qualified}`"))
        .and_then(|n| {
            if shipped_kinds()
                .iter()
                .any(|s| *s == short_name(n.as_str()))
            {
                Ok(n)
            } else {
                Err(anyhow!(
                    "unknown kind `{input}` (try one of: {})",
                    shipped_kinds().join(", ")
                ))
            }
        })
}

fn short_name(nsid: &str) -> &str {
    nsid.rsplit('.').next().unwrap_or(nsid)
}

fn first_session() -> Result<CliSession> {
    let dir = CliSession::dir()?;
    if !dir.is_dir() {
        bail!(
            "no session store at {} — run `idiolect oauth login` first",
            dir.display()
        );
    }
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let bytes = std::fs::read(&path)?;
        if let Ok(session) = serde_json::from_slice::<CliSession>(&bytes) {
            return Ok(session);
        }
    }
    bail!(
        "no usable session in {} — run `idiolect oauth login` first",
        dir.display()
    )
}

/// TID-shaped rkey: milliseconds since the bsky epoch, base32 lower
/// alphabet, 13 chars. This matches the PDS server's default rkey
/// generator closely enough for hand-rolled publishes.
fn tid_now() -> String {
    const ALPHABET: &[u8] = b"234567abcdefghijklmnopqrstuvwxyz";
    let micros = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_micros());
    // Reserve top 2 bits for clock id; we use zero (single-host).
    let mut n: u64 = (micros & ((1 << 53) - 1)) as u64;
    let mut buf = [0u8; 13];
    for byte in buf.iter_mut().rev() {
        *byte = ALPHABET[(n & 0x1f) as usize];
        n >>= 5;
    }
    String::from_utf8(buf.to_vec()).unwrap_or_else(|_| "unknown".to_owned())
}
