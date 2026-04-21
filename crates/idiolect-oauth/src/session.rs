//! The [`OAuthSession`] value — the serde mirror of the bundled
//! session lexicon.
//!
//! A session is the minimum persistent state an atproto OAuth client
//! needs to keep between requests: the pair of tokens, the `DPoP` key
//! material, the current `DPoP` nonce, and enough metadata to route
//! refreshes back to the issuing PDS.
//!
//! The struct's field set tracks the session lexicon in
//! `lexicons/session.json`. When you add a field here, add it there
//! too; the round-trip test asserts every required field survives a
//! lens application, which catches drift between the two.

use serde::{Deserialize, Serialize};

/// Persistent OAuth session state for one authenticated account.
///
/// The fields map one-to-one to the schema declared in
/// `lexicons/session.json`; serde names are camel-case so the struct
/// round-trips through the schema's json wire form without
/// translation.
///
/// # Secrets
///
/// `access_jwt`, `refresh_jwt`, and `dpop_private_key_jwk` are
/// sensitive. `Debug` is derived for development ergonomics, but
/// callers must filter these values out of any production log sink:
/// even the refresh token is enough to mint fresh access tokens until
/// the account re-authenticates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthSession {
    /// DID of the authenticated account.
    pub did: String,

    /// Base URL of the PDS that issued the tokens. Refreshes must
    /// hit the same host.
    pub pds_url: String,

    /// Opaque access JWT. Never log or render.
    pub access_jwt: String,

    /// Opaque refresh JWT. Never log or render.
    pub refresh_jwt: String,

    /// `DPoP` private key in `JWK`-serialized form. Never log or
    /// render.
    pub dpop_private_key_jwk: String,

    /// Latest `DPoP` nonce the PDS returned. Updated on every request
    /// that yields a fresh value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpop_nonce: Option<String>,

    /// OAuth 2.0 `token_type`. Always `"DPoP"` for atproto today.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_type: Option<String>,

    /// Space-delimited scope string the PDS returned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    /// User-facing handle at issuance time. Authoritative source is
    /// the identity layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,

    /// When the session was issued or last refreshed (RFC 3339).
    pub issued_at: String,

    /// Access-token expiry (RFC 3339). Callers must refresh before
    /// this instant.
    pub expires_at: String,

    /// Optional refresh-token absolute expiry (RFC 3339). Past this,
    /// the user must re-authenticate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_expires_at: Option<String>,
}

impl OAuthSession {
    /// Convenience constructor for the required fields. Optional
    /// metadata (nonce, scope, handle, refresh expiry) can be set
    /// after construction.
    #[must_use]
    pub fn new(
        did: impl Into<String>,
        pds_url: impl Into<String>,
        access_jwt: impl Into<String>,
        refresh_jwt: impl Into<String>,
        dpop_private_key_jwk: impl Into<String>,
        issued_at: impl Into<String>,
        expires_at: impl Into<String>,
    ) -> Self {
        Self {
            did: did.into(),
            pds_url: pds_url.into(),
            access_jwt: access_jwt.into(),
            refresh_jwt: refresh_jwt.into(),
            dpop_private_key_jwk: dpop_private_key_jwk.into(),
            dpop_nonce: None,
            token_type: None,
            scope: None,
            handle: None,
            issued_at: issued_at.into(),
            expires_at: expires_at.into(),
            refresh_expires_at: None,
        }
    }

    // -----------------------------------------------------------------
    // expiry helpers
    // -----------------------------------------------------------------

    /// Has the access token expired as of `now`?
    ///
    /// Parses [`expires_at`](Self::expires_at) as RFC 3339. A malformed
    /// timestamp is treated as expired — better to force a refresh
    /// than to trust an unparseable value.
    #[must_use]
    pub fn is_expired(&self, now: time::OffsetDateTime) -> bool {
        // Unparseable timestamps fail safe and read as expired so
        // the caller forces a refresh.
        parse_rfc3339(&self.expires_at).is_none_or(|t| now >= t)
    }

    /// Duration until `expires_at`. Returns `Duration::ZERO` if the
    /// token is already expired or the timestamp is unparseable.
    #[must_use]
    pub fn time_until_expiry(&self, now: time::OffsetDateTime) -> time::Duration {
        match parse_rfc3339(&self.expires_at) {
            Some(t) if t > now => t - now,
            _ => time::Duration::ZERO,
        }
    }

    /// Should this session be refreshed proactively?
    ///
    /// Returns `true` when the access token will expire within
    /// `threshold` from `now`. A typical refresh daemon polls with
    /// `threshold = Duration::minutes(5)` — refresh as soon as the
    /// token has less than five minutes of life.
    ///
    /// Also returns `true` if [`expires_at`](Self::expires_at) is
    /// malformed, matching [`is_expired`](Self::is_expired)'s
    /// fail-safe behavior.
    #[must_use]
    pub fn needs_refresh(&self, now: time::OffsetDateTime, threshold: time::Duration) -> bool {
        parse_rfc3339(&self.expires_at).is_none_or(|t| (t - now) <= threshold)
    }

    /// Has the refresh token itself expired? `false` if
    /// `refresh_expires_at` is unset (meaning the PDS did not
    /// advertise an absolute refresh deadline).
    #[must_use]
    pub fn refresh_expired(&self, now: time::OffsetDateTime) -> bool {
        self.refresh_expires_at
            .as_deref()
            .and_then(parse_rfc3339)
            .is_some_and(|t| now >= t)
    }
}

/// Parse an RFC 3339 timestamp into an `OffsetDateTime`. Returns
/// `None` on any parse error so callers can decide how to react —
/// `is_expired` and `needs_refresh` treat unparseable values as
/// expired (fail-safe); `refresh_expired` treats them as not-yet-
/// expired (fail-open, consistent with "no refresh deadline declared").
fn parse_rfc3339(s: &str) -> Option<time::OffsetDateTime> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
}

/// Errors produced while mapping [`OAuthSession`] to and from its
/// panproto w-instance representation.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The session could not be serialized to json — typically a
    /// bug in the struct definition, not runtime data.
    #[error("serialize session to json: {0}")]
    Serialize(serde_json::Error),

    /// The json payload did not match the expected session schema.
    #[error("deserialize session from json: {0}")]
    Deserialize(serde_json::Error),
}
