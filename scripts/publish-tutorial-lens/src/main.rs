//! One-shot publisher for the docs/book/src/tutorial/03-apply-lens.md
//! demonstration data. Publishes three records to the configured
//! account's PDS:
//!
//!   1. `dev.panproto.schema.schema/v1` — single-field "post:body"
//!      with a string `text` child.
//!   2. `dev.panproto.schema.schema/v2` — same shape with the kind
//!      relabelled to `text` (derived from the chain's
//!      `target_schema`).
//!   3. `dev.panproto.schema.lens/rename-sort-string-to-text` —
//!      single-step protolens chain that applies a `rename_sort`.
//!
//! Auth uses ATProto app passwords (Bearer mode); no DPoP, no OAuth.
//! Set the following env vars before running:
//!
//!   PDS_URL          e.g. https://jellybaby.us-east.host.bsky.network
//!   ATPROTO_HANDLE   e.g. idiolect.dev
//!   ATPROTO_PASSWORD an app password generated from
//!                    https://bsky.app/settings/app-passwords
//!
//! Then `cargo run -p publish-tutorial-lens`. The binary prints the
//! three resulting at-uris; paste them into the tutorial.

use std::env;

use anyhow::{Context, Result, anyhow};
use idiolect_records::dev::panproto::schema::lens::PanprotoLensRoundTripClass;
use idiolect_records::{Datetime, PanprotoLens, PanprotoSchema, Record};
use panproto_lens::protolens::elementary;
use panproto_schema::{Protocol, Schema, SchemaBuilder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[tokio::main]
async fn main() -> Result<()> {
    let pds = env::var("PDS_URL").context("PDS_URL env var")?;
    let handle = env::var("ATPROTO_HANDLE").context("ATPROTO_HANDLE env var")?;
    let password = env::var("ATPROTO_PASSWORD").context("ATPROTO_PASSWORD env var")?;

    let http = reqwest::Client::new();

    // -----------------------------------------------------------------
    // login
    // -----------------------------------------------------------------
    let session = create_session(&http, &pds, &handle, &password).await?;
    let did = session.did.clone();
    let bearer = session.access_jwt;
    eprintln!("logged in as {did}");

    // -----------------------------------------------------------------
    // build artifacts
    // -----------------------------------------------------------------
    let protocol = Protocol::default();

    // Source schema: a `post:body` object with one `string`-kinded
    // `text` child. Mirrors `single_field_source_schema()` in the
    // `idiolect-lens` roundtrip integration test.
    let src_schema = SchemaBuilder::new(&protocol)
        .entry("post:body")
        .vertex("post:body", "object", None)
        .map_err(|e| anyhow!("source schema vertex: {e}"))?
        .vertex("post:body.text", "string", None)
        .map_err(|e| anyhow!("source schema vertex: {e}"))?
        .edge("post:body", "post:body.text", "prop", Some("text"))
        .map_err(|e| anyhow!("source schema edge: {e}"))?
        .build()
        .map_err(|e| anyhow!("source schema build: {e}"))?;

    // Protolens chain: rename the `string` sort to `text`. The
    // target schema is derived from the chain so the loader and
    // the runtime always agree on what comes out.
    let protolens = elementary::rename_sort("string", "text");
    let tgt_schema = protolens
        .target_schema(&src_schema, &protocol)
        .map_err(|e| anyhow!("derive target schema: {e}"))?;

    // -----------------------------------------------------------------
    // publish schemas first; their at-uris are referenced by the lens
    // -----------------------------------------------------------------
    let src_record = make_schema_record(&src_schema)?;
    let src_rkey = "tutorial-post-body-v1";
    publish(
        &http,
        &pds,
        &bearer,
        &did,
        PanprotoSchema::NSID,
        src_rkey,
        &src_record,
    )
    .await?;
    let src_uri = format!("at://{did}/{}/{src_rkey}", PanprotoSchema::NSID);
    eprintln!("published {src_uri}");

    let tgt_record = make_schema_record(&tgt_schema)?;
    let tgt_rkey = "tutorial-post-body-v2";
    publish(
        &http,
        &pds,
        &bearer,
        &did,
        PanprotoSchema::NSID,
        tgt_rkey,
        &tgt_record,
    )
    .await?;
    let tgt_uri = format!("at://{did}/{}/{tgt_rkey}", PanprotoSchema::NSID);
    eprintln!("published {tgt_uri}");

    // -----------------------------------------------------------------
    // publish the lens
    // -----------------------------------------------------------------
    let lens_blob = serde_json::to_value(&protolens)?;
    let lens_record = PanprotoLens {
        blob: Some(lens_blob.clone()),
        created_at: now()?,
        laws_verified: Some(true),
        object_hash: sha256_of(&lens_blob)?,
        round_trip_class: Some(PanprotoLensRoundTripClass::Iso),
        source_schema: src_uri.parse().context("parse source-schema at-uri")?,
        target_schema: tgt_uri.parse().context("parse target-schema at-uri")?,
    };
    let lens_rkey = "tutorial-rename-sort-string-to-text";
    publish(
        &http,
        &pds,
        &bearer,
        &did,
        PanprotoLens::NSID,
        lens_rkey,
        &lens_record,
    )
    .await?;
    let lens_uri = format!("at://{did}/{}/{lens_rkey}", PanprotoLens::NSID);
    eprintln!("published {lens_uri}");

    println!("\nuse in tutorial 03:");
    println!("  lens          {lens_uri}");
    println!("  source schema {src_uri}");
    println!("  target schema {tgt_uri}");
    Ok(())
}

// -----------------------------------------------------------------
// session
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
}

async fn create_session(
    http: &reqwest::Client,
    pds: &str,
    identifier: &str,
    password: &str,
) -> Result<CreateSessionResponse> {
    let url = format!("{pds}/xrpc/com.atproto.server.createSession");
    let resp = http
        .post(&url)
        .json(&CreateSessionRequest { identifier, password })
        .send()
        .await
        .context("createSession request")?;
    if !resp.status().is_success() {
        return Err(anyhow!(
            "createSession returned {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        ));
    }
    Ok(resp.json::<CreateSessionResponse>().await?)
}

// -----------------------------------------------------------------
// publish
// -----------------------------------------------------------------

#[derive(Serialize)]
struct CreateRecordRequest<'a> {
    repo: &'a str,
    collection: &'a str,
    rkey: &'a str,
    record: serde_json::Value,
}

async fn publish<R: Serialize>(
    http: &reqwest::Client,
    pds: &str,
    bearer: &str,
    repo: &str,
    collection: &str,
    rkey: &str,
    record: &R,
) -> Result<()> {
    // Wire form needs a $type discriminator on the body, even though
    // the repo/collection arguments already pin the NSID; this is the
    // convention every other ATProto record follows.
    let mut value = serde_json::to_value(record)?;
    if let serde_json::Value::Object(ref mut map) = value {
        map.insert(
            "$type".to_owned(),
            serde_json::Value::String(collection.to_owned()),
        );
    }

    let url = format!("{pds}/xrpc/com.atproto.repo.createRecord");
    let resp = http
        .post(&url)
        .bearer_auth(bearer)
        .json(&CreateRecordRequest {
            repo,
            collection,
            rkey,
            record: value,
        })
        .send()
        .await
        .context("createRecord request")?;
    if !resp.status().is_success() {
        return Err(anyhow!(
            "createRecord({collection}/{rkey}) returned {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        ));
    }
    Ok(())
}

// -----------------------------------------------------------------
// helpers
// -----------------------------------------------------------------

fn make_schema_record(schema: &Schema) -> Result<PanprotoSchema> {
    let blob = serde_json::to_value(schema)?;
    Ok(PanprotoSchema {
        blob: Some(blob.clone()),
        constraint_count: None,
        created_at: now()?,
        edge_count: Some(i64::try_from(schema.edges.len()).unwrap_or(0)),
        object_hash: sha256_of(&blob)?,
        protocol: "atproto".to_owned(),
        vertex_count: Some(i64::try_from(schema.vertices.len()).unwrap_or(0)),
    })
}

fn now() -> Result<Datetime> {
    // RFC 3339 millisecond precision matches the rest of the
    // ecosystem's `Datetime::parse` inputs.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock before unix epoch")?;
    let secs = now.as_secs() as i64;
    let millis = now.subsec_millis();
    // Format manually so we don't pull a date crate.
    let datetime = format_rfc3339(secs, millis);
    Datetime::parse(datetime).map_err(|e| anyhow!("parse datetime: {e}"))
}

fn format_rfc3339(secs: i64, millis: u32) -> String {
    // Inline a small Gregorian split. Fine for "now"; not robust to
    // historic dates. Matches `2026-05-12T17:30:00.000Z` shape.
    let days = secs.div_euclid(86_400);
    let time_of_day = secs.rem_euclid(86_400);
    let (year, month, day) = days_to_ymd(days);
    let hour = (time_of_day / 3600) as u32;
    let minute = ((time_of_day % 3600) / 60) as u32;
    let second = (time_of_day % 60) as u32;
    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z"
    )
}

fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    // Days since 1970-01-01 to (year, month, day). Iterative; clarity
    // over speed.
    let mut year: i64 = 1970;
    let mut remaining = days;
    loop {
        let len = if is_leap(year) { 366 } else { 365 };
        if remaining < len {
            break;
        }
        remaining -= len;
        year += 1;
    }
    let months_normal = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let months_leap = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let lengths = if is_leap(year) {
        &months_leap
    } else {
        &months_normal
    };
    let mut month: u32 = 0;
    for (idx, &len) in lengths.iter().enumerate() {
        if remaining < len {
            month = (idx + 1) as u32;
            break;
        }
        remaining -= len;
    }
    let day = (remaining + 1) as u32;
    (year, month, day)
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn sha256_of(value: &serde_json::Value) -> Result<String> {
    let canonical = serde_json::to_vec(value)?;
    let mut h = Sha256::new();
    h.update(&canonical);
    let digest = h.finalize();
    // Use a base32-like encoding so we get a stable, valid string
    // shape for the object_hash field. Hex is fine; both are widely
    // tolerated as long as we use the same form everywhere.
    let _ = base64::engine::general_purpose::STANDARD_NO_PAD;
    Ok(format!("sha256:{}", hex::encode(digest)))
}
