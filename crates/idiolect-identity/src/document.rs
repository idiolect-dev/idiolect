//! atproto-flavoured DID documents.
//!
//! The full W3C DID Core document is a superset of what atproto
//! consumers care about. This module models the small subset idiolect
//! uses:
//!
//! - `id` — echoed back (should match the resolved DID).
//! - `alsoKnownAs` — the at-identifier handle, e.g. `at://alice.test`.
//!   Zero or many entries.
//! - `service` — typed endpoints; idiolect only reads the
//!   `#atproto_pds` service, which carries the repo's PDS base URL.
//! - `verificationMethod` — public keys; kept as raw json for
//!   pass-through since signature verification lives outside this
//!   crate.
//!
//! Unknown fields in the document are preserved verbatim under
//! [`DidDocument::extras`] so callers that need them are not forced
//! through a second parse.

use serde::{Deserialize, Serialize};

/// Parsed DID document with atproto-relevant fields lifted out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DidDocument {
    /// The DID the document describes. MUST match the DID used to fetch.
    pub id: String,

    /// All handles the identity claims. Entries are at-uri shaped
    /// strings (`at://alice.test`). Empty list is valid.
    #[serde(default, rename = "alsoKnownAs", skip_serializing_if = "Vec::is_empty")]
    pub also_known_as: Vec<String>,

    /// Service endpoints declared by the identity, in document order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub service: Vec<Service>,

    /// Verification methods (public keys). Retained as raw json so
    /// downstream consumers that do signature verification can parse
    /// them with a richer type without a second fetch.
    #[serde(default, rename = "verificationMethod", skip_serializing_if = "Vec::is_empty")]
    pub verification_method: Vec<serde_json::Value>,

    /// Everything else in the document. Preserved verbatim so future
    /// fields the spec adds do not require a parser change here.
    #[serde(flatten)]
    pub extras: serde_json::Map<String, serde_json::Value>,
}

/// One service endpoint from a DID document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Service {
    /// Service id within the document (`#atproto_pds`, etc.).
    pub id: String,
    /// Service type (`AtprotoPersonalDataServer`, etc.).
    #[serde(rename = "type")]
    pub service_type: String,
    /// URL or URI the service is reachable at.
    #[serde(rename = "serviceEndpoint")]
    pub service_endpoint: String,
}

impl DidDocument {
    /// The canonical atproto handle for this identity, if any.
    ///
    /// The handle is the first `alsoKnownAs` entry starting with
    /// `at://`, with the `at://` prefix stripped. Identities may
    /// declare multiple handles; first-wins matches the atproto
    /// convention that the "primary" handle is listed first.
    #[must_use]
    pub fn handle(&self) -> Option<&str> {
        self.also_known_as
            .iter()
            .find_map(|s| s.strip_prefix("at://"))
    }

    /// Base URL of the atproto PDS, read from the `#atproto_pds`
    /// service entry.
    ///
    /// Returns `None` if the document has no atproto PDS service,
    /// which means the identity has no reachable repo (rare in
    /// practice but allowed by the spec).
    #[must_use]
    pub fn pds_url(&self) -> Option<&str> {
        self.service
            .iter()
            .find(|s| s.id == "#atproto_pds" || s.service_type == "AtprotoPersonalDataServer")
            .map(|s| s.service_endpoint.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_doc() -> DidDocument {
        serde_json::from_value(serde_json::json!({
            "id": "did:plc:alice",
            "alsoKnownAs": ["at://alice.test", "at://alice.bsky.social"],
            "verificationMethod": [
                { "id": "did:plc:alice#atproto", "type": "Multikey",
                  "controller": "did:plc:alice", "publicKeyMultibase": "z6M..." }
            ],
            "service": [
                {
                    "id": "#atproto_pds",
                    "type": "AtprotoPersonalDataServer",
                    "serviceEndpoint": "https://pds.example.com"
                }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn handle_strips_at_prefix() {
        let doc = fixture_doc();
        assert_eq!(doc.handle(), Some("alice.test"));
    }

    #[test]
    fn handle_none_when_no_also_known_as() {
        let mut doc = fixture_doc();
        doc.also_known_as.clear();
        assert!(doc.handle().is_none());
    }

    #[test]
    fn pds_url_reads_atproto_service() {
        let doc = fixture_doc();
        assert_eq!(doc.pds_url(), Some("https://pds.example.com"));
    }

    #[test]
    fn pds_url_none_when_service_missing() {
        let mut doc = fixture_doc();
        doc.service.clear();
        assert!(doc.pds_url().is_none());
    }

    #[test]
    fn extras_are_preserved() {
        let doc: DidDocument = serde_json::from_value(serde_json::json!({
            "id": "did:plc:x",
            "context": ["https://www.w3.org/ns/did/v1"]
        }))
        .unwrap();
        assert!(doc.extras.contains_key("context"));
    }

    #[test]
    fn roundtrip_preserves_fields() {
        let doc = fixture_doc();
        let rt: DidDocument = serde_json::from_value(serde_json::to_value(&doc).unwrap()).unwrap();
        assert_eq!(rt, doc);
    }
}
