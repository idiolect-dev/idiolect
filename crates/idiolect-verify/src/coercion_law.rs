//! [`CoercionLawRunner`]: dispatches a lens to panproto's
//! `dev.panproto.translate.verifyCoercionLaws` xrpc and reports any
//! returned `coercionLawViolation` entries as a falsified verification.
//!
//! Coercion laws constrain how a lens carries theory inhabitants
//! across a translation: signature preservation, axiom soundness,
//! coherence under composition. Panproto centralises the actual law
//! checking; idiolect's job here is to drive the call, package the
//! result as a `Verification` record, and stash a counterexample
//! pointer so consumers can chase the first violation.

use idiolect_lens::LensError;
use idiolect_records::generated::dev::idiolect::defs::{LpCoercionLaw, Tool};
use idiolect_records::generated::dev::idiolect::verification::{
    Verification, VerificationKind, VerificationProperty, VerificationResult,
};

use crate::error::{VerifyError, VerifyResult};
use crate::runner::{VerificationRunner, VerificationTarget, build_verification};

/// Minimal client surface for the coercion-law xrpc. The runner
/// stays generic over the transport so unit tests can plug a stub
/// while the deployed verifier wires up an http-backed client.
#[allow(async_fn_in_trait)]
pub trait CoercionLawClient: Send + Sync {
    /// Invoke `dev.panproto.translate.verifyCoercionLaws` for `lens_uri`
    /// against `standard` (and an optional `version` pin) and return
    /// the parsed violation list. An empty vec means the laws hold.
    ///
    /// # Errors
    ///
    /// Forward [`LensError::Transport`] for any backend-level failure
    /// and [`LensError::NotFound`] when the lens or standard is
    /// unknown to the panproto store.
    async fn verify_coercion_laws(
        &self,
        lens_uri: &str,
        standard: &str,
        version: Option<&str>,
    ) -> Result<Vec<CoercionLawViolation>, LensError>;
}

/// One entry from the verifyCoercionLaws response. The fields mirror
/// upstream panproto's `coercionLawViolation` shape; idiolect carries
/// them verbatim onto the falsified verification's counterexample.
#[derive(Debug, Clone)]
pub struct CoercionLawViolation {
    /// Identifier of the law that the lens failed to satisfy
    /// (e.g. `signature-preservation`, `axiom-soundness`).
    pub law: String,
    /// Operator-readable detail about the failure: which axiom, which
    /// inhabitant, which translation step. Empty when upstream does
    /// not provide one.
    pub detail: String,
}

/// Runner for the `coercion-law` verification kind.
///
/// Holds the panproto translate client and the standard the lens is
/// being checked against. The runner reports `Holds` when the client
/// returns no violations, `Falsified` when it returns at least one.
/// `violationThreshold` is stamped onto the emitted property as
/// metadata only — it documents the budget the verifier ran under
/// (so a `Holds` from a low-threshold run can be distinguished from
/// a `Holds` from an exhaustive one) and is not consumed by the
/// runner's own holds/falsified decision.
pub struct CoercionLawRunner<C> {
    client: C,
    standard: String,
    version: Option<String>,
    violation_threshold: Option<u32>,
}

impl<C> CoercionLawRunner<C> {
    /// Build a runner pinned to a specific coercion-law standard.
    pub const fn new(
        client: C,
        standard: String,
        version: Option<String>,
        violation_threshold: Option<u32>,
    ) -> Self {
        Self {
            client,
            standard,
            version,
            violation_threshold,
        }
    }
}

impl<C> VerificationRunner for CoercionLawRunner<C>
where
    C: CoercionLawClient,
{
    fn kind(&self) -> VerificationKind {
        VerificationKind::CoercionLaw
    }

    fn tool(&self) -> Tool {
        Tool {
            commit: None,
            name: "idiolect-verify/coercion-law".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }

    async fn run(&self, target: &VerificationTarget) -> VerifyResult<Verification> {
        let lens_uri = target.lens.uri.clone().ok_or_else(|| {
            VerifyError::InvalidInput(
                "target.lens has no uri; coercion-law runner needs an at-uri".into(),
            )
        })?;

        let property = || {
            VerificationProperty::LpCoercionLaw(LpCoercionLaw {
                standard: self.standard.clone(),
                version: self.version.clone(),
                violation_threshold: self.violation_threshold.map(i64::from),
            })
        };

        let violations = self
            .client
            .verify_coercion_laws(&lens_uri, &self.standard, self.version.as_deref())
            .await?;

        if violations.is_empty() {
            return Ok(build_verification(
                target,
                self,
                VerificationResult::Holds,
                None,
                property(),
            ));
        }

        // The lexicon's `counterexample` is a `cid-link` — a content
        // address pointing at a stored minimal counterexample, not a
        // free-form string. Until this runner has a content-
        // addressed store wired in, surface the first violation
        // through tracing and leave the field empty. `violationThreshold`
        // is a runtime parameter (how many violations the upstream
        // runner produced), not part of the holds/falsified decision:
        // any violation at all falsifies the claim.
        let first = &violations[0];
        tracing::info!(
            law = %first.law,
            detail = %first.detail,
            "coercion-law verification falsified",
        );

        Ok(build_verification(
            target,
            self,
            VerificationResult::Falsified,
            None,
            property(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use idiolect_records::generated::dev::idiolect::defs::LensRef;
    use std::sync::Mutex;

    #[derive(Default)]
    struct StubClient {
        violations: Mutex<Vec<CoercionLawViolation>>,
        last_call: Mutex<Option<(String, String, Option<String>)>>,
    }

    impl CoercionLawClient for StubClient {
        async fn verify_coercion_laws(
            &self,
            lens_uri: &str,
            standard: &str,
            version: Option<&str>,
        ) -> Result<Vec<CoercionLawViolation>, LensError> {
            *self.last_call.lock().unwrap() = Some((
                lens_uri.to_owned(),
                standard.to_owned(),
                version.map(str::to_owned),
            ));
            Ok(self.violations.lock().unwrap().clone())
        }
    }

    fn target() -> VerificationTarget {
        VerificationTarget {
            lens: LensRef {
                cid: None,
                direction: None,
                uri: Some(
                    idiolect_records::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/c1")
                        .expect("valid at-uri"),
                ),
            },
            verifier: idiolect_records::Did::parse("did:plc:verifier").expect("valid DID"),
            occurred_at: idiolect_records::Datetime::parse("2026-04-25T00:00:00.000Z")
                .expect("valid datetime"),
            tool_override: None,
        }
    }

    #[tokio::test]
    async fn empty_violation_list_holds() {
        let client = StubClient::default();
        let runner = CoercionLawRunner::new(client, "panproto-core".into(), None, None);
        let v = runner.run(&target()).await.unwrap();
        assert!(matches!(v.result, VerificationResult::Holds));
        assert!(matches!(
            v.property,
            VerificationProperty::LpCoercionLaw(LpCoercionLaw { .. })
        ));
        let last = runner.client.last_call.lock().unwrap().clone().unwrap();
        assert_eq!(last.0, "at://did:plc:x/dev.panproto.schema.lens/c1");
        assert_eq!(last.1, "panproto-core");
        assert!(last.2.is_none());
    }

    #[tokio::test]
    async fn nonempty_violation_list_falsifies() {
        let client = StubClient::default();
        *client.violations.lock().unwrap() = vec![
            CoercionLawViolation {
                law: "signature-preservation".into(),
                detail: "carrier of plus does not match".into(),
            },
            CoercionLawViolation {
                law: "axiom-soundness".into(),
                detail: "commutativity not preserved".into(),
            },
        ];

        let runner =
            CoercionLawRunner::new(client, "panproto-core".into(), Some("1.0".into()), Some(1));
        let v = runner.run(&target()).await.unwrap();
        assert!(matches!(v.result, VerificationResult::Falsified));
        // The lexicon's `counterexample` is a cid-link, not a free-
        // form description. Until a content-addressed store is wired
        // in, the runner emits the falsifying detail through tracing
        // and leaves the lexicon field empty.
        assert!(v.counterexample.is_none());
    }

    #[tokio::test]
    async fn missing_lens_uri_is_invalid_input() {
        let client = StubClient::default();
        let runner = CoercionLawRunner::new(client, "panproto-core".into(), None, None);
        let mut t = target();
        t.lens.uri = None;
        let err = runner.run(&t).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidInput(_)));
    }
}
