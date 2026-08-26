# HTTP query API

`idiolect-orchestrator` exposes this read-only endpoint set under the
`query-http` feature. All requests are `GET`. Query endpoints return JSON;
health checks and metrics return text.

The route surface is generated from
`orchestrator-spec/queries.json`. Each query maps onto **two**
endpoints: a friendly REST path under `/v1/…` and an
ATProto-style xrpc path under
`/xrpc/dev.idiolect.query.<queryName>`. Both call the same
handler. The snapshot below reflects idiolect 0.12.1.

## Liveness and metrics

| Path | Returns |
| --- | --- |
| `GET /healthz` | 200 OK if the process is alive. |
| `GET /readyz` | 200 OK once the catalog has caught up. |
| `GET /metrics` | Prometheus exposition. |
| `GET /v1/stats` | Per-kind record counts. |
| `GET /v1/verifications/sufficient?lens_uri=...&kinds=...&hold=true` | `{ sufficient, required_kinds, require_holds }`; `kinds` is comma-separated. |

## Generated query endpoints

| REST path | xrpc path | Returns |
| --- | --- | --- |
| `GET /v1/bounties/open` | `/xrpc/dev.idiolect.query.openBounties` | Bounties whose status is `open`, `claimed`, or unset. |
| `GET /v1/bounties/want-lens?source_uri=...&target_uri=...` | `/xrpc/dev.idiolect.query.bountiesForWantLens` | Bounties requesting a lens for the schema pair. |
| `GET /v1/bounties/by-requester?requester_did=...` | `/xrpc/dev.idiolect.query.bountiesByRequester` | Bounties by requester DID. |
| `GET /v1/adapters?framework=...` | `/xrpc/dev.idiolect.query.adaptersForFramework` | Adapters declared for a framework. |
| `GET /v1/adapters/by-invocation-protocol?kind=...` | `/xrpc/dev.idiolect.query.adaptersByInvocationProtocol` | Adapters by invocation-protocol kind. |
| `GET /v1/adapters/with-verification` | `/xrpc/dev.idiolect.query.adaptersWithVerification` | Adapters that carry a verification reference. |
| `GET /v1/recommendations` | `/xrpc/dev.idiolect.query.recommendationsStartingFrom` | Recommendations starting from a given source schema. |
| `GET /v1/verifications?lens_uri=...` | `/xrpc/dev.idiolect.query.verificationsForLens` | Verifications for a specific lens. |
| `GET /v1/verifications/by-kind?kind=...` | `/xrpc/dev.idiolect.query.verificationsByKind` | Verifications by kind. |
| `GET /v1/communities?member_did=...` | `/xrpc/dev.idiolect.query.communitiesForMember` | Communities for a member DID. |
| `GET /v1/communities/by-name?name=...` | `/xrpc/dev.idiolect.query.communitiesByName` | Communities by case-insensitive name. |
| `GET /v1/dialects/for-community?community_uri=...` | `/xrpc/dev.idiolect.query.dialectsForCommunity` | Dialects owned by a community. |
| `GET /v1/beliefs/about?subject_uri=...` | `/xrpc/dev.idiolect.query.beliefsAboutRecord` | Beliefs whose subject is a given record. |
| `GET /v1/beliefs/by-holder?holder_did=...` | `/xrpc/dev.idiolect.query.beliefsByHolder` | Beliefs by holder DID. |
| `GET /v1/vocabularies/by-world?world=...` | `/xrpc/dev.idiolect.query.vocabulariesWithWorld` | Vocabularies declared with a given `world`. |
| `GET /v1/vocabularies/by-name?name=...` | `/xrpc/dev.idiolect.query.vocabulariesByName` | Vocabularies by exact name. |

The authoritative parameter list per endpoint is in
[`orchestrator-spec/queries.json`](https://github.com/idiolect-dev/idiolect/blob/main/orchestrator-spec/queries.json).
The codegen-emitted handlers live in
`crates/idiolect-orchestrator/src/generated/http.rs`.

## Pagination and response shape

Every generated list endpoint accepts `limit` (default 100, maximum
1000) and `offset` (default 0). Its response is:

```json
{
  "items": [
    { "uri": "at://did:plc:example/dev.idiolect.bounty/3l5", "author": "did:plc:example", "rev": "3l5", "record": {} }
  ],
  "total": 1,
  "limit": 100,
  "offset": 0
}
```

## Error shape

A request that fails parameter validation returns 400; internal
failures return 500. Both use `{ "error": "<code>", "message":
"<detail>" }`.

## Versioning

The `v1` and `/xrpc/` prefixes are the route contract. New
endpoints are additive. Pre-1.0 the project may rename or
restructure endpoints between minor versions. See
[Stability and versioning](./stability.md). At 1.0 the prefixes
become stable and breaking changes ship under `v2` (or, for the
xrpc surface, under new method names that deprecate the old).
