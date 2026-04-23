//! Atrium-backed [`PdsClient`] implementation.
//!
//! Wraps [`atrium_api::client::AtpServiceClient`] so the lens runtime
//! can fetch `dev.panproto.schema.lens` records from a live personal
//! data server over XRPC. Gated behind the `pds-atrium` cargo feature
//! so the core crate stays transport-agnostic.
//!
//! # Construction
//!
//! The simplest path uses reqwest as the HTTP transport:
//!
//! ```ignore
//! use idiolect_lens::{PdsResolver, pds_atrium::AtriumPdsClient};
//!
//! let client = AtriumPdsClient::with_service_url("https://bsky.social");
//! let resolver = PdsResolver::new(client);
//! ```
//!
//! For custom transports (isahc, a test double, an authenticated
//! client built elsewhere), hand in a ready-made
//! [`atrium_api::client::AtpServiceClient`] via
//! [`AtriumPdsClient::from_service_client`].
//!
//! # Error mapping
//!
//! Atrium returns [`atrium_xrpc::Error`]; this module translates it
//! into [`LensError`] per the contract documented on [`PdsClient`]:
//!
//! - XRPC responses carrying the lexicon-defined `RecordNotFound`
//!   variant collapse to [`LensError::NotFound`].
//! - Every other atrium error (transport, http, decode) becomes
//!   [`LensError::Transport`].

use std::sync::Arc;

use atrium_api::client::AtpServiceClient;
use atrium_api::com::atproto::repo::{
    create_record, delete_record, get_record, list_records, put_record,
};
use atrium_api::types::{TryIntoUnknown, string::Cid};
use atrium_xrpc::XrpcClient;
use atrium_xrpc_client::reqwest::ReqwestClient;

use crate::error::LensError;
use crate::resolver::{
    CreateRecordRequest, DeleteRecordRequest, PdsClient, PdsWriter, PutRecordRequest,
    WriteRecordResponse,
};

/// Live PDS client backed by atrium's XRPC stack.
///
/// Generic over any [`atrium_xrpc::XrpcClient`]; concrete constructors
/// are provided for the reqwest transport via
/// [`AtriumPdsClient::with_service_url`]. Clone cost is one `Arc`
/// bump — the inner service client is already shareable.
pub struct AtriumPdsClient<T>
where
    T: XrpcClient + Send + Sync,
{
    /// Atrium's full service client. We only ever call
    /// `service.com.atproto.repo.get_record`, but storing the whole
    /// client keeps the door open for future lexicons without an api
    /// break.
    client: Arc<AtpServiceClient<T>>,
}

impl<T> Clone for AtriumPdsClient<T>
where
    T: XrpcClient + Send + Sync,
{
    fn clone(&self) -> Self {
        Self {
            client: Arc::clone(&self.client),
        }
    }
}

impl AtriumPdsClient<ReqwestClient> {
    /// Construct a client that talks to the PDS at `service_url`
    /// using reqwest as the http transport.
    ///
    /// Accepts anything that looks like a URL; atrium only requires
    /// `Display` at construction time. Authentication is not wired
    /// up — `getRecord` is an unauthenticated endpoint on public
    /// records, and authenticated flows belong on a higher layer
    /// that hands [`AtriumPdsClient::from_service_client`] a
    /// pre-configured atrium client.
    #[must_use]
    pub fn with_service_url(service_url: impl AsRef<str>) -> Self {
        let xrpc = ReqwestClient::new(service_url);
        Self::from_service_client(AtpServiceClient::new(xrpc))
    }
}

impl<T> AtriumPdsClient<T>
where
    T: XrpcClient + Send + Sync,
{
    /// Wrap a ready-made [`AtpServiceClient`].
    ///
    /// Use this when you need a custom xrpc transport (isahc, a
    /// wiremock-backed test double) or a pre-authenticated session.
    #[must_use]
    pub fn from_service_client(client: AtpServiceClient<T>) -> Self {
        Self {
            client: Arc::new(client),
        }
    }
}

impl<T> PdsClient for AtriumPdsClient<T>
where
    T: XrpcClient + Send + Sync,
{
    async fn get_record(
        &self,
        did: &str,
        collection: &str,
        rkey: &str,
    ) -> Result<serde_json::Value, LensError> {
        let params = get_record::ParametersData {
            cid: None,
            collection: collection.parse().map_err(|e: &'static str| {
                LensError::Transport(format!("invalid collection: {e}"))
            })?,
            repo: did
                .parse()
                .map_err(|e: &'static str| LensError::Transport(format!("invalid repo: {e}")))?,
            rkey: rkey
                .parse()
                .map_err(|e: &'static str| LensError::Transport(format!("invalid rkey: {e}")))?,
        };

        let output = self
            .client
            .service
            .com
            .atproto
            .repo
            .get_record(params.into())
            .await
            .map_err(map_atrium_error)?;

        // `output.value` is an `atrium_api::types::Unknown`; its
        // Serialize impl produces the same JSON the PDS sent on the
        // wire, so the downstream `serde_json::from_value::<PanprotoLens>`
        // step in `PdsResolver::resolve` sees exactly the record body.
        serde_json::to_value(&output.value).map_err(LensError::from)
    }

    async fn list_records(
        &self,
        did: &str,
        collection: &str,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> Result<crate::resolver::ListRecordsResponse, LensError> {
        let params = list_records::ParametersData {
            collection: collection.parse().map_err(|e: &'static str| {
                LensError::Transport(format!("invalid collection: {e}"))
            })?,
            repo: did
                .parse()
                .map_err(|e: &'static str| LensError::Transport(format!("invalid repo: {e}")))?,
            // atrium's `limit` is `LimitedNonZeroU8<100>` (1..=100).
            // Clamp and convert; None when the caller omitted it.
            limit: limit.and_then(|l| {
                let clamped = l.clamp(1, 100);
                #[allow(clippy::cast_possible_truncation)]
                let as_u8 = clamped as u8;
                as_u8.try_into().ok()
            }),
            cursor: cursor.map(str::to_owned),
            reverse: None,
        };
        let output = self
            .client
            .service
            .com
            .atproto
            .repo
            .list_records(params.into())
            .await
            .map_err(|e| map_list_error(&e))?;
        // atrium returns typed records; re-serialize to the
        // generic `{uri, cid, value}` shape our abstraction expects.
        let raw = serde_json::to_value(&output.records)
            .map_err(|e| LensError::Transport(format!("list_records serialize: {e}")))?;
        let records = serde_json::from_value::<Vec<crate::resolver::ListedRecord>>(raw)
            .map_err(|e| LensError::Transport(format!("list_records shape: {e}")))?;
        Ok(crate::resolver::ListRecordsResponse {
            records,
            cursor: output.cursor.clone(),
        })
    }
}

fn map_list_error(err: &atrium_xrpc::Error<list_records::Error>) -> LensError {
    // listRecords does not define a RecordNotFound variant; every
    // failure becomes Transport carrying the atrium message.
    LensError::Transport(err.to_string())
}

/// Translate an [`atrium_xrpc::Error`] into a [`LensError`].
///
/// The `RecordNotFound` lexicon-defined error variant becomes
/// [`LensError::NotFound`]; every other failure (transport,
/// encoding, unexpected response shape) becomes
/// [`LensError::Transport`] carrying the atrium message verbatim.
fn map_atrium_error(err: atrium_xrpc::Error<get_record::Error>) -> LensError {
    // stringify first — the error carries the server-provided message
    // we want to surface whether this becomes NotFound or Transport.
    let message = err.to_string();
    match err {
        atrium_xrpc::Error::XrpcResponse(resp)
            if matches!(
                resp.error,
                Some(atrium_xrpc::error::XrpcErrorKind::Custom(
                    get_record::Error::RecordNotFound(_),
                ))
            ) =>
        {
            LensError::NotFound(message)
        }
        _ => LensError::Transport(message),
    }
}

// -----------------------------------------------------------------
// write path
// -----------------------------------------------------------------

impl<T> PdsWriter for AtriumPdsClient<T>
where
    T: XrpcClient + Send + Sync,
{
    async fn create_record(
        &self,
        request: CreateRecordRequest,
    ) -> Result<WriteRecordResponse, LensError> {
        // atrium wants a typed `Unknown` for the record body. the
        // easiest path from a `serde_json::Value` is to round-trip
        // through `TryIntoUnknown`, which serializes → ipld → Unknown.
        let record_unknown = request.record.clone().try_into_unknown().map_err(|e| {
            LensError::Transport(format!(
                "record body not convertible to atrium Unknown: {e}"
            ))
        })?;

        let rkey = request
            .rkey
            .map(|k| {
                k.parse()
                    .map_err(|e: &'static str| LensError::Transport(format!("invalid rkey: {e}")))
            })
            .transpose()?;

        let input = create_record::InputData {
            collection: request.collection.parse().map_err(|e: &'static str| {
                LensError::Transport(format!("invalid collection: {e}"))
            })?,
            record: record_unknown,
            repo: request
                .repo
                .parse()
                .map_err(|e: &'static str| LensError::Transport(format!("invalid repo: {e}")))?,
            rkey,
            swap_commit: None,
            validate: request.validate,
        };

        let output = self
            .client
            .service
            .com
            .atproto
            .repo
            .create_record(input.into())
            .await
            .map_err(|e| map_create_record_error(&e))?;

        Ok(WriteRecordResponse {
            uri: output.uri.clone(),
            cid: cid_to_string(&output.cid),
        })
    }

    async fn put_record(
        &self,
        request: PutRecordRequest,
    ) -> Result<WriteRecordResponse, LensError> {
        let record_unknown = request.record.clone().try_into_unknown().map_err(|e| {
            LensError::Transport(format!(
                "record body not convertible to atrium Unknown: {e}"
            ))
        })?;

        let swap_record = request
            .swap_record
            .map(|c| {
                c.parse::<Cid>()
                    .map_err(|e| LensError::Transport(format!("invalid swap-record cid: {e}")))
            })
            .transpose()?;

        let input =
            put_record::InputData {
                collection: request.collection.parse().map_err(|e: &'static str| {
                    LensError::Transport(format!("invalid collection: {e}"))
                })?,
                record: record_unknown,
                repo: request.repo.parse().map_err(|e: &'static str| {
                    LensError::Transport(format!("invalid repo: {e}"))
                })?,
                rkey: request.rkey.parse().map_err(|e: &'static str| {
                    LensError::Transport(format!("invalid rkey: {e}"))
                })?,
                swap_commit: None,
                swap_record,
                validate: request.validate,
            };

        let output = self
            .client
            .service
            .com
            .atproto
            .repo
            .put_record(input.into())
            .await
            .map_err(|e| map_put_record_error(&e))?;

        Ok(WriteRecordResponse {
            uri: output.uri.clone(),
            cid: cid_to_string(&output.cid),
        })
    }

    async fn delete_record(&self, request: DeleteRecordRequest) -> Result<(), LensError> {
        let swap_record = request
            .swap_record
            .map(|c| {
                c.parse::<Cid>()
                    .map_err(|e| LensError::Transport(format!("invalid swap-record cid: {e}")))
            })
            .transpose()?;

        let input =
            delete_record::InputData {
                collection: request.collection.parse().map_err(|e: &'static str| {
                    LensError::Transport(format!("invalid collection: {e}"))
                })?,
                repo: request.repo.parse().map_err(|e: &'static str| {
                    LensError::Transport(format!("invalid repo: {e}"))
                })?,
                rkey: request.rkey.parse().map_err(|e: &'static str| {
                    LensError::Transport(format!("invalid rkey: {e}"))
                })?,
                swap_commit: None,
                swap_record,
            };

        self.client
            .service
            .com
            .atproto
            .repo
            .delete_record(input.into())
            .await
            .map_err(|e| map_delete_record_error(&e))?;

        Ok(())
    }
}

/// Render an atrium `Cid` back into the standard string form the PDS
/// echoes on the wire. `Cid::as_ref()` hands us the inner
/// `cid::Cid`, whose `Display` impl produces the canonical base32 form.
fn cid_to_string(cid: &Cid) -> String {
    cid.as_ref().to_string()
}

/// Translate an atrium error from a `createRecord` call into a
/// [`LensError`]. Create has no `NotFound`-shaped variant — the only
/// typed error is `InvalidSwap` — so everything collapses to
/// [`LensError::Transport`] with the message verbatim.
fn map_create_record_error(err: &atrium_xrpc::Error<create_record::Error>) -> LensError {
    LensError::Transport(err.to_string())
}

/// Translate an atrium error from a `putRecord` call. Same shape as
/// [`map_create_record_error`].
fn map_put_record_error(err: &atrium_xrpc::Error<put_record::Error>) -> LensError {
    LensError::Transport(err.to_string())
}

/// Translate an atrium error from a `deleteRecord` call. Same shape as
/// [`map_create_record_error`].
fn map_delete_record_error(err: &atrium_xrpc::Error<delete_record::Error>) -> LensError {
    LensError::Transport(err.to_string())
}
