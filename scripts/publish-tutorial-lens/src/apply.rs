//! Runs `apply_lens` against the records published by
//! `publish-tutorial-lens`, end to end against the live PDS,
//! using the shipped `PdsSchemaLoader`.

use anyhow::Result;
use idiolect_lens::{
    ApplyLensInput, AtUri, PdsResolver, PdsSchemaLoader, ReqwestPdsClient, apply_lens,
};
use panproto_schema::Protocol;

const PDS: &str = "https://jellybaby.us-east.host.bsky.network";
const LENS: &str = "at://did:plc:wdl4nnvxxdy4mc5vddxlm6f3/dev.panproto.schema.lens/tutorial-rename-sort-string-to-text";

#[tokio::main]
async fn main() -> Result<()> {
    let client = ReqwestPdsClient::with_service_url(PDS);
    let resolver = PdsResolver::new(client.clone());
    let loader = PdsSchemaLoader::new(client);
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
