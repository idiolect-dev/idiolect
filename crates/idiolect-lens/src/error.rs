//! Error types for the lens runtime.
//!
//! Every fallible operation in this crate returns [`LensError`]. The
//! variants are intentionally narrow: `Resolver` implementations
//! translate their own transport or storage failures into the
//! appropriate `LensError` case, so downstream callers only pattern-
//! match one enum regardless of backend.

use thiserror::Error;

/// Errors produced by the lens runtime.
///
/// `Resolver` and `SchemaLoader` implementations wrap backend-specific
/// failures (http errors, decode errors, missing refs) into these
/// variants so callers never need to reach for a backend-specific
/// error type.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LensError {
    /// The at-uri could not be parsed.
    #[error("invalid at-uri: {0}")]
    InvalidUri(String),

    /// The resolver or schema loader could not locate the requested
    /// artifact.
    ///
    /// For `InMemoryResolver`, this means the `HashMap` had no entry.
    /// For `PdsResolver`, this means the PDS returned a
    /// `RecordNotFound`-shaped error. For `PanprotoVcsResolver`, the
    /// ref table had no entry for the at-uri, or the object-hash was
    /// absent from the content-addressed store.
    #[error("no record found for {0}")]
    NotFound(String),

    /// The resolver or schema loader reached the backend but the
    /// backend reported an error.
    ///
    /// Use this for http 5xx, vcs fetch failures, and other
    /// transport-level problems. The string carries the backend's
    /// message verbatim.
    #[error("lens transport error: {0}")]
    Transport(String),

    /// The backend returned bytes, but they did not decode into the
    /// expected json shape (a `PanprotoLens` record, a lexicon json
    /// document, or a serialized protolens).
    #[error("lens decode failed: {0}")]
    Decode(#[from] serde_json::Error),

    /// The lens record had no body: neither an inline protolens blob
    /// nor (later) a cid-link to fetch.
    #[error("lens record at {0} has no protolens body")]
    MissingBody(String),

    /// The source or target lexicon json failed to parse into a
    /// panproto [`panproto_schema::Schema`].
    #[error("lexicon parse failed: {0}")]
    LexiconParse(String),

    /// Instantiating the protolens against the source schema failed.
    /// Wraps [`panproto_lens::LensError`].
    #[error("protolens instantiation failed: {0}")]
    Instantiate(String),

    /// Parsing the source record json into a panproto w-type instance
    /// failed.
    #[error("instance parse failed: {0}")]
    InstanceParse(String),

    /// Applying the lens (get or put direction) failed at the panproto
    /// layer.
    #[error("lens apply failed: {0}")]
    Translate(String),
}
