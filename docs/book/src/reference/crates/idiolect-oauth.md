# idiolect-oauth

> **Source:** [`crates/idiolect-oauth/`](https://github.com/idiolect-dev/idiolect/tree/main/crates/idiolect-oauth)
>
> This crate is `publish = false` and is not on docs.rs. The
> authoritative reference is the source above plus the rustdoc
> built locally with `cargo doc -p idiolect-oauth --open`.

The crate provides AT Protocol [OAuth](../../glossary.md#oauth) session storage through its
token-store trait and shipped implementations. The OAuth dance itself lives
in `atrium-oauth-client`, and the DPoP signer lives in
`idiolect-lens` under the `dpop-p256` feature.

Because the crate is `publish = false`, depend via git or path:

```toml
[dependencies]
idiolect-oauth = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.12.1", features = ["store-filesystem"] }
```

## Public surface

`OAuthTokenStore` is the trait every store implements. Its
methods are `save` / `load` / `delete` keyed by DID, plus a
defaulted `list_dids`.
`OAuthSession` carries the access token, refresh token, expiry,
and DPoP key. The session has helpers (`is_expired`,
`time_until_expiry`, `needs_refresh(now, threshold)`,
`refresh_expired`) for callers that want to drive their own
refresh policy.

For the common case, the crate also ships `refresh_if_needed`
and a `Refresher` trait: `refresh_if_needed(store, refresher,
did)` loads the session, decides whether to refresh (wall clock
plus a `DEFAULT_REFRESH_THRESHOLD_SECS` buffer), drives the
caller-supplied `Refresher` if so, persists the result, and
returns the live session.

## Shipped stores

| Store | Feature | Backing |
| --- | --- | --- |
| `InMemoryOAuthTokenStore` | (always) | `HashMap`-backed; for tests. |
| `FilesystemOAuthTokenStore` | `store-filesystem` | One JSON file per session. |
| `SqliteOAuthTokenStore` | `store-sqlite` | One row per session. |

All three implement `OAuthTokenStore`. The trait uses native async
methods and is not object-safe. Callers thus parameterize their
application over a store type or define an object-safe adapter.

## Errors

`StoreError` covers store-side failures. `SessionError` covers
session-shape failures. Callers that want a flattened error type
build their own at the application boundary.

## Feature flags

| Feature | Adds |
| --- | --- |
| `store-filesystem` | The filesystem-backed session store. |
| `store-sqlite` | The SQLite-backed session store. |

## DPoP keys

The session carries a Demonstrating Proof of Possession
([DPoP](../../glossary.md#dpop)) private key as a JWK. Persistence is the store's
responsibility; both shipped stores persist it alongside the
session. A custom store must do the same. The OAuth RFC
does not define this key; [RFC 9449](https://www.rfc-editor.org/rfc/rfc9449.html)
defines DPoP key binding. Reusing the bound key is necessary for
requests made with the same DPoP-bound token.

The signer behind the DPoP-bound HTTP layer is `P256DpopProver`
in `idiolect-lens` under the `dpop-p256` feature. The lens
crate's `SigningPdsWriter` wraps a `DpopProver` so every PDS
write sends a DPoP-bound proof header.
