//! Publishes a tutorial community and its `dev.idiolect.recommendation`.
//! The PDS assigns fresh record keys, so the example can be rerun.
//!
//! Auth: app-password Bearer mode. Reads three env vars (same set
//! as the lens publisher):
//!
//!   PDS_URL          e.g. https://jellybaby.us-east.host.bsky.network
//!   ATPROTO_HANDLE   e.g. idiolect.dev
//!   ATPROTO_PASSWORD an app password
//!
//! Then `cargo run --bin publish-tutorial-recommendation`.

use std::env;

use anyhow::{Context, Result, anyhow};
use idiolect_records::generated::dev::idiolect::defs::LensRef;
use idiolect_records::generated::dev::idiolect::recommendation::{
    ConditionSourceIs, Recommendation, RecommendationConditions,
};
use idiolect_records::{Community, Record};
use serde::{Deserialize, Serialize};

const LENS_URI: &str = "at://did:plc:wdl4nnvxxdy4mc5vddxlm6f3/dev.panproto.schema.lens/tutorial-rename-sort-string-to-text";
const SRC_SCHEMA_URI: &str =
    "at://did:plc:wdl4nnvxxdy4mc5vddxlm6f3/dev.panproto.schema.schema/tutorial-post-body-v1";

#[tokio::main]
async fn main() -> Result<()> {
    let pds = env::var("PDS_URL").context("PDS_URL env var")?;
    let handle = env::var("ATPROTO_HANDLE").context("ATPROTO_HANDLE env var")?;
    let password = env::var("ATPROTO_PASSWORD").context("ATPROTO_PASSWORD env var")?;

    let http = reqwest::Client::new();
    let session = create_session(&http, &pds, &handle, &password).await?;
    eprintln!("logged in as {}", session.did);

    let community = Community {
        appview_endpoint: None,
        conventions: None,
        conventions_text: None,
        core_lenses: None,
        core_schemas: None,
        created_at: now()?,
        description: "Community created by the idiolect publishing tutorial.".to_owned(),
        endorsed_communities: None,
        member_role_vocab: None,
        members: Some(vec![session.did.parse()?]),
        membership_roll: None,
        name: "idiolect-tutorial".to_owned(),
        record_hosting: None,
        role_assignments: None,
    };
    let community_uri = publish(&http, &pds, &session.access_jwt, &session.did, &community).await?;

    // A single `conditionSourceIs` limits this recommendation to
    // records that use the tutorial's v1 source schema.
    let rec = Recommendation {
        issuing_community: community_uri.parse()?,
        conditions: vec![RecommendationConditions::ConditionSourceIs(
            ConditionSourceIs {
                schema: idiolect_records::generated::dev::idiolect::defs::SchemaRef {
                    cid: None,
                    language: None,
                    uri: Some(SRC_SCHEMA_URI.parse()?),
                },
            },
        )],
        preconditions: None,
        lens_path: vec![LensRef {
            uri: Some(LENS_URI.parse()?),
            cid: None,
            direction: None,
        }],
        caveats: None,
        caveats_text: None,
        annotations: Some(
            "Tutorial recommendation: endorses the rename-sort \
             demonstration lens for source records that match v1 \
             of the tutorial post-body schema."
                .to_owned(),
        ),
        required_verifications: None,
        basis: None,
        occurred_at: now()?,
        supersedes: None,
    };

    let recommendation_uri = publish(&http, &pds, &session.access_jwt, &session.did, &rec).await?;
    println!("community      {community_uri}");
    println!("recommendation {recommendation_uri}");
    Ok(())
}

// -----------------------------------------------------------------
// session + publish
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
        .json(&CreateSessionRequest {
            identifier,
            password,
        })
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

#[derive(Serialize)]
struct CreateRecordRequest<'a> {
    repo: &'a str,
    collection: &'a str,
    record: serde_json::Value,
}

#[derive(Deserialize)]
struct CreateRecordResponse {
    uri: String,
}

async fn publish<R: Record>(
    http: &reqwest::Client,
    pds: &str,
    bearer: &str,
    repo: &str,
    rec: &R,
) -> Result<String> {
    let mut value = serde_json::to_value(rec)?;
    if let serde_json::Value::Object(ref mut map) = value {
        map.insert(
            "$type".to_owned(),
            serde_json::Value::String(R::NSID.to_owned()),
        );
    }
    let url = format!("{pds}/xrpc/com.atproto.repo.createRecord");
    let resp = http
        .post(&url)
        .bearer_auth(bearer)
        .json(&CreateRecordRequest {
            repo,
            collection: R::NSID,
            record: value,
        })
        .send()
        .await
        .context("createRecord request")?;
    if !resp.status().is_success() {
        return Err(anyhow!(
            "createRecord returned {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        ));
    }
    Ok(resp.json::<CreateRecordResponse>().await?.uri)
}

fn now() -> Result<idiolect_records::Datetime> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("clock before unix epoch")?;
    let secs = now.as_secs() as i64;
    let millis = now.subsec_millis();
    let days = secs.div_euclid(86_400);
    let time_of_day = secs.rem_euclid(86_400);
    let (year, month, day) = days_to_ymd(days);
    let hour = (time_of_day / 3600) as u32;
    let minute = ((time_of_day % 3600) / 60) as u32;
    let second = (time_of_day % 60) as u32;
    let s = format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z");
    idiolect_records::Datetime::parse(s).map_err(|e| anyhow!("parse datetime: {e}"))
}

fn days_to_ymd(days: i64) -> (i64, u32, u32) {
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
    let lengths = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
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
