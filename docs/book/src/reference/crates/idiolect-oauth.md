# idiolect-oauth

> **Source:** [`crates/idiolect-oauth/`](https://github.com/idiolect-dev/idiolect/tree/main/crates/idiolect-oauth)
>
> This crate is `publish = false` and is not on docs.rs. The
> authoritative reference is the source above plus the rustdoc
> built locally with `cargo doc -p idiolect-oauth --open`.

ATProto OAuth session storage. The crate carries the token-store
trait and shipped implementations; the OAuth dance itself lives
in `atrium-oauth-client` and the DPoP signer lives in
`idiolect-lens` under the `dpop-p256` feature.

Because the crate is `publish = false`, depend via git or path:

```toml
[dependencies]
idiolect-oauth = { git = "https://github.com/idiolect-dev/idiolect", tag = "v0.8.0", features = ["store-filesystem"] }
```

## Public surface

`OAuthTokenStore` is the trait every store implements; the
typical surface is `get` / `put` / `delete` keyed by DID.
`OAuthSession` carries the access token, refresh token, expiry,
and DPoP key. The session has helpers (`is_expired`,
`time_until_expiry`, `needs_refresh(now, threshold)`,
`refresh_expired`) for callers that want to drive their own
refresh policy.

## Shipped stores

| Store | Feature | Backing |
| --- | --- | --- |
| `InMemoryOAuthTokenStore` | (always) | `HashMap`-backed; for tests. |
| `FilesystemOAuthTokenStore` | `store-filesystem` | One JSON file per session. |
| `SqliteOAuthTokenStore` | `store-sqlite` | One row per session. |

All three implement `OAuthTokenStore`. Anything that takes
`Arc<dyn OAuthTokenStore>` accepts any of them.

## Errors

`StoreError` covers store-side failures; `SessionError` covers
session-shape failures. Callers that want a flattened error type
build their own at the application boundary.

## Feature flags

| Feature | Adds |
| --- | --- |
| `store-filesystem` | The filesystem-backed session store. |
| `store-sqlite` | The SQLite-backed session store. |

## DPoP keys

The session carries a DPoP keypair. Persistence is the store's
responsibility; both shipped stores persist it alongside the
session. A custom store must do the same; the OAuth RFC
requires DPoP keys to survive across requests.

The signer behind the DPoP-bound HTTP layer is `P256DpopProver`
in `idiolect-lens` under the `dpop-p256` feature. The lens
crate's `SigningPdsWriter` wraps a `DpopProver` so every PDS
write sends a DPoP-bound proof header.
