//! Core [`VerificationRunner`] trait + supporting types.

use idiolect_records::generated::dev::idiolect::defs::{LensRef, Tool};
use idiolect_records::generated::dev::idiolect::verification::{
    Verification, VerificationKind, VerificationProperty, VerificationResult,
};

use crate::error::VerifyResult;

/// Caller-supplied metadata needed to stamp a verification record.
///
/// The trait's [`run`](VerificationRunner::run) method produces a
/// `Verification` body; these fields let the caller customize
/// everything around *what property was verified* without the runner
/// itself reaching for clock / identity / tool versions.
#[derive(Debug, Clone)]
pub struct VerificationTarget {
    /// The lens the verification applies to. Echoed back on the
    /// published record's `lens` field.
    pub lens: LensRef,
    /// DID of the party publishing the verification. Echoed back on
    /// `verifier`.
    pub verifier: String,
    /// Timestamp the verifier recorded the result. Echoed back on
    /// `occurred_at`.
    pub occurred_at: String,
    /// Tool identity + version. Populated by the runner by default
    /// (e.g. `RoundtripTestRunner` names itself) but callers can
    /// override for the published record.
    pub tool_override: Option<Tool>,
}

/// Synchronous-or-async verification runner.
///
/// A runner is stateful (it holds e.g. a random-seed for property
/// tests, or a compiled panproto lens). Calling [`run`](Self::run)
/// consumes the input corpus and returns a `Verification` record.
///
/// Runners return a `Verification` with `result = Holds` /
/// `Falsified` / `Inconclusive` rather than raising on a failed
/// property: a falsified verification is a *first-class record*, not
/// an error. [`VerifyError`](crate::VerifyError) is reserved for
/// input-shape or transport failures.
#[allow(async_fn_in_trait)]
pub trait VerificationRunner: Send + Sync {
    /// The property kind this runner checks. Used to stamp the
    /// returned [`Verification`]'s `kind` field and to route a
    /// population of verifications through a [`VerificationKind`]-
    /// dispatch.
    fn kind(&self) -> VerificationKind;

    /// Canonical tool identifier for this runner. Forms the default
    /// `tool` field on the verification record; overridable per-run
    /// via [`VerificationTarget::tool_override`].
    fn tool(&self) -> Tool;

    /// Run the verification and produce a record.
    ///
    /// # Errors
    ///
    /// Returns [`VerifyError`](crate::VerifyError) for input-shape,
    /// transport, or irrecoverable-state failures. A *falsified*
    /// property returns `Ok(Verification { result: Falsified, ... })`
    /// — not an error — because falsification is the signal the
    /// community is paying the runner to produce.
    async fn run(&self, target: &VerificationTarget) -> VerifyResult<Verification>;
}

/// Package a runner result into a [`Verification`] record shaped for
/// direct publication via [`idiolect_lens::RecordPublisher::create`].
///
/// `property` is the structured statement of what the verifier is
/// asserting — a `LensProperty` variant sized to match `runner.kind()`
/// (see `dev.idiolect.defs#lensProperty` in the lexicons for the
/// authoritative shape).
#[must_use]
pub fn build_verification<R: VerificationRunner + ?Sized>(
    target: &VerificationTarget,
    runner: &R,
    result: VerificationResult,
    counterexample: Option<String>,
    property: VerificationProperty,
) -> Verification {
    Verification {
        basis: None,
        counterexample,
        dependencies: None,
        property,
        kind: runner.kind(),
        lens: target.lens.clone(),
        occurred_at: target.occurred_at.clone(),
        proof_artifact: None,
        result,
        tool: target
            .tool_override
            .clone()
            .unwrap_or_else(|| runner.tool()),
        verifier: target.verifier.clone(),
    }
}

/// Forward [`VerificationRunner`] through `Arc<T>` so a single
/// runner can be shared across tasks (matches the `Arc<T>` blanket
/// impls on every other idiolect boundary trait).
impl<T: VerificationRunner + ?Sized> VerificationRunner for std::sync::Arc<T> {
    fn kind(&self) -> VerificationKind {
        (**self).kind()
    }

    fn tool(&self) -> Tool {
        (**self).tool()
    }

    async fn run(&self, target: &VerificationTarget) -> VerifyResult<Verification> {
        (**self).run(target).await
    }
}
