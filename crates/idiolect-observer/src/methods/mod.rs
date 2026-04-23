//! Reference [`ObservationMethod`](crate::method::ObservationMethod)
//! implementations shipped with the observer crate.
//!
//! The reference methods are deliberately narrow and composable —
//! nothing in the observer pipeline is hard-coded against a specific
//! method, so downstream crates define their own by implementing the
//! trait directly.
//!
//! - [`correction_rate::CorrectionRateMethod`] — per-lens correction
//!   counts grouped by reason. Signal: translation-quality rumor.
//! - [`encounter_throughput::EncounterThroughputMethod`] — encounter
//!   traffic by kind and downstream result. Signal: firehose liveness.
//! - [`verification_coverage::VerificationCoverageMethod`] — per-lens
//!   verification counts by kind, result, and distinct verifiers.
//!   Signal: formal-channel evidence.
//! - [`lens_adoption::LensAdoptionMethod`] — per-lens encounter count
//!   and distinct-invoker DIDs. Signal: adoption breadth.
//! - [`dialect_federation::DialectFederationMethod`] — watched
//!   communities' current dialect + lens-set delta since the
//!   previous snapshot. Signal: federation surface change.
//! - [`action_distribution::ActionDistributionMethod`] — encounter
//!   counts grouped by structured `use.action` with optional
//!   vocabulary-rooted rollup. Signal: which actions communities
//!   are translating for.
//! - [`purpose_distribution::PurposeDistributionMethod`] — parallel
//!   aggregator over `use.purpose`. Signal: for what ends.
//! - [`basis_distribution::BasisDistributionMethod`] — record counts
//!   bucketed by basis variant. Signal: first-party vs third-party
//!   attribution rates.
//! - [`attribution_chains::AttributionChainsMethod`] — counts of
//!   belief records by holder + subject. Signal: labeler coverage
//!   over the firehose.

pub mod action_distribution;
pub mod attribution_chains;
pub mod basis_distribution;
pub mod correction_rate;
pub mod dialect_federation;
pub mod encounter_throughput;
pub mod lens_adoption;
pub mod purpose_distribution;
pub mod verification_coverage;

pub use action_distribution::ActionDistributionMethod;
pub use attribution_chains::AttributionChainsMethod;
pub use basis_distribution::BasisDistributionMethod;
pub use correction_rate::CorrectionRateMethod;
pub use dialect_federation::DialectFederationMethod;
pub use encounter_throughput::EncounterThroughputMethod;
pub use lens_adoption::LensAdoptionMethod;
pub use purpose_distribution::PurposeDistributionMethod;
pub use verification_coverage::VerificationCoverageMethod;
