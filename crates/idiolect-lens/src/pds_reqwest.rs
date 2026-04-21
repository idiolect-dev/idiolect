//! Reqwest-backed [`PdsClient`] and [`PdsWriter`].
//!
//! A principled escape hatch from atrium — consumers who want raw
//! control over the HTTP pipeline, or who cannot depend on atrium's
//! version cadence, can enable `pds-reqwest` instead of `pds-atrium`.
//!
//! The implementation speaks the three XRPC endpoints the lens runtime
//! needs directly:
//!
//! - `GET  /xrpc/com.atproto.repo.getRecord?repo&collection&rkey`
//! - `POST /xrpc/com.atproto.repo.createRecord` (json body)
//! - `POST /xrpc/com.atproto.repo.putRecord`    (json body)
//! - `POST /xrpc/com.atproto.repo.deleteRecord` (json body)
//!
//! Authentication is injected by passing a pre-configured
//! [`reqwest::Client`] via [`ReqwestPdsClient::with_client`].
//! Callers that need authenticated writes wire up bearer-token
//! headers on the client; that concern is intentionally off the
//! lens-runtime path.
//!
//! Feature-gated under `pds-reqwest`.

use serde_json::json;

use crate::error::LensError;
use crate::resolver::{
    CreateRecordRequest, DeleteRecordRequest, PdsClient, PdsWriter, PutRecordRequest,
    WriteRecordResponse,
};

/// Reqwest-backed PDS client.
///
/// Holds a base URL (`https://pds.example`) and a [`reqwest::Client`].
/// Clone is cheap: `reqwest::Client` is `Arc`-backed internally.
#[derive(Debug, Clone)]
pub struct ReqwestPdsClient {
    /// Base URL of the PDS service, e.g. `https://bsky.social`.
    /// Must NOT include a trailing slash or the `/xrpc` suffix;
    /// both are appended internally.
    base: String,
    /// The actual HTTP client. Swapping in a pre-configured client
    /// with auth headers is how callers authenticate writes.
    http: reqwest::Client,
}

impl ReqwestPdsClient {
    /// Construct a client pointed at `base_url` using a default
    /// reqwest client (rustls, no auth, no custom headers).
    #[must_use]
    pub fn with_service_url(base_url: impl Into<String>) -> Self {
        Self::with_client(base_url, reqwest::Client::new())
    }

    /// Construct a client pointed at `base_url` using a caller-supplied
    /// [`reqwest::Client`]. Attach bearer-token default headers to the
    /// `http` client before calling this to authenticate writes.
    #[must_use]
    pub fn with_client(base_url: impl Into<String>, http: reqwest::Client) -> Self {
        let mut base: String = base_url.into();
        // tolerate a single trailing slash; internal code assumes none.
        if base.ends_with('/') {
            base.pop();
        }
        Self { base, http }
    }

    /// Borrow the underlying reqwest client.
    #[must_use]
    pub const fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// Borrow the configured base URL.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base
    }

    fn xrpc(&self, method: &str) -> String {
        format!("{}/xrpc/{method}", self.base)
    }
}

impl PdsClient for ReqwestPdsClient {
    async fn get_record(
        &self,
        did: &str,
        collection: &str,
        rkey: &str,
    ) -> Result<serde_json::Value, LensError> {
        let url = self.xrpc("com.atproto.repo.getRecord");
        let resp = self
            .http
            .get(&url)
            .query(&[("repo", did), ("collection", collection), ("rkey", rkey)])
            .send()
            .await
            .map_err(|e| LensError::Transport(format!("getRecord transport: {e}")))?;
        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LensError::Transport(format!("getRecord body: {e}")))?;
        if !status.is_success() {
            return Err(xrpc_error(status, &body));
        }
        // The XRPC envelope is { uri, cid, value }. The lens runtime
        // wants just the record body.
        match body.get("value").cloned() {
            Some(value) => Ok(value),
            None => Err(LensError::Transport(format!(
                "getRecord response missing `value` field: {body}"
            ))),
        }
    }

    async fn list_records(
        &self,
        did: &str,
        collection: &str,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> Result<crate::resolver::ListRecordsResponse, LensError> {
        let url = self.xrpc("com.atproto.repo.listRecords");
        let mut query: Vec<(&str, String)> = vec![
            ("repo", did.to_owned()),
            ("collection", collection.to_owned()),
        ];
        if let Some(l) = limit {
            query.push(("limit", l.to_string()));
        }
        if let Some(c) = cursor {
            query.push(("cursor", c.to_owned()));
        }
        let resp = self
            .http
            .get(&url)
            .query(&query)
            .send()
            .await
            .map_err(|e| LensError::Transport(format!("listRecords transport: {e}")))?;
        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LensError::Transport(format!("listRecords body: {e}")))?;
        if !status.is_success() {
            return Err(xrpc_error(status, &body));
        }
        serde_json::from_value(body)
            .map_err(|e| LensError::Transport(format!("listRecords decode: {e}")))
    }
}

impl PdsWriter for ReqwestPdsClient {
    async fn create_record(
        &self,
        request: CreateRecordRequest,
    ) -> Result<WriteRecordResponse, LensError> {
        let mut body = json!({
            "repo": request.repo,
            "collection": request.collection,
            "record": request.record,
        });
        if let Some(rkey) = request.rkey {
            body["rkey"] = serde_json::Value::String(rkey);
        }
        if let Some(v) = request.validate {
            body["validate"] = serde_json::Value::Bool(v);
        }

        let resp = self
            .http
            .post(self.xrpc("com.atproto.repo.createRecord"))
            .json(&body)
            .send()
            .await
            .map_err(|e| LensError::Transport(format!("createRecord transport: {e}")))?;
        parse_write_response(resp).await
    }

    async fn put_record(
        &self,
        request: PutRecordRequest,
    ) -> Result<WriteRecordResponse, LensError> {
        let mut body = json!({
            "repo": request.repo,
            "collection": request.collection,
            "rkey": request.rkey,
            "record": request.record,
        });
        if let Some(v) = request.validate {
            body["validate"] = serde_json::Value::Bool(v);
        }
        if let Some(swap) = request.swap_record {
            body["swapRecord"] = serde_json::Value::String(swap);
        }

        let resp = self
            .http
            .post(self.xrpc("com.atproto.repo.putRecord"))
            .json(&body)
            .send()
            .await
            .map_err(|e| LensError::Transport(format!("putRecord transport: {e}")))?;
        parse_write_response(resp).await
    }

    async fn delete_record(&self, request: DeleteRecordRequest) -> Result<(), LensError> {
        let mut body = json!({
            "repo": request.repo,
            "collection": request.collection,
            "rkey": request.rkey,
        });
        if let Some(swap) = request.swap_record {
            body["swapRecord"] = serde_json::Value::String(swap);
        }

        let resp = self
            .http
            .post(self.xrpc("com.atproto.repo.deleteRecord"))
            .json(&body)
            .send()
            .await
            .map_err(|e| LensError::Transport(format!("deleteRecord transport: {e}")))?;
        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        // Grab the error body for diagnostics.
        let detail: serde_json::Value = resp.json().await.unwrap_or_else(|_| json!({}));
        Err(xrpc_error(status, &detail))
    }
}

async fn parse_write_response(
    resp: reqwest::Response,
) -> Result<WriteRecordResponse, LensError> {
    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| LensError::Transport(format!("write response body: {e}")))?;
    if !status.is_success() {
        return Err(xrpc_error(status, &body));
    }
    let uri = body
        .get("uri")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .ok_or_else(|| LensError::Transport(format!("response missing `uri`: {body}")))?;
    let cid = body
        .get("cid")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .ok_or_else(|| LensError::Transport(format!("response missing `cid`: {body}")))?;
    Ok(WriteRecordResponse { uri, cid })
}

/// Map a non-2xx response to the right [`LensError`] variant.
///
/// XRPC error responses look like `{ "error": "RecordNotFound", "message": "..." }`.
/// `RecordNotFound` (case-insensitive) maps to [`LensError::NotFound`];
/// everything else collapses to [`LensError::Transport`] with the
/// status and message preserved for operators.
fn xrpc_error(status: reqwest::StatusCode, body: &serde_json::Value) -> LensError {
    let kind = body.get("error").and_then(|v| v.as_str()).unwrap_or("");
    let message = body.get("message").and_then(|v| v.as_str()).unwrap_or("");
    let detail = format!("{status} {kind}: {message}");
    if kind.eq_ignore_ascii_case("RecordNotFound") {
        LensError::NotFound(detail)
    } else {
        LensError::Transport(detail)
    }
}
