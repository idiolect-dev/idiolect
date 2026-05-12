# Configure OAuth sessions

[`idiolect-oauth`](../reference/crates/idiolect-oauth.md)
provides the token-store trait and three shipped implementations.
The crate does *not* implement the OAuth dance itself: that
lives in `atrium-oauth-client`. The crate also does *not* ship a
`refresh_if_needed` helper or a CLI login command; consumers
drive the dance and the refresh decision in their own code,
using `OAuthSession`'s helpers (`is_expired`, `needs_refresh`,
`refresh_expired`) for timing.

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

All three implement `OAuthTokenStore`. Anything that takes
`Arc<dyn OAuthTokenStore>` accepts any of them.

`idiolect-oauth` is `publish = false`; depend via git.

## Filesystem store

```toml
idiolect-oauth = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.8.0", features = ["store-filesystem"] }
```

```rust
use idiolect_oauth::{FilesystemOAuthTokenStore, OAuthTokenStore};

let store = FilesystemOAuthTokenStore::open("./sessions/")?;

// Write a session (returned by the OAuth dance, not by this crate):
store.save(&session).await?;

// Read it back later:
let recovered = store.load(&session.did).await?;
```

The directory contains one JSON file per session keyed by DID.

## SQLite store

```toml
idiolect-oauth = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.8.0", features = ["store-sqlite"] }
```

```rust
use idiolect_oauth::{SqliteOAuthTokenStore, OAuthTokenStore};

let store = SqliteOAuthTokenStore::open("sessions.sqlite").await?;
```

## Drive the OAuth dance

The dance itself is `atrium-oauth-client`'s job; the crate
returns an authenticated session you store via
`OAuthTokenStore::save`. The session shape (`OAuthSession`) is
documented in the crate's source: it carries the DID, PDS URL,
access JWT, refresh JWT, DPoP private key (JWK-serialized),
DPoP nonce, and expiry timestamps as public fields.

For session-staleness decisions, read `OAuthSession::is_expired`
and `OAuthSession::needs_refresh(now, threshold)` in your own
refresh path; the crate does not ship a `refresh_if_needed`
helper, and the application decides how to drive the refresh
endpoint.

## DPoP

The session's DPoP keypair is what makes the access token bound.
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
ATProto is moving off app passwords in favour of OAuth +
DPoP; the next-iteration login UX wraps `atrium-oauth`'s
browser-handoff dance and persists the resulting DPoP-bound
session through the same `OAuthTokenStore`. The `OAuthSession`
shape already carries the DPoP private key field; switching
flows is a CLI substitution, not a session-shape change.

## `refresh_if_needed`

`idiolect_oauth::refresh_if_needed(&store, &refresher, did)`
loads a session, decides whether to refresh based on the
current wall clock plus a 60-second buffer, drives the caller-
supplied `Refresher::refresh` if so, persists the result, and
returns the live session. Callers who want to drive the
decision themselves read `OAuthSession::needs_refresh` and
`OAuthSession::is_expired` directly.

```rust
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
