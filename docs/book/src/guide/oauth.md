# Configure OAuth sessions

[`idiolect-oauth`](../reference/crates/idiolect-oauth.md) stores
[OAuth](../glossary.md#oauth "An authorization framework for delegated access")
sessions after an application completes the authorization flow. The
crate supplies `OAuthTokenStore`, three stores, and refresh timing; an
OAuth client supplies the network exchange.

## When you need it

Anything that publishes records (encounter, recommendation,
verification, observation, lens, dialect, vocab, ...) needs an
authenticated PDS session. Reading records does not.

## Pick a store

| Store | Feature | Use when |
| --- | --- | --- |
| `InMemoryOAuthTokenStore` | (always) | Tests and fixtures. |
| `FilesystemOAuthTokenStore` | `store-filesystem` | A single operator process running on one host. Sessions live under a directory; one file per DID. |
| `SqliteOAuthTokenStore` | `store-sqlite` | Multi-process or multi-tenant deployments. Concurrent reads, fsync per write. |

All three implement `OAuthTokenStore`. Its native async methods make
the trait non-object-safe, so callers remain generic over
`S: OAuthTokenStore` rather than using `Arc<dyn OAuthTokenStore>`.

`idiolect-oauth` is `publish = false`; depend via git.

## Filesystem store

```toml
idiolect-oauth = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.11.1", features = ["store-filesystem"] }
```

```text
use idiolect_oauth::{FilesystemOAuthTokenStore, OAuthTokenStore};

std::fs::create_dir_all("./sessions/")?;
let store = FilesystemOAuthTokenStore::new("./sessions/")?;

// Write a session (returned by the OAuth dance, not by this crate):
store.save(&session).await?;

// Read it back later:
let recovered = store.load(&session.did).await?; // Option<OAuthSession>
```

The directory contains one JSON file per session keyed by DID.

## SQLite store

```toml
idiolect-oauth = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.11.1", features = ["store-sqlite"] }
```

```text
use idiolect_oauth::{SqliteOAuthTokenStore, OAuthTokenStore};

let store = SqliteOAuthTokenStore::open("sessions.sqlite")?;
```

## Drive the OAuth dance

The authorization client returns an authenticated session that you
store through `OAuthTokenStore::save`. The `OAuthSession` shape is
documented in the crate's source: it carries the DID, PDS URL,
access JWT, refresh JWT, DPoP private key (JWK-serialized),
DPoP nonce, and expiry timestamps as public fields.

For session-staleness decisions, either call `refresh_if_needed`
(documented below) or read `OAuthSession::is_expired` and
`OAuthSession::needs_refresh(now, threshold)` in your own refresh
path. Either way, the application supplies the `Refresher` that
drives the refresh endpoint.

## DPoP

The session's
[DPoP](../glossary.md#dpop "Proof-of-possession binding for OAuth access tokens")
keypair binds the access token.
The signer (the `P256DpopProver` in
[`idiolect-lens`](../reference/crates/idiolect-lens.md) under
the `dpop-p256` feature) consumes the keypair from the session
and signs every PDS write through `SigningPdsWriter`.

Persisting the DPoP key with the session is the store's job.
Both shipped persistent stores
(`FilesystemOAuthTokenStore`, `SqliteOAuthTokenStore`) do; if
you write a custom store, do the same.

## `idiolect oauth login` (transitional)

The `idiolect` CLI ships an `oauth login` subcommand that
exchanges a handle + app password for an access JWT via
`com.atproto.server.createSession` and persists the resulting
session as a JSON file under
`$IDIOLECT_SESSION_DIR` (default
`~/.config/idiolect/sessions/`):

```bash
idiolect oauth login --handle yourhandle.bsky.social --pds-url https://bsky.social
# password from --app-password or ATPROTO_APP_PASSWORD / ATPROTO_PASSWORD env
idiolect oauth list
idiolect oauth logout --did did:plc:...
```

This path uses **app passwords in legacy Bearer mode**.
This CLI path is separate from `OAuthSession`: its JSON file contains
the DID, handle, PDS URL, access JWT, and refresh JWT, but no DPoP key.
Use the library store for a DPoP-bound OAuth deployment.

## `refresh_if_needed`

`idiolect_oauth::refresh_if_needed(&store, &refresher, did)`
loads a session, decides whether to refresh based on the
current wall clock plus a 60-second buffer, drives the caller-
supplied `Refresher::refresh` if so, persists the result, and
returns the live session. Callers who want to drive the
decision themselves read `OAuthSession::needs_refresh` and
`OAuthSession::is_expired` directly.

```text
use idiolect_oauth::{refresh_if_needed, Refresher, RefreshError, OAuthSession};

struct MyRefresher { /* http client, auth-server URL, ... */ }

impl Refresher for MyRefresher {
    async fn refresh(&self, session: &OAuthSession) -> Result<OAuthSession, RefreshError> {
        // POST refresh_token to the auth-server's token_endpoint.
        // Return a fresh OAuthSession with new access_jwt / expires_at.
        todo!()
    }
}

let fresh = refresh_if_needed(&store, &MyRefresher { /* ... */ }, "did:plc:...").await?;
```

The trait is narrow on purpose: the refresh HTTP call lives in
whatever OAuth client the application uses (atrium-oauth, a
hand-rolled reqwest call, an in-memory fake for tests).
`idiolect-oauth` owns the storage and timing decision around it.
