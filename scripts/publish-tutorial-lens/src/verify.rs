//! Runs `RoundtripTestRunner` against the published tutorial lens
//! end-to-end against the live PDS. Confirms that the rename-sort
//! lens round-trips the corpus byte-for-byte (it's an Iso).

use anyhow::Result;
use idiolect_lens::{PdsResolver, PdsSchemaLoader, ReqwestPdsClient};
use idiolect_records::Datetime;
use idiolect_records::generated::dev::idiolect::defs::LensRef;
use idiolect_verify::{
    RoundtripTestRunner, VerificationRunner, VerificationTarget,
};
use panproto_schema::Protocol;

const PDS: &str = "https://jellybaby.us-east.host.bsky.network";
const LENS_URI: &str = "at://did:plc:wdl4nnvxxdy4mc5vddxlm6f3/dev.panproto.schema.lens/tutorial-rename-sort-string-to-text";

#[tokio::main]
async fn main() -> Result<()> {
    let client = ReqwestPdsClient::with_service_url(PDS);
    let resolver = PdsResolver::new(client.clone());
    let loader = PdsSchemaLoader::new(client);

    // Small corpus matching the lens's source schema (single
    // `text` string child on a `post:body` object).
    let corpus = vec![
        serde_json::json!({ "text": "hello, world" }),
        serde_json::json!({ "text": "" }),
        serde_json::json!({ "text": "líneas con tildes y emoji 🦀" }),
    ];

    let runner = RoundtripTestRunner::new(resolver, loader, Protocol::default(), corpus);

    let target = VerificationTarget {
        lens: LensRef {
            uri: Some(LENS_URI.parse()?),
            cid: None,
            direction: None,
        },
        verifier: "did:plc:wdl4nnvxxdy4mc5vddxlm6f3".parse()?,
        occurred_at: Datetime::parse("2026-05-12T00:00:00.000Z")?,
        tool_override: None,
    };

    let verification = runner.run(&target).await?;
    println!("result = {:?}", verification.result);
    println!("kind   = {:?}", verification.kind);
    println!(
        "tool   = {} {}",
        verification.tool.name, verification.tool.version
    );
    Ok(())
}
