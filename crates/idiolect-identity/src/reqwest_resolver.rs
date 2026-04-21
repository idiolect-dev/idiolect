//! Reqwest-backed live [`IdentityResolver`].
//!
//! Dispatches on the DID method:
//!
//! - `did:plc:<id>` → `GET <plc_directory>/did:plc:<id>`.
//!   The caller configures the plc directory URL (default:
//!   `https://plc.directory`, the public PLC registry). A self-hosted
//!   plc mirror is a drop-in configuration.
//! - `did:web:<host>[:<path>]` → `GET https://<host>[/<path>]/.well-known/did.json`.
//!   Colons in the identifier are path separators per the did:web spec.
//!
//! No caching: callers who need a cache wrap this resolver in their
//! own. The trait is narrow enough that a caching layer is a small
//! crate of its own.
//!
//! Feature-gated under `resolver-reqwest`.

use crate::did::{Did, DidMethod};
use crate::document::DidDocument;
use crate::error::IdentityError;
use crate::resolver::IdentityResolver;

/// Live resolver over reqwest.
#[derive(Debug, Clone)]
pub struct ReqwestIdentityResolver {
    /// Base URL of a PLC directory. Typically `https://plc.directory`.
    /// No trailing slash.
    plc_directory: String,
    /// Shared http client.
    http: reqwest::Client,
}

impl ReqwestIdentityResolver {
    /// Default plc.directory endpoint.
    pub const DEFAULT_PLC_DIRECTORY: &'static str = "https://plc.directory";

    /// Construct a resolver using the public plc.directory and a
    /// default reqwest client.
    #[must_use]
    pub fn new() -> Self {
        Self::with_client(Self::DEFAULT_PLC_DIRECTORY, reqwest::Client::new())
    }

    /// Construct a resolver pointed at a specific PLC directory URL.
    ///
    /// A trailing slash in `plc_directory` is stripped before use.
    #[must_use]
    pub fn with_client(plc_directory: impl Into<String>, http: reqwest::Client) -> Self {
        let mut pd = plc_directory.into();
        if pd.ends_with('/') {
            pd.pop();
        }
        Self {
            plc_directory: pd,
            http,
        }
    }

    /// Borrow the configured PLC directory URL.
    #[must_use]
    pub fn plc_directory(&self) -> &str {
        &self.plc_directory
    }

    /// Borrow the underlying http client.
    #[must_use]
    pub const fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// URL for a did:plc resolution.
    fn plc_url(&self, did: &Did) -> String {
        format!("{}/{}", self.plc_directory, did.as_str())
    }

    /// URL for a did:web resolution per the spec:
    /// `https://<host>[/<path-with-slashes>]/.well-known/did.json`.
    fn web_url(identifier: &str) -> String {
        // did:web uses ':' as a path separator inside the identifier.
        // The first colon-separated segment is the host; the rest are
        // path segments.
        let mut parts = identifier.split(':');
        let host = parts.next().unwrap_or("");
        let path: Vec<&str> = parts.collect();
        if path.is_empty() {
            format!("https://{host}/.well-known/did.json")
        } else {
            format!("https://{host}/{}/did.json", path.join("/"))
        }
    }
}

impl Default for ReqwestIdentityResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl IdentityResolver for ReqwestIdentityResolver {
    async fn resolve(&self, did: &Did) -> Result<DidDocument, IdentityError> {
        let url = match did.method() {
            DidMethod::Plc => self.plc_url(did),
            DidMethod::Web => Self::web_url(did.identifier()),
        };

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| IdentityError::Transport(format!("GET {url}: {e}")))?;
        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(IdentityError::NotFound(did.to_string()));
        }
        if !status.is_success() {
            return Err(IdentityError::Transport(format!(
                "GET {url}: status {status}"
            )));
        }

        resp.json::<DidDocument>()
            .await
            .map_err(|e| IdentityError::InvalidDocument(format!("{url}: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_url_host_only() {
        assert_eq!(
            ReqwestIdentityResolver::web_url("example.com"),
            "https://example.com/.well-known/did.json"
        );
    }

    #[test]
    fn web_url_with_path_segments() {
        assert_eq!(
            ReqwestIdentityResolver::web_url("example.com:users:alice"),
            "https://example.com/users/alice/did.json"
        );
    }

    #[test]
    fn plc_url_prepends_directory() {
        let r = ReqwestIdentityResolver::with_client(
            "https://plc.example",
            reqwest::Client::new(),
        );
        let did = Did::parse("did:plc:abc123").unwrap();
        assert_eq!(r.plc_url(&did), "https://plc.example/did:plc:abc123");
    }

    #[test]
    fn trailing_slash_stripped() {
        let r = ReqwestIdentityResolver::with_client(
            "https://plc.example/",
            reqwest::Client::new(),
        );
        assert_eq!(r.plc_directory(), "https://plc.example");
    }
}
