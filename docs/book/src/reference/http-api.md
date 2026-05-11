# HTTP query API

Read-only endpoints exposed by `idiolect-orchestrator` under the
`query-http` feature. All requests are `GET`. All responses are
JSON.

The route surface is generated from
`orchestrator-spec/queries.json`. Each query maps onto **two**
endpoints: a friendly REST path under `/v1/…` and an
ATProto-style xrpc path under
`/xrpc/dev.idiolect.query.<queryName>`. Both call the same
handler. The snapshot below reflects the spec at v0.8.0.

## Liveness and metrics

| Path | Returns |
| --- | --- |
| `GET /healthz` | 200 OK if the process is alive. |
| `GET /readyz` | 200 OK once the catalog has caught up. |
| `GET /metrics` | Prometheus exposition. |
| `GET /v1/stats` | Per-kind record counts. |

## Generated query endpoints

| REST path | xrpc path | Returns |
| --- | --- | --- |
| `GET /v1/bounties/open` | `/xrpc/dev.idiolect.query.openBounties` | Bounties whose status is `open`, `claimed`, or unset. |
| `GET /v1/bounties/want-lens?...` | `/xrpc/dev.idiolect.query.bountiesForWantLens` | Bounties whose `wants` is a specific lens. |
| `GET /v1/bounties/by-requester?requester_did=...` | `/xrpc/dev.idiolect.query.bountiesByRequester` | Bounties by requester DID. |
| `GET /v1/adapters?framework=...` | `/xrpc/dev.idiolect.query.adaptersForFramework` | Adapters declared for a framework. |
| `GET /v1/adapters/by-invocation-protocol?...` | `/xrpc/dev.idiolect.query.adaptersByInvocationProtocol` | Adapters by invocation-protocol kind. |
| `GET /v1/adapters/with-verification?...` | `/xrpc/dev.idiolect.query.adaptersWithVerification` | Adapters that carry at least one verification record. |
| `GET /v1/recommendations` | `/xrpc/dev.idiolect.query.recommendationsStartingFrom` | Recommendations starting from a given source schema. |
| `GET /v1/verifications?lens_uri=...` | `/xrpc/dev.idiolect.query.verificationsForLens` | Verifications for a specific lens. |
| `GET /v1/verifications/by-kind?...` | `/xrpc/dev.idiolect.query.verificationsByKind` | Verifications by kind. |
| `GET /v1/communities?...` | `/xrpc/dev.idiolect.query.communitiesForMember` | Communities for a member DID. |
| `GET /v1/communities/by-name?...` | `/xrpc/dev.idiolect.query.communitiesByName` | Communities by name. |
| `GET /v1/dialects/for-community?...` | `/xrpc/dev.idiolect.query.dialectsForCommunity` | Dialects owned by a community. |
| `GET /v1/beliefs/about?...` | `/xrpc/dev.idiolect.query.beliefsAboutRecord` | Beliefs whose subject is a given record. |
| `GET /v1/beliefs/by-holder?...` | `/xrpc/dev.idiolect.query.beliefsByHolder` | Beliefs by holder DID. |
| `GET /v1/vocabularies/by-world?...` | `/xrpc/dev.idiolect.query.vocabulariesWithWorld` | Vocabularies declared with a given `world`. |
| `GET /v1/vocabularies/by-name?...` | `/xrpc/dev.idiolect.query.vocabulariesByName` | Vocabularies by name. |

The authoritative parameter list per endpoint is in
[`orchestrator-spec/queries.json`](https://github.com/idiolect-dev/idiolect/blob/main/orchestrator-spec/queries.json).
The codegen-emitted handlers live in
`crates/idiolect-orchestrator/src/generated/http.rs`.

## Response shape

List endpoints return a JSON envelope whose exact shape is
generated per query. The general pattern: a top-level object
carrying the result list plus pagination metadata. See the
emitted Rust types in
`crates/idiolect-orchestrator/src/generated/` for the precise
shape per endpoint.

## Error shape

A request that fails parameter validation returns 400 with a
JSON body naming the offending field. Internal failures return
500 with a brief message.

## Versioning

The `v1` and `/xrpc/` prefixes are the route contract. New
endpoints are additive. Pre-1.0 the project may rename or
restructure endpoints between minor versions; see
[Stability and versioning](./stability.md). At 1.0 the prefixes
become stable and breaking changes ship under `v2` (or, for the
xrpc surface, under new method names that deprecate the old).
