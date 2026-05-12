//! `idiolect oauth` — manage authenticated PDS sessions.
//!
//! Three subcommands:
//!
//! ```text
//! idiolect oauth login  --handle H --app-password P --pds-url URL
//! idiolect oauth list
//! idiolect oauth logout --did D
//! ```
//!
//! Sessions persist as one JSON file per DID under
//! `~/.config/idiolect/sessions/` (override via `IDIOLECT_SESSION_DIR`).
//! Each file carries the minimum fields the publisher path needs:
//! `did`, `pds_url`, `access_jwt`, `refresh_jwt`.
//!
//! This is the app-password Bearer path. The full OAuth + `DPoP` flow
//! (via `atrium-oauth-client`) is the planned upgrade; the file
//! format here is forward-compatible with adding `dpop_private_key`
//! later.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

const DEFAULT_PDS_URL: &str = "https://bsky.social";

/// On-disk session shape. One JSON file per DID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliSession {
    pub did: String,
    pub handle: Option<String>,
    pub pds_url: String,
    #[serde(rename = "accessJwt")]
    pub access_jwt: String,
    #[serde(rename = "refreshJwt")]
    pub refresh_jwt: String,
}

impl CliSession {
    /// Standard sessions directory.
    pub fn dir() -> Result<PathBuf> {
        if let Ok(p) = std::env::var("IDIOLECT_SESSION_DIR") {
            return Ok(PathBuf::from(p));
        }
        let home = std::env::var("HOME").context("HOME env var")?;
        Ok(PathBuf::from(home).join(".config/idiolect/sessions"))
    }

    /// Where on disk a session for `did` lives.
    pub fn path_for(did: &str) -> Result<PathBuf> {
        let mut filename = String::with_capacity(did.len() + 5);
        for ch in did.chars() {
            match ch {
                ':' | '/' | '\\' | '\0' => filename.push('_'),
                c => filename.push(c),
            }
        }
        filename.push_str(".json");
        Ok(Self::dir()?.join(filename))
    }

    /// Load the session for `did` from disk.
    pub fn load(did: &str) -> Result<Self> {
        let path = Self::path_for(did)?;
        let bytes =
            std::fs::read(&path).with_context(|| format!("read session at {}", path.display()))?;
        let session: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse session at {}", path.display()))?;
        Ok(session)
    }

    /// Save the session to disk. Creates the sessions dir if missing.
    pub fn save(&self) -> Result<PathBuf> {
        let path = Self::path_for(&self.did)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        let bytes = serde_json::to_vec_pretty(self)?;
        std::fs::write(&path, bytes)
            .with_context(|| format!("write session at {}", path.display()))?;
        Ok(path)
    }
}

pub async fn dispatch(args: &[String]) -> Result<ExitCode> {
    let Some(sub) = args.first() else {
        bail!("usage: idiolect oauth <login|list|logout> ...");
    };
    let rest = &args[1..];
    match sub.as_str() {
        "login" => cmd_login(rest).await,
        "list" => cmd_list(rest),
        "logout" => cmd_logout(rest),
        other => bail!("unknown oauth subcommand: {other}"),
    }
}

// -----------------------------------------------------------------
// login
// -----------------------------------------------------------------

#[derive(Serialize)]
struct CreateSessionRequest<'a> {
    identifier: &'a str,
    password: &'a str,
}

#[derive(Deserialize)]
struct CreateSessionResponse {
    did: String,
    #[serde(rename = "accessJwt")]
    access_jwt: String,
    #[serde(rename = "refreshJwt")]
    refresh_jwt: String,
    #[serde(default)]
    handle: Option<String>,
}

async fn cmd_login(args: &[String]) -> Result<ExitCode> {
    let mut handle: Option<String> = None;
    let mut password: Option<String> = None;
    let mut pds_url = DEFAULT_PDS_URL.to_owned();

    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--handle" => {
                handle = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("--handle requires a value"))?
                        .clone(),
                );
            }
            "--app-password" => {
                password = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("--app-password requires a value"))?
                        .clone(),
                );
            }
            "--pds-url" => {
                pds_url = iter
                    .next()
                    .ok_or_else(|| anyhow!("--pds-url requires a value"))?
                    .clone();
            }
            other => bail!("unknown flag: {other}"),
        }
    }

    let handle = handle.ok_or_else(|| anyhow!("--handle required"))?;
    // Allow the password to come from env (so it doesn't end up in
    // shell history) when --app-password is omitted.
    let password = match password {
        Some(p) => p,
        None => std::env::var("ATPROTO_APP_PASSWORD")
            .or_else(|_| std::env::var("ATPROTO_PASSWORD"))
            .context(
                "--app-password not supplied; pass --app-password or set \
                 ATPROTO_APP_PASSWORD / ATPROTO_PASSWORD",
            )?,
    };

    let http = reqwest::Client::new();
    let url = format!("{pds_url}/xrpc/com.atproto.server.createSession");
    let resp = http
        .post(&url)
        .json(&CreateSessionRequest {
            identifier: &handle,
            password: &password,
        })
        .send()
        .await
        .context("createSession request")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("createSession returned {status}: {body}");
    }
    let resp: CreateSessionResponse = resp.json().await.context("decode createSession response")?;

    let session = CliSession {
        did: resp.did.clone(),
        handle: resp.handle.or(Some(handle)),
        pds_url,
        access_jwt: resp.access_jwt,
        refresh_jwt: resp.refresh_jwt,
    };
    let path = session.save()?;

    let out = serde_json::json!({
        "did":     session.did,
        "handle":  session.handle,
        "pds_url": session.pds_url,
        "stored":  path.display().to_string(),
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(ExitCode::from(0))
}

// -----------------------------------------------------------------
// list
// -----------------------------------------------------------------

fn cmd_list(_args: &[String]) -> Result<ExitCode> {
    let dir = CliSession::dir()?;
    if !dir.is_dir() {
        println!("[]");
        return Ok(ExitCode::from(0));
    }
    let mut sessions = Vec::new();
    for entry in std::fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        match CliSession::load_from_path(&path) {
            Ok(s) => sessions.push(serde_json::json!({
                "did":     s.did,
                "handle":  s.handle,
                "pds_url": s.pds_url,
            })),
            Err(e) => {
                eprintln!("warning: skipping {}: {e}", path.display());
            }
        }
    }
    println!("{}", serde_json::to_string_pretty(&sessions)?);
    Ok(ExitCode::from(0))
}

impl CliSession {
    fn load_from_path(path: &std::path::Path) -> Result<Self> {
        let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}

// -----------------------------------------------------------------
// logout
// -----------------------------------------------------------------

fn cmd_logout(args: &[String]) -> Result<ExitCode> {
    let mut did: Option<String> = None;
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--did" => {
                did = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("--did requires a value"))?
                        .clone(),
                );
            }
            other => bail!("unknown flag: {other}"),
        }
    }
    let did = did.ok_or_else(|| anyhow!("--did required"))?;
    let path = CliSession::path_for(&did)?;
    match std::fs::remove_file(&path) {
        Ok(()) => println!("deleted {}", path.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("no session at {}", path.display());
        }
        Err(e) => return Err(e).context("delete session file"),
    }
    Ok(ExitCode::from(0))
}
