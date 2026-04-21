//! Verification runners for `dev.idiolect.verification` records.
//!
//! The `Verification` lexicon defines a fixed taxonomy of property
//! kinds (`roundtrip-test`, `property-test`, `formal-proof`,
//! `conformance-test`, `static-check`, `convergence-preserving`) and
//! an `observer`-published record per assertion. Before this crate,
//! verifications were *counted* by the observer but never *run*:
//! anyone could publish a "holds" verification for any lens, and the
//! orchestrator's `sufficient_verifications_for` query was strictly
//! gullible.
//!
//! A [`VerificationRunner`] takes a lens record and the
//! [`PanprotoLens`] body's already-compiled runtime artifacts,
//! actually checks the property it claims to verify, and returns a
//! [`Verification`] record the caller can publish. The result field
//! is `Holds`, `Falsified`, or `Inconclusive`, following the lexicon
//! taxonomy.
//!
//! Two runners ship today:
//!
//! - [`RoundtripTestRunner`] — applies the lens forward then backward
//!   on a caller-supplied corpus of source records and checks that
//!   `put(get(src)) == src`. Matches the `roundtrip-test` taxonomy
//!   entry.
//! - [`StaticCheckRunner`] — runs panproto's schema validator on the
//!   lens's source and target schemas, ensuring both parse cleanly
//!   under the declared protocol. Matches the `static-check`
//!   taxonomy entry.
//!
//! Other kinds (`property-test` with random-case generation,
//! `formal-proof` via Coq/Lean artifact checking,
//! `convergence-preserving` via a round-trip-with-edits pipeline) are
//! straightforward to add under the same trait — they're deferred
//! until idiolect has callers asking for them.

pub mod error;
/// Generated from `verify-spec/runners.json` by `idiolect-codegen`.
/// Do not edit by hand; regenerate via
/// `cargo run -p idiolect-codegen -- generate`.
pub mod generated;
pub mod property_test;
pub mod roundtrip;
pub mod runner;
pub mod static_check;

pub use error::{VerifyError, VerifyResult};
pub use property_test::PropertyTestRunner;
pub use roundtrip::RoundtripTestRunner;
pub use runner::{VerificationRunner, VerificationTarget};
pub use static_check::StaticCheckRunner;
