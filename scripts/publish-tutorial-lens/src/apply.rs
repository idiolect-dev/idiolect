//! Runs `apply_lens` against the records published by
//! `publish-tutorial-lens`, end to end against the live PDS.
//!
//! Validates that the tutorial 3 sample code actually works:
//! resolve the lens by at-uri, load both schemas from the PDS,
//! project the source record through the chain, print the view.

use std::pin::Pin;

use anyhow::Result;
use idiolect_lens::{
    ApplyLensInput, AtUri, LensError, PdsResolver, ReqwestPdsClient, SchemaLoader, apply_lens,
};
use panproto_schema::{Protocol, Schema};

const PDS: &str = "https://jellybaby.us-east.host.bsky.network";
const LENS: &str = "at://did:plc:wdl4nnvxxdy4mc5vddxlm6f3/dev.panproto.schema.lens/tutorial-rename-sort-string-to-text";

/// SchemaLoader that fetches a `dev.panproto.schema.schema` record
/// from a PDS and pulls the typed `Schema` out of its `blob` field.
/// The shipped `FilesystemSchemaLoader` reads ATProto lexicons; the
/// `dev.panproto.schema.schema` lexicon carries a serialised
/// `panproto_schema::Schema` instead, so the loader needs to know
/// where to look.
struct PdsSchemaLoader {
    http: reqwest::Client,
}

impl PdsSchemaLoader {
    fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
        }
    }
}

impl SchemaLoader for PdsSchemaLoader {
    fn load<'a>(
        &'a self,
        at_uri: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Schema, LensError>> + Send + 'a>> {
        Box::pin(async move {
            let rest = at_uri
                .strip_prefix("at://")
                .ok_or_else(|| LensError::Transport(format!("not an at-uri: {at_uri}")))?;
            let mut parts = rest.splitn(3, '/');
            let (did, coll, rkey) = match (parts.next(), parts.next(), parts.next()) {
                (Some(d), Some(c), Some(r)) => (d, c, r),
                _ => return Err(LensError::Transport(format!("malformed at-uri: {at_uri}"))),
            };
            let url = format!(
                "{PDS}/xrpc/com.atproto.repo.getRecord?repo={did}&collection={coll}&rkey={rkey}"
            );
            let resp = self
                .http
                .get(&url)
                .send()
                .await
                .map_err(|e| LensError::Transport(format!("{e}")))?;
            let body: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| LensError::Transport(format!("{e}")))?;
            let blob = body
                .get("value")
                .and_then(|v| v.get("blob"))
                .cloned()
                .ok_or_else(|| LensError::LexiconParse("no blob".into()))?;
            serde_json::from_value(blob).map_err(|e| LensError::LexiconParse(e.to_string()))
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = ReqwestPdsClient::with_service_url(PDS);
    let resolver = PdsResolver::new(client);
    let loader = PdsSchemaLoader::new();
    let protocol = Protocol::default();

    let lens_uri = AtUri::parse(LENS)?;
    let source_record: serde_json::Value = serde_json::from_str(r#"{ "text": "hello, world" }"#)?;

    let out = apply_lens(
        &resolver,
        &loader,
        &protocol,
        ApplyLensInput {
            lens_uri,
            source_record,
            source_root_vertex: None,
        },
    )
    .await?;

    println!(
        "target_record = {}",
        serde_json::to_string_pretty(&out.target_record)?
    );
    Ok(())
}
