//! Community infrastructure for Idiolect.
//!
//! This crate makes a governed community change the unit of work. A workspace
//! manifest names the community, packages, authorities, governance rules,
//! resource limits, federation dependencies, and exit policy. Change packets
//! combine a human proposal with Panproto compatibility evidence, reviews,
//! verification outcomes, and migration requirements. Approved packets can be
//! assembled into signed release bundles and portable exports.

mod analyze;
mod error;
mod export;
mod governance;
mod migration;
mod model;
mod release;
mod workspace;

pub use analyze::analyze_schema_change;
pub use error::{CommunityError, CommunityResult};
pub use export::{ExportInventory, export_workspace};
pub use governance::{evaluate_governance, record_review, record_verification, verification_gate};
pub use migration::{advance_migration, create_migration_run};
pub use model::*;
pub use release::{
    CommunitySigningKey, artifact, create_release, generate_signing_key, sign_release,
    verify_release,
};
pub use workspace::{
    DEFAULT_MANIFEST_NAME, check_workspace, doctor_workspace, init_workspace, load_manifest,
    load_packet, load_release, repair_workspace, save_json, save_manifest, save_packet,
};
