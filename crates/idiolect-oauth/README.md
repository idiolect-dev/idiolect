# idiolect-oauth

Panproto-native schema and store trait for atproto OAuth session state.

## Overview

OAuth tokens do not travel back to a PDS as published records. They are
ephemeral secrets held by the authoring client. The crate still models them
with a panproto schema. We call this the internal-state schema pattern: local
state such as sessions, firehose cursors, identity caches, and DPoP nonces
uses the same schema operations without entering the federated record stream.

The schema lives in-crate at `lexicons/session.json`, parsed via
`panproto_protocols::web_document::atproto::parse_lexicon` into a
`panproto_schema::Schema`. The matching Rust struct is `OAuthSession`;
the persistence boundary is `OAuthTokenStore`.

## Architecture

```mermaid
flowchart LR
    SCHEMA["lexicons/session.json<br/>(dev.idiolect.internal.oauthSession)"]
    PP["panproto parse_lexicon"]
    SESS["OAuthSession struct<br/>{ access_jwt, refresh_jwt,<br/>dpop_jwk, expiries }"]

    subgraph store["OAuthTokenStore"]
        MEM["InMemoryOAuthTokenStore"]
        FS["FilesystemOAuthTokenStore<br/>(encrypted)"]
        SQL["SqliteOAuthTokenStore<br/>(WAL)"]
    end

    SW["idiolect-lens::<br/>SigningPdsWriter"]
    PDS[("PDS<br/>authenticated writes")]

    SCHEMA --> PP --> SESS
    SESS --> MEM
    SESS --> FS
    SESS --> SQL
    MEM --> SW
    FS --> SW
    SQL --> SW
    SW --> PDS
```

## Usage

```rust
use idiolect_oauth::{OAuthSession, OAuthTokenStore, InMemoryOAuthTokenStore};

let store = InMemoryOAuthTokenStore::new();
let session = OAuthSession::new(
    "did:plc:alice",
    "https://pds.example",
    "access-jwt",
    "refresh-jwt",
    "dpop-jwk",
    "2026-04-19T00:00:00Z",
    "2026-04-19T01:00:00Z",
);

store.save(&session).await?;
let loaded = store.load("did:plc:alice").await?;

// Proactive refresh via expiry helpers.
use time::{OffsetDateTime, Duration};
if session.needs_refresh(OffsetDateTime::now_utc(), Duration::minutes(5)) {
    refresh_and_save(&store).await?;
}
```

## Feature flags

| Flag | Default | Effect |
| ---- | ------- | ------ |
| `store-filesystem` | off | `FilesystemOAuthTokenStore`: one encrypted file per session. |
| `store-sqlite` | off | `SqliteOAuthTokenStore`: one row per session, WAL-journaled. |

## Security

The `access_jwt`, `refresh_jwt`, and `dpop_private_key_jwk` fields are
secrets. `Debug` is derived for development ergonomics but must be
filtered from production logs. Disk-backed stores are responsible for
encryption at rest. The refresh token grants full repo write until the
account re-authenticates.

## Design notes

- The schema's nsid is `dev.idiolect.internal.oauthSession`. The
  `internal.` segment keeps the namespace under `dev.idiolect.*` (so
  panproto's atproto parser accepts it) while flagging consumers that
  this nsid never appears on a PDS firehose.
- Lenses over w-instances of the session schema express token lifecycle
  as panproto operations: issue via `put`, refresh via a field-rewriting
  `get`, and revoke via a token-dropping `put`. Those lenses
  live in whichever component needs them. This crate ships only the
  schema, the struct, and the store trait.

## Stability

idiolect is pre-1.0. Minor releases may change Rust APIs, lexicon shapes,
wire formats, or CLI surfaces. Pin an exact version if you depend on this
crate, and read [CHANGELOG.md](../../CHANGELOG.md) before upgrading.

## Related

- [`idiolect-lens`](../idiolect-lens): `SigningPdsWriter` consumes
  sessions from this crate's store to authenticate record writes.
