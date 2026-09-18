# Community workspace manifest

`idiolect.toml` is the local control document for a community workspace. Field names use TOML spelling emitted by `toml`; the Rust structs use `serde(rename_all = "camelCase")`.

## Top level

| Field | Type | Required | Meaning |
|---|---:|:---:|---|
| `manifestVersion` | integer | default `1` | On-disk manifest format. Other versions are rejected. |
| `community` | table | yes | Community identity and purpose. |
| `packages` | array of tables | default empty | Governed schema or vocabulary roots. |
| `authorities` | array of tables | default empty | DIDs and roles allowed to review or publish. |
| `governance` | table | defaults | Decision and verification policy. |
| `resources` | table | defaults | Limits for untrusted inputs and bounded work. |
| `federation` | array of tables | default empty | Peer-community dependencies. |
| `release` | table | defaults | Bundle location and signing threshold. |
| `exit` | table | defaults | Material included in portable exports. |

## `[community]`

| Field | Type | Meaning |
|---|---:|---|
| `name` | string | Participant-facing name. Must not be blank. |
| `did` | string | Community or delegated-service DID. Must begin with `did:`. |
| `description` | string | Plain-language purpose and scope. |
| `record` | string, optional | Published `dev.idiolect.community` at-uri. |

## `[[packages]]`

| Field | Type | Default | Meaning |
|---|---:|---:|---|
| `name` | string | none | Unique stable local name. |
| `path` | string | none | Workspace-relative path. Absolute paths and parent traversal are rejected. |
| `protocol` | string | `atproto` | Canonical Panproto protocol registry key. |
| `crossDocument` | boolean | `false` | Documents are intended to resolve internal references together. |

## `[[authorities]]`

| Field | Type | Meaning |
|---|---:|---|
| `did` | string | Authorized participant DID. Duplicate DIDs are rejected. |
| `roles` | string array | Roles accepted in reviews, such as `member`, `maintainer`, or `steward`. |

## `[governance]`

| Field | Type | Default | Meaning |
|---|---:|---:|---|
| `model` | enum | `maintainer` | `maintainer`, `consent`, `vote`, `steward`, or `hybrid`. |
| `minApprovals` | integer | `1` | Minimum approvals required by the selected model. |
| `quorum` | integer | `1` | Minimum distinct participating reviewers. |
| `approvalThreshold` | number | `0.5` | Vote approval ratio in the inclusive range 0–1. |
| `reviewPeriodDays` | integer | `3` | Minimum public review period before release. |
| `stewardReviewFor` | string array | `breaking`, `data-loss` | Consequences that require approving steward review. |
| `requiredVerifications` | string array | `schema-compatibility` | Kinds that need a verified result before release. |

The library evaluates `reviewPeriodDays` against packet creation time during release construction. A zero value disables the time delay but does not bypass reviews or verification.

## `[resources]`

| Field | Type | Default | Meaning |
|---|---:|---:|---|
| `maxDocuments` | integer | `256` | Maximum schema documents in one operation. |
| `maxSchemaBytes` | integer | `16777216` | Maximum total schema input bytes. |
| `maxSearchSteps` | integer | `100000` | Migration-search work-unit budget. |
| `maxVerificationCases` | integer | `10000` | Maximum cases in one verification run. |

## `[[federation]]`

| Field | Type | Default | Meaning |
|---|---:|---:|---|
| `community` | string | none | Peer community record or DID. |
| `relation` | string | none | `follows`, `extends`, `bridges`, or `forked-from`. |
| `release` | string, optional | none | Exact release or version requirement. |
| `updatePolicy` | string | `review` | `review`, `follow-compatible`, or `pinned`. |
| `mappings` | string array | empty | Published lens or mapping at-uris. |

## `[release]`

| Field | Type | Default | Meaning |
|---|---:|---:|---|
| `directory` | string | `.idiolect/releases` | Workspace-relative release output directory. |
| `minSignatures` | integer | `1` | Minimum distinct valid ES256 authority signatures. |
| `publicationTarget` | string, optional | none | PDS or registry target recorded for deployment tooling. |

## `[exit]`

All four fields are booleans and default to `true`: `includeHistory`, `includePackages`, `includeMigrations`, and `includeReleases`.

## Validation commands

```console
idiolect check --workspace .
idiolect doctor --workspace . --json
```

The JSON doctor output is stable enough for CI policy gates. Consumers should match diagnostic `code`, not prose `message`.
