//! Schema-diff and lens-based record migration.
//!
//! Given two versions of a lexicon, `idiolect-migrate` classifies the
//! structural diff through `panproto_check::classify` and — for diffs
//! covered by shipped migration recipes — produces a [`MigrationPlan`]
//! carrying the source and target schema hashes plus a lens body the
//! caller can publish as a `dev.panproto.schema.lens` record. Records
//! that already existed under the old schema travel through
//! `idiolect_lens::apply_lens` against the published lens to reach
//! the new schema.
//!
//! # Scope
//!
//! - **Non-breaking diffs** (added optional fields, added vertices,
//!   added edges): the caller does not need a migration — records
//!   valid under the old schema remain valid under the new.
//!   [`classify`] returns `compatible = true`; no plan is needed.
//! - **Auto-derivable breaking diffs** (removed optional field,
//!   renamed vertex via a hint): [`plan_auto`] returns a
//!   [`MigrationPlan`] with a protolens-chain body the caller can
//!   publish.
//! - **Non-auto breaking diffs** (removed required field, changed
//!   required-field type, added required field): [`plan_auto`]
//!   returns `Err(PlannerError::NotAutoDerivable)` listing which
//!   breaking changes resist automation. The caller writes the lens
//!   by hand.
//!
//! # Record migration
//!
//! [`migrate_record`] wraps `idiolect_lens::apply_lens` for the
//! one-shot case: given a lens record, a source record body, and a
//! [`idiolect_lens::SchemaLoader`] that can resolve both schema
//! hashes, it returns the migrated target body.
//!
//! # Relationship to the workspace
//!
//! This crate owns no new runtime state. It is a thin typed façade
//! over `panproto-check` (for diff classification) and
//! `idiolect-lens` (for record translation). The runtime cost of a
//! migration equals the cost of one `apply_lens` per record; no
//! per-crate caches live here.

pub mod error;
pub mod plan;
pub mod record;

pub use error::{MigrateError, MigrateResult, PlannerError};
pub use panproto_check::{CompatReport, SchemaDiff};
pub use plan::{MigrationPlan, classify, plan_auto};
pub use record::migrate_record;
