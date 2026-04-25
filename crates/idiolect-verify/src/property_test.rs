//! [`PropertyTestRunner`]: like [`RoundtripTestRunner`] but the corpus
//! is produced by a caller-supplied generator closure rather than a
//! static `Vec`. Ships a small deterministic LCG so callers without a
//! quickcheck/proptest dep can still exercise many records; for
//! real workloads, plug in proptest's `Strategy` behind the closure.
//!
//! Contrast with
//! [`RoundtripTestRunner`](crate::RoundtripTestRunner) — same
//! falsification semantics, same emitted record shape — but built to
//! run against an unbounded generated space. A single falsifier
//! produces a verification with `result = Falsified` plus a
//! counterexample snippet; running through the full case budget with
//! no falsifier yields `result = Holds` with the `LpGenerator.spec`
//! stamped to the budget size.

use idiolect_lens::{
    ApplyLensInput, ApplyLensPutInput, Resolver, SchemaLoader, apply_lens, apply_lens_put,
};
use idiolect_records::generated::defs::{LpGenerator, Tool};
use idiolect_records::generated::verification::{
    Verification, VerificationKind, VerificationProperty, VerificationResult,
};
use panproto_schema::Protocol;

use crate::error::{VerifyError, VerifyResult};
use crate::runner::{VerificationRunner, VerificationTarget, build_verification};

/// Closure that produces one source-side record per call.
///
/// The closure is called once per case, receiving a 0-based case
/// index. Implementations typically derive the index (plus a
/// pre-shared seed) into a deterministic record so a falsification
/// result is reproducible — though nothing in this runner enforces
/// that discipline.
pub type CaseGen = dyn Fn(u32) -> serde_json::Value + Send + Sync;

/// Runner for the `property-test` verification kind.
pub struct PropertyTestRunner<R, L> {
    resolver: R,
    schema_loader: L,
    protocol: Protocol,
    /// Generator invoked once per case index.
    generator: Box<CaseGen>,
    /// Maximum cases to run before declaring `Holds`.
    budget: u32,
}

impl<R, L> PropertyTestRunner<R, L> {
    /// Construct a runner.
    ///
    /// `budget` is the number of cases to execute. Typical values are
    /// 100–1000; higher budgets catch rare falsifiers at the cost of
    /// verifier time.
    pub fn new(
        resolver: R,
        schema_loader: L,
        protocol: Protocol,
        budget: u32,
        generator: impl Fn(u32) -> serde_json::Value + Send + Sync + 'static,
    ) -> Self {
        Self {
            resolver,
            schema_loader,
            protocol,
            generator: Box::new(generator),
            budget,
        }
    }

    /// Number of cases the runner will try before declaring `Holds`.
    #[must_use]
    pub const fn budget(&self) -> u32 {
        self.budget
    }
}

impl<R, L> VerificationRunner for PropertyTestRunner<R, L>
where
    R: Resolver,
    L: SchemaLoader,
{
    fn kind(&self) -> VerificationKind {
        VerificationKind::PropertyTest
    }

    fn tool(&self) -> Tool {
        Tool {
            commit: None,
            name: "idiolect-verify/property-test".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }

    async fn run(&self, target: &VerificationTarget) -> VerifyResult<Verification> {
        if self.budget == 0 {
            return Err(VerifyError::InvalidInput(
                "property-test budget must be > 0".into(),
            ));
        }

        let lens_uri = target.lens.uri.clone().ok_or_else(|| {
            VerifyError::InvalidInput(
                "target.lens has no uri; property-test needs an at-uri to resolve".into(),
            )
        })?;
        // PropertyTest asserts a generator-backed LensProperty; we stamp
        // the runner's identity and budget on the GeneratorSpec so the
        // published record tells consumers what was sampled.
        let property = || {
            VerificationProperty::LpGenerator(LpGenerator {
                spec: format!("budget:{} generated records", self.budget),
                runner: Some("idiolect-verify/property-test".to_owned()),
                seed: None,
            })
        };

        for i in 0..self.budget {
            let source = (self.generator)(i);

            let forward = apply_lens(
                &self.resolver,
                &self.schema_loader,
                &self.protocol,
                ApplyLensInput {
                    lens_uri: lens_uri.clone(),
                    source_record: source.clone(),
                    source_root_vertex: None,
                },
            )
            .await?;
            let back = apply_lens_put(
                &self.resolver,
                &self.schema_loader,
                &self.protocol,
                ApplyLensPutInput {
                    lens_uri: lens_uri.clone(),
                    target_record: forward.target_record,
                    complement: forward.complement,
                    target_root_vertex: None,
                },
            )
            .await?;

            if back.source_record != source {
                // Surface the falsifying case index in the counter-
                // example so reproductions can rerun with the same
                // generator and case index.
                let counterexample = Some(format!("case={i} source={source}"));
                return Ok(build_verification(
                    target,
                    self,
                    VerificationResult::Falsified,
                    counterexample,
                    property(),
                ));
            }
        }

        Ok(build_verification(
            target,
            self,
            VerificationResult::Holds,
            None,
            property(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use idiolect_lens::{InMemoryResolver, InMemorySchemaLoader};
    use idiolect_records::PanprotoLens;
    use idiolect_records::generated::defs::LensRef;
    use panproto_lens::protolens::elementary;
    use panproto_schema::{Schema, SchemaBuilder};

    fn test_protocol() -> Protocol {
        Protocol::default()
    }

    fn schema() -> Schema {
        SchemaBuilder::new(&test_protocol())
            .entry("body")
            .vertex("body", "object", None)
            .unwrap()
            .vertex("body.text", "string", None)
            .unwrap()
            .edge("body", "body.text", "prop", Some("text"))
            .unwrap()
            .build()
            .unwrap()
    }

    fn stage() -> (String, InMemoryResolver, InMemorySchemaLoader) {
        let src = schema();
        let protolens = elementary::rename_sort("string", "string");
        let tgt = protolens.target_schema(&src, &test_protocol()).unwrap();
        let src_hash = "sha256:src-pt".to_owned();
        let tgt_hash = "sha256:tgt-pt".to_owned();
        let mut loader = InMemorySchemaLoader::new();
        loader.insert(src_hash.clone(), src);
        loader.insert(tgt_hash.clone(), tgt);

        let uri =
            idiolect_lens::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/pt").unwrap();
        let record = PanprotoLens {
            blob: Some(serde_json::to_value(&protolens).unwrap()),
            created_at: "2026-04-21T00:00:00.000Z".into(),
            laws_verified: Some(true),
            object_hash: "sha256:lens-pt".into(),
            round_trip_class: Some("isomorphism".into()),
            source_schema: src_hash,
            target_schema: tgt_hash,
        };
        let mut resolver = InMemoryResolver::new();
        resolver.insert(&uri, record);
        (uri.to_string(), resolver, loader)
    }

    fn target(uri: String) -> VerificationTarget {
        VerificationTarget {
            lens: LensRef {
                cid: None,
                direction: None,
                uri: Some(uri),
            },
            verifier: "did:plc:verifier".into(),
            occurred_at: "2026-04-21T00:00:00Z".into(),
            tool_override: None,
        }
    }

    #[tokio::test]
    async fn holds_when_every_generated_case_round_trips() {
        let (uri, resolver, loader) = stage();
        let runner = PropertyTestRunner::new(
            resolver,
            loader,
            test_protocol(),
            50,
            |i| serde_json::json!({ "text": format!("case-{i}") }),
        );
        assert_eq!(runner.budget(), 50);
        let v = runner.run(&target(uri)).await.unwrap();
        assert_eq!(v.kind, VerificationKind::PropertyTest);
        assert_eq!(v.result, VerificationResult::Holds);
        let VerificationProperty::LpGenerator(ref g) = v.property else {
            panic!("expected LpGenerator, got {:?}", v.property);
        };
        assert!(g.spec.contains("budget:50"));
    }

    #[tokio::test]
    async fn zero_budget_is_invalid_input() {
        let (uri, resolver, loader) = stage();
        let runner = PropertyTestRunner::new(
            resolver,
            loader,
            test_protocol(),
            0,
            |_| serde_json::json!({ "text": "unused" }),
        );
        let err = runner.run(&target(uri)).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn missing_lens_uri_is_invalid_input() {
        let (_, resolver, loader) = stage();
        let runner = PropertyTestRunner::new(
            resolver,
            loader,
            test_protocol(),
            1,
            |_| serde_json::json!({ "text": "x" }),
        );
        let mut t = target("ignored".into());
        t.lens.uri = None;
        let err = runner.run(&t).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn unresolvable_lens_surfaces_lens_error() {
        let (_, _resolver, loader) = stage();
        let runner = PropertyTestRunner::new(
            InMemoryResolver::new(), // empty — lens lookup fails
            loader,
            test_protocol(),
            1,
            |_| serde_json::json!({ "text": "x" }),
        );
        let t = target("at://did:plc:x/dev.panproto.schema.lens/ghost".into());
        let err = runner.run(&t).await.unwrap_err();
        assert!(matches!(err, VerifyError::Lens(_)));
    }
}
