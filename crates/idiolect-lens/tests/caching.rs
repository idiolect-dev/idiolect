//! Integration tests: [`CachingResolver`] layered over a live-shaped
//! [`PdsResolver`] with a mocked HTTP backend. Exercises the cache
//! against the same transport the production happy path uses.

#![cfg(feature = "pds-reqwest")]

use std::time::Duration;

use idiolect_lens::{CachingResolver, PdsResolver, ReqwestPdsClient, Resolver};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn lens_envelope() -> serde_json::Value {
    json!({
        "uri": "at://did:plc:x/dev.panproto.schema.lens/l1",
        "value": {
            "createdAt": "2026-04-19T00:00:00.000Z",
            "objectHash": "sha256:deadbeef",
            "sourceSchema": "at://did:plc:x/dev.panproto.schema.schema/a",
            "targetSchema": "at://did:plc:x/dev.panproto.schema.schema/b"
        }
    })
}

#[tokio::test]
async fn caching_resolver_collapses_repeat_resolves() {
    let server = MockServer::start().await;
    // `expect(1)` asserts the http backend was only hit once; the
    // cache is responsible for the other 99 hits.
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.getRecord"))
        .and(query_param("repo", "did:plc:x"))
        .and(query_param("collection", "dev.panproto.schema.lens"))
        .and(query_param("rkey", "l1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(lens_envelope()))
        .expect(1)
        .mount(&server)
        .await;

    let client = ReqwestPdsClient::with_service_url(server.uri());
    let resolver = PdsResolver::new(client);
    let caching = CachingResolver::new(resolver, Duration::from_secs(60));

    let uri = idiolect_lens::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/l1").unwrap();
    for _ in 0..100 {
        caching.resolve(&uri).await.unwrap();
    }
    assert_eq!(caching.cache_size(), 1);
    // `expect(1)` on the mock auto-verifies on drop of `server`.
    drop(server);
}
