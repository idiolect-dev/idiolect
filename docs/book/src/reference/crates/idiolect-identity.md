# idiolect-identity

> **API reference:** [docs.rs/idiolect-identity](https://docs.rs/idiolect-identity/latest/idiolect_identity/)
> · **Source:** [`crates/idiolect-identity/`](https://github.com/idiolect-dev/idiolect/tree/main/crates/idiolect-identity)
> · **Crate:** [crates.io/idiolect-identity](https://crates.io/crates/idiolect-identity)
>
> This page is an editorial overview. The per-symbol surface
> (every public type, trait, function, and feature flag) is the
> docs.rs link above. That is the authoritative reference.

The crate resolves a decentralized identifier
([DID](../../glossary.md#did)) to a structured `DidDocument`
carrying the also-known-as set, service entries, verification
methods, and any additional fields the source document carries.

```toml
[dependencies]
idiolect-identity = { version = "0.12.0", features = ["resolver-reqwest"] }
```

## Public surface

`IdentityResolver` is the trait every resolver implements. The
crate ships three implementations.

| Type | Feature | Backing |
| --- | --- | --- |
| `InMemoryIdentityResolver` | (always) | `HashMap<Did, DidDocument>`. Tests and fixtures. |
| `ReqwestIdentityResolver` | `resolver-reqwest` | Reqwest-backed; resolves `did:plc` via plc.directory and `did:web` via `.well-known/did.json`. |
| `CachingIdentityResolver<R>` | (always) | TTL'd cache wrapping any inner resolver. |

`DidDocument` carries the resolved data. The shipped accessors
include `handle()`, `pds_url()`, and the underlying
`also_known_as` field. See docs.rs for the full surface.

## Errors

`IdentityError` is the single error type the crate exposes.
Variants distinguish transport failures, parse failures, and
unsupported DID methods.

## Feature flags

| Feature | Adds |
| --- | --- |
| `resolver-reqwest` | The `ReqwestIdentityResolver` implementation. |

## Caching

The shipped `CachingIdentityResolver` wraps any inner resolver
with the `Duration` supplied to `CachingIdentityResolver::new`.
Cache hits skip the HTTP request entirely. Cache misses fall
through to the inner resolver. Errors are not cached.
