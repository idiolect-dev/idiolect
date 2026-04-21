//! Loading the bundled session lexicon as a panproto [`Schema`].
//!
//! The Lexicon document at `lexicons/session.json` is included at
//! compile time via `include_str!` and parsed at runtime through
//! [`panproto_protocols::web_document::atproto::parse_lexicon`]. The
//! return value is a real panproto [`Schema`] graph; lenses
//! compiled against it behave exactly like lenses over any
//! `dev.idiolect.*` record schema.
//!
//! The session nsid deliberately uses the `dev.idiolect.internal.*`
//! prefix. It's in the same reversed-DNS root so panproto's atproto
//! parser accepts it, but the `internal.` segment flags that this
//! nsid is not a first-party PDS-published record. No firehose
//! consumer subscribing to `dev.idiolect.*` receives these, because
//! the appview never writes them to a PDS in the first place.

use panproto_protocols::web_document::atproto;
use panproto_schema::Schema;

/// The fully-qualified nsid of the OAuth session schema.
pub const SESSION_NSID: &str = "dev.idiolect.internal.oauthSession";

/// The bundled session lexicon json. Exposed so callers who want to
/// ship the document alongside their own lexicon tree can inline it
/// without touching the filesystem.
pub const SESSION_LEXICON_JSON: &str = include_str!("../lexicons/session.json");

/// Errors raised while loading the session schema.
#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    /// The bundled lexicon json failed to parse. This is a bug in
    /// this crate, not runtime data.
    #[error("parse bundled session lexicon json: {0}")]
    ParseJson(#[from] serde_json::Error),

    /// The lexicon parsed but `atproto::parse_lexicon` rejected it.
    /// This is a bug in this crate or an incompatible panproto
    /// version, not runtime data.
    #[error("parse lexicon as panproto schema: {0}")]
    ParseLexicon(String),
}

/// Load the OAuth session schema as a panproto [`Schema`].
///
/// The result is stable across calls: the bundled json is fixed at
/// compile time, and `atproto::parse_lexicon` is deterministic. Call
/// sites that want to memoize can wrap the result in a `OnceLock`.
///
/// # Errors
///
/// Returns [`SchemaError`] if the bundled lexicon fails to round-
/// trip through json or through the panproto atproto parser; both
/// are compile-time bugs in this crate.
pub fn session_schema() -> Result<Schema, SchemaError> {
    let value: serde_json::Value = serde_json::from_str(SESSION_LEXICON_JSON)?;
    atproto::parse_lexicon(&value).map_err(|e| SchemaError::ParseLexicon(e.to_string()))
}

/// Default source-root vertex for a session w-instance.
///
/// atproto record lexicons parse into a schema whose primary entry
/// is the outer record vertex. The body (flat top-level fields)
/// lives at `{nsid}:body`; routing json through the body vertex
/// matches the `OAuthSession` struct's wire form.
#[must_use]
pub fn session_body_vertex() -> String {
    format!("{SESSION_NSID}:body")
}
