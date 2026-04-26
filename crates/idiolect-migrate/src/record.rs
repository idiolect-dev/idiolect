//! One-shot record migration via `idiolect_lens::apply_lens`.

use idiolect_lens::{ApplyLensInput, Resolver, SchemaLoader, apply_lens};
use panproto_schema::Protocol;

use crate::error::{MigrateError, MigrateResult};

/// Run a lens migration on a single record body.
///
/// `lens_uri` resolves through `resolver` to a `PanprotoLens` record;
/// the record's source and target schema hashes are looked up in
/// `schema_loader`, and `source_record` is translated through the
/// lens's `get` direction.
///
/// # Errors
///
/// Any [`MigrateError`]. Most failures come from the lens runtime —
/// unresolvable lens, missing schema, decode failure — and surface
/// via [`MigrateError::Lens`].
pub async fn migrate_record<R, L>(
    resolver: &R,
    schema_loader: &L,
    protocol: &Protocol,
    lens_uri: &str,
    source_record: serde_json::Value,
) -> MigrateResult<serde_json::Value>
where
    R: Resolver,
    L: SchemaLoader,
{
    let lens_uri = idiolect_records::AtUri::parse(lens_uri)
        .map_err(|e| MigrateError::InvalidInput(format!("lens-uri: {e}")))?;
    let output = apply_lens(
        resolver,
        schema_loader,
        protocol,
        ApplyLensInput {
            lens_uri,
            source_record,
            source_root_vertex: None,
        },
    )
    .await
    .map_err(MigrateError::from)?;
    Ok(output.target_record)
}
