//! Error types for community workspace operations.

use std::path::PathBuf;

/// Result alias for community operations.
pub type CommunityResult<T> = Result<T, CommunityError>;

/// Failures produced while loading, validating, analysing, or exporting a
/// community workspace.
#[derive(Debug, thiserror::Error)]
pub enum CommunityError {
    /// A filesystem operation failed.
    #[error("I/O error at {path}: {source}")]
    Io {
        /// Path involved in the operation.
        path: PathBuf,
        /// Underlying I/O failure.
        source: std::io::Error,
    },
    /// TOML could not be decoded.
    #[error("invalid workspace manifest: {0}")]
    ManifestDecode(#[from] toml::de::Error),
    /// TOML could not be encoded.
    #[error("could not encode workspace manifest: {0}")]
    ManifestEncode(#[from] toml::ser::Error),
    /// JSON could not be decoded or encoded.
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// The workspace or an artifact failed semantic validation.
    #[error("invalid community artifact: {0}")]
    Invalid(String),
    /// Panproto could not parse or compare a schema.
    #[error("schema analysis failed: {0}")]
    Analysis(String),
    /// Governance has not authorized an operation.
    #[error("governance gate held: {0}")]
    Governance(String),
    /// A signature could not be produced or verified.
    #[error("signature error: {0}")]
    Signature(String),
}

impl CommunityError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
