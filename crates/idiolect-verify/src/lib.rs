//! Verification runners for `dev.idiolect.verification` records.
//!
//! The `Verification` lexicon defines a fixed taxonomy of property
//! kinds (`roundtrip-test`, `property-test`, `formal-proof`,
//! `conformance-test`, `static-check`, `convergence-preserving`,
//! `coercion-law`) and an `observer`-published record per assertion.
//! This crate runs the property a verification claims, so the
//! orchestrator's `sufficient_verifications_for` query is grounded
//! in checked results rather than self-reported "holds" assertions.
//!
//! A [`VerificationRunner`] takes a lens record and the
//! [`PanprotoLens`] body's already-compiled runtime artifacts,
//! actually checks the property it claims to verify, and returns a
//! [`Verification`] record the caller can publish. The result field
//! is `Holds`, `Falsified`, or `Inconclusive`, following the lexicon
//! taxonomy.
//!
//! The runners shipped here cover the kinds idiolect callers
//! currently ask for:
//!
//! - [`RoundtripTestRunner`] — applies the lens forward then backward
//!   on a caller-supplied corpus of source records and checks that
//!   `put(get(src)) == src`. Matches the `roundtrip-test` taxonomy
//!   entry.
//! - [`PropertyTestRunner`] — same shape as roundtrip-test but the
//!   corpus comes from a caller-supplied generator closure rather
//!   than a static `Vec`. Matches the `property-test` entry.
//! - [`StaticCheckRunner`] — runs panproto's schema validator on the
//!   lens's source and target schemas, ensuring both parse cleanly
//!   under the declared protocol. Matches the `static-check` entry.
//! - [`CoercionLawRunner`] — dispatches the lens to panproto's
//!   `dev.panproto.translate.verifyCoercionLaws` xrpc and falsifies
//!   the verification on any returned `coercionLawViolation`. Matches
//!   the `coercion-law` entry.
//!
//! Remaining kinds (`formal-proof` via Coq/Lean artifact checking,
//! `convergence-preserving` via a round-trip-with-edits pipeline) are
//! straightforward to add under the same trait once a caller asks.

pub mod coercion_law;
pub mod error;
/// Generated from `verify-spec/runners.json` by `idiolect-codegen`.
/// Do not edit by hand; regenerate via
/// `cargo run -p idiolect-codegen -- generate`.
pub mod generated;
pub mod property_test;
pub mod roundtrip;
pub mod runner;
pub mod static_check;

pub use coercion_law::{CoercionLawClient, CoercionLawRunner, CoercionLawViolation};
pub use error::{VerifyError, VerifyResult};
pub use property_test::PropertyTestRunner;
pub use roundtrip::RoundtripTestRunner;
pub use runner::{VerificationRunner, VerificationTarget};
pub use static_check::StaticCheckRunner;
