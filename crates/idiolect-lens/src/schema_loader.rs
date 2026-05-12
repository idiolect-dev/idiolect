//! Load a `panproto_schema::Schema` by its content-addressed hash.
//!
//! The [`PanprotoLens`](idiolect_records::PanprotoLens) record names
//! its source and target schemas by `object_hash`; to instantiate the
//! lens we have to turn each hash into a concrete
//! [`panproto_schema::Schema`] graph.
//!
//! That lookup lives behind [`SchemaLoader`] so the same lens runtime
//! works against an in-memory fixture, a live PDS schema fetch, or a
//! panproto vcs store. An [`InMemorySchemaLoader`] is shipped for
//! tests and offline fixtures.

use std::collections::HashMap;

use panproto_schema::Schema;

use crate::error::LensError;

/// Backend-agnostic loader for panproto [`Schema`] graphs.
///
/// Implementations should return [`LensError::NotFound`] when no
/// schema matches the given hash and [`LensError::Transport`] for
/// backend-level failures.
pub trait SchemaLoader: Send + Sync {
    /// Load the schema whose content-addressed hash is `object_hash`.
    ///
    /// The contract is opaque to the kind of schema the loader hands
    /// back: a single-file schema (the kind panproto's
    /// `dev.panproto.node.getFileSchema` returns), a project-scope
    /// schema unioned across many files (`getProjectSchema`), or
    /// anything content-addressed and deserialisable as a panproto
    /// `Schema`. Dialects routinely span several source schemas, so
    /// the runtime intentionally avoids assuming a particular scope.
    /// It asks the loader for "the schema at this hash" and
    /// instantiates against whatever it gets back.
    ///
    /// # Errors
    ///
    /// See [`LensError`] variants. `NotFound` when the hash is
    /// unknown; `Transport` when the backend fails; `LexiconParse`
    /// when the backend returned bytes that were not a valid
    /// panproto schema graph.
    fn load<'a>(
        &'a self,
        object_hash: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Schema, LensError>> + Send + 'a>>;
}

/// A `HashMap`-backed schema loader, keyed by `object_hash`.
///
/// Intended for fixtures and unit tests: populate the map with
/// already-built [`Schema`] values (e.g. the result of
/// [`panproto_protocols::atproto::parse_lexicon`] on a lexicon json
/// document) and the loader hands them back on demand.
#[derive(Debug, Default, Clone)]
pub struct InMemorySchemaLoader {
    entries: HashMap<String, Schema>,
}

impl InMemorySchemaLoader {
    /// Construct an empty loader.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or replace) a schema under the given object-hash.
    pub fn insert(&mut self, object_hash: String, schema: Schema) {
        self.entries.insert(object_hash, schema);
    }

    /// Number of registered schemas.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether any schemas are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl SchemaLoader for InMemorySchemaLoader {
    fn load<'a>(
        &'a self,
        object_hash: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Schema, LensError>> + Send + 'a>>
    {
        Box::pin(async move {
            self.entries
                .get(object_hash)
                .cloned()
                .ok_or_else(|| LensError::NotFound(format!("schema:{object_hash}")))
        })
    }
}

/// Forward [`SchemaLoader`] through a shared `Arc<T>`.
impl<T: SchemaLoader + ?Sized> SchemaLoader for std::sync::Arc<T> {
    fn load<'a>(
        &'a self,
        object_hash: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Schema, LensError>> + Send + 'a>>
    {
        (**self).load(object_hash)
    }
}

/// Filesystem-backed [`SchemaLoader`].
///
/// Expects a directory whose entries are named `<object_hash>.json`,
/// each file an atproto Lexicon document (the same shape idiolect's
/// `lexicons/dev/...` tree uses). On `load`, the loader reads the
/// matching file, parses it through
/// [`panproto_protocols::web_document::atproto::parse_lexicon`], and
/// returns the resulting [`Schema`] graph.
///
/// The content-address check is the caller's responsibility: the
/// loader does not re-hash the file to confirm it matches its
/// filename. A downstream deployment that needs hash verification
/// wraps this loader.
#[derive(Debug, Clone)]
pub struct FilesystemSchemaLoader {
    dir: std::path::PathBuf,
}

impl FilesystemSchemaLoader {
    /// Construct a loader rooted at `dir`. The directory must exist
    /// and be readable.
    ///
    /// # Errors
    ///
    /// Returns [`LensError::Transport`] when `dir` does not exist or
    /// is not a directory.
    pub fn new<P: Into<std::path::PathBuf>>(dir: P) -> Result<Self, LensError> {
        let dir = dir.into();
        if !dir.is_dir() {
            return Err(LensError::Transport(format!(
                "{} is not a directory",
                dir.display()
            )));
        }
        Ok(Self { dir })
    }

    /// Borrow the backing directory path.
    #[must_use]
    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    fn path_for(&self, object_hash: &str) -> std::path::PathBuf {
        // Sanitize the same way the filesystem session store does:
        // `:` and `/` are not legal in common filename schemes.
        let mut filename = String::with_capacity(object_hash.len() + 5);
        for ch in object_hash.chars() {
            match ch {
                ':' | '/' | '\\' | '\0' => filename.push('_'),
                c => filename.push(c),
            }
        }
        filename.push_str(".json");
        self.dir.join(filename)
    }
}

impl SchemaLoader for FilesystemSchemaLoader {
    fn load<'a>(
        &'a self,
        object_hash: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Schema, LensError>> + Send + 'a>>
    {
        Box::pin(async move {
            let path = self.path_for(object_hash);
            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Err(LensError::NotFound(format!("schema:{object_hash}")));
                }
                Err(e) => {
                    return Err(LensError::Transport(format!(
                        "read {}: {e}",
                        path.display()
                    )));
                }
            };
            let value: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|e| LensError::Transport(format!("parse {}: {e}", path.display())))?;
            panproto_protocols::web_document::atproto::parse_lexicon(&value)
                .map_err(|e| LensError::Transport(format!("lexicon parse {object_hash}: {e}")))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn filesystem_loader_rejects_non_directory() {
        let err = FilesystemSchemaLoader::new("/does/not/exist").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("is not a directory"));
    }

    #[tokio::test]
    async fn filesystem_loader_round_trips_a_lexicon_file() {
        let dir = tempfile::tempdir().unwrap();
        // Minimal lexicon (same shape as idiolect's dev.idiolect.*).
        let lex = serde_json::json!({
            "lexicon": 1,
            "id": "dev.example.ping",
            "defs": {
                "main": {
                    "type": "record",
                    "key": "tid",
                    "record": {
                        "type": "object",
                        "required": ["occurredAt"],
                        "properties": {
                            "occurredAt": { "type": "string", "format": "datetime" }
                        }
                    }
                }
            }
        });
        std::fs::write(
            dir.path().join("sha256_deadbeef.json"),
            serde_json::to_vec(&lex).unwrap(),
        )
        .unwrap();

        let loader = FilesystemSchemaLoader::new(dir.path()).unwrap();
        // Object hash with a `:` is sanitized to `_` in the filename.
        let schema = loader.load("sha256:deadbeef").await.unwrap();
        assert!(!schema.vertices.is_empty());
    }

    #[tokio::test]
    async fn filesystem_loader_missing_file_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let loader = FilesystemSchemaLoader::new(dir.path()).unwrap();
        let err = loader.load("sha256:absent").await.unwrap_err();
        assert!(matches!(err, LensError::NotFound(_)));
    }

    #[test]
    fn schema_loader_trait_object_future_is_send() {
        fn assert_send<T: Send>(_: &T) {}
        let l: std::sync::Arc<dyn SchemaLoader> = std::sync::Arc::new(InMemorySchemaLoader::new());
        let fut = l.load("sha256:abc");
        assert_send(&fut);
    }

    #[tokio::test]
    async fn filesystem_loader_bad_json_surfaces_transport() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("sha256_corrupt.json"), b"{not json").unwrap();
        let loader = FilesystemSchemaLoader::new(dir.path()).unwrap();
        let err = loader.load("sha256:corrupt").await.unwrap_err();
        assert!(matches!(err, LensError::Transport(_)));
    }
}

// -----------------------------------------------------------------
// PDS-backed schema loader
// -----------------------------------------------------------------

/// PDS-backed [`SchemaLoader`].
///
/// The `dev.panproto.schema.schema` lexicon carries a serialised
/// [`panproto_schema::Schema`] in its `blob` field. This loader
/// reads the at-uri the lens runtime passes to
/// [`SchemaLoader::load`], fetches the record body via any
/// [`PdsClient`](crate::resolver::PdsClient), and pulls the
/// `blob` out.
///
/// The runtime always calls `load` with an at-uri (the value of
/// the lens record's `source_schema` or `target_schema` field), so
/// this loader is the natural pair for a [`PdsResolver`](crate::PdsResolver):
///
/// ```ignore
/// use idiolect_lens::{PdsResolver, PdsSchemaLoader, ReqwestPdsClient};
///
/// let client = ReqwestPdsClient::with_service_url("https://example.host.bsky.network");
/// let resolver = PdsResolver::new(client.clone());
/// let loader   = PdsSchemaLoader::new(client);
/// // resolver + loader both speak the PDS XRPC surface.
/// ```
///
/// Generic over `C: PdsClient` so tests can pair it with an
/// in-memory PDS mock.
#[derive(Debug, Clone)]
pub struct PdsSchemaLoader<C> {
    client: C,
}

impl<C> PdsSchemaLoader<C> {
    /// Wrap any [`PdsClient`](crate::resolver::PdsClient).
    #[must_use]
    pub const fn new(client: C) -> Self {
        Self { client }
    }

    /// Borrow the underlying client, e.g. to inspect it in a test.
    #[must_use]
    pub const fn client(&self) -> &C {
        &self.client
    }
}

impl<C: crate::resolver::PdsClient> SchemaLoader for PdsSchemaLoader<C> {
    fn load<'a>(
        &'a self,
        at_uri: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Schema, LensError>> + Send + 'a>>
    {
        Box::pin(async move {
            let (did, collection, rkey) = split_at_uri(at_uri)?;
            let body = self.client.get_record(did, collection, rkey).await?;
            let blob = body.get("blob").cloned().ok_or_else(|| {
                LensError::LexiconParse(format!("schema record at {at_uri} has no `blob` field"))
            })?;
            serde_json::from_value::<Schema>(blob).map_err(|e| {
                LensError::LexiconParse(format!("decode schema blob at {at_uri}: {e}"))
            })
        })
    }
}

/// Split an `at://<did>/<collection>/<rkey>` string into its three
/// parts. Returns a `LensError::Transport` on malformed input.
fn split_at_uri(at_uri: &str) -> Result<(&str, &str, &str), LensError> {
    let rest = at_uri
        .strip_prefix("at://")
        .ok_or_else(|| LensError::Transport(format!("not an at-uri: {at_uri}")))?;
    let mut parts = rest.splitn(3, '/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(d), Some(c), Some(r)) if !d.is_empty() && !c.is_empty() && !r.is_empty() => {
            Ok((d, c, r))
        }
        _ => Err(LensError::Transport(format!("malformed at-uri: {at_uri}"))),
    }
}

#[cfg(test)]
mod pds_loader_tests {
    use super::*;
    use crate::resolver::{ListRecordsResponse, PdsClient};

    struct StubPds {
        body: serde_json::Value,
    }

    impl PdsClient for StubPds {
        async fn get_record(
            &self,
            _did: &str,
            _collection: &str,
            _rkey: &str,
        ) -> Result<serde_json::Value, LensError> {
            Ok(self.body.clone())
        }

        async fn list_records(
            &self,
            _did: &str,
            _collection: &str,
            _limit: Option<u32>,
            _cursor: Option<&str>,
        ) -> Result<ListRecordsResponse, LensError> {
            Err(LensError::Transport("not implemented in stub".into()))
        }
    }

    /// Build a real `Schema` via the panproto `SchemaBuilder` and
    /// serialise it, so the loader's deserialize step exercises a
    /// shape the panproto runtime actually produces. Smallest
    /// non-trivial schema: a single "post" object vertex with a
    /// "string" child reached by a "text" edge.
    fn minimal_schema_blob() -> serde_json::Value {
        let proto = panproto_schema::Protocol::default();
        let schema = panproto_schema::SchemaBuilder::new(&proto)
            .entry("post")
            .vertex("post", "object", None)
            .unwrap()
            .vertex("post.text", "string", None)
            .unwrap()
            .edge("post", "post.text", "prop", Some("text"))
            .unwrap()
            .build()
            .unwrap();
        serde_json::to_value(&schema).expect("serialise Schema")
    }

    #[tokio::test]
    async fn pds_loader_extracts_blob_field() {
        let stub = StubPds {
            body: serde_json::json!({
                "blob":       minimal_schema_blob(),
                "objectHash": "sha256:deadbeef",
                "protocol":   "atproto",
                "createdAt":  "2026-04-19T00:00:00.000Z",
            }),
        };
        let loader = PdsSchemaLoader::new(stub);
        let schema = loader
            .load("at://did:plc:x/dev.panproto.schema.schema/abc")
            .await
            .unwrap();
        assert!(!schema.vertices.is_empty());
    }

    #[tokio::test]
    async fn pds_loader_rejects_record_with_no_blob() {
        let stub = StubPds {
            body: serde_json::json!({ "objectHash": "sha256:x", "protocol": "atproto" }),
        };
        let loader = PdsSchemaLoader::new(stub);
        let err = loader
            .load("at://did:plc:x/dev.panproto.schema.schema/abc")
            .await
            .unwrap_err();
        assert!(matches!(err, LensError::LexiconParse(_)));
    }

    #[tokio::test]
    async fn pds_loader_rejects_malformed_at_uri() {
        let stub = StubPds {
            body: serde_json::json!({}),
        };
        let loader = PdsSchemaLoader::new(stub);
        let err = loader.load("not-an-at-uri").await.unwrap_err();
        assert!(matches!(err, LensError::Transport(_)));
    }
}
