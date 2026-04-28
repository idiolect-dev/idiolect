//! [`RoundtripTestRunner`]: applies a lens forward then backward on a
//! corpus of source records and checks that `put(get(src)) == src` for
//! every one. A single counterexample falsifies the verification.
//!
//! This is the simplest way to empirically check that a lens is lossy-
//! at-worst-in-known-ways — `iso` lenses should round-trip every
//! record, `projection` lenses should round-trip records in the
//! projection's fibre, and `opaque` lenses should never claim the
//! roundtrip-test property.

use idiolect_lens::{
    ApplyLensInput, ApplyLensPutInput, Resolver, SchemaLoader, apply_lens, apply_lens_put,
};
use idiolect_records::generated::dev::idiolect::defs::{LpRoundtrip, Tool};
use idiolect_records::generated::dev::idiolect::verification::{
    Verification, VerificationKind, VerificationProperty, VerificationResult,
};
use panproto_schema::Protocol;

use crate::error::{VerifyError, VerifyResult};
use crate::runner::{VerificationRunner, VerificationTarget, build_verification};

/// Runner for the `roundtrip-test` verification kind.
///
/// Drives `apply_lens` → `apply_lens_put` on every record in a
/// caller-supplied corpus and reports falsification on the first
/// mismatch. Returns `Holds` only if every corpus record round-trips
/// byte-for-byte.
///
/// Generic over the resolver and schema-loader so the same runner
/// works against in-memory fixtures (unit tests) and live PDS+VCS
/// stores (deployed verifier).
pub struct RoundtripTestRunner<R, L> {
    resolver: R,
    schema_loader: L,
    protocol: Protocol,
    /// Corpus of source-side records to round-trip. Each is a raw
    /// lexicon-shaped JSON object matching the lens's source schema.
    corpus: Vec<serde_json::Value>,
}

impl<R, L> RoundtripTestRunner<R, L> {
    /// Construct a runner with an explicit corpus.
    pub const fn new(
        resolver: R,
        schema_loader: L,
        protocol: Protocol,
        corpus: Vec<serde_json::Value>,
    ) -> Self {
        Self {
            resolver,
            schema_loader,
            protocol,
            corpus,
        }
    }

    /// Number of records in the corpus (stamped on the verification's
    /// `input_space` field for operator debugging).
    #[must_use]
    pub const fn corpus_size(&self) -> usize {
        self.corpus.len()
    }
}

impl<R, L> VerificationRunner for RoundtripTestRunner<R, L>
where
    R: Resolver,
    L: SchemaLoader,
{
    fn kind(&self) -> VerificationKind {
        VerificationKind::RoundtripTest
    }

    fn tool(&self) -> Tool {
        Tool {
            commit: None,
            name: "idiolect-verify/roundtrip-test".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }

    async fn run(&self, target: &VerificationTarget) -> VerifyResult<Verification> {
        if self.corpus.is_empty() {
            return Err(VerifyError::InvalidInput(
                "roundtrip-test corpus is empty".into(),
            ));
        }

        let lens_uri = target.lens.uri.clone().ok_or_else(|| {
            VerifyError::InvalidInput(
                "target.lens has no uri; roundtrip-test needs an at-uri to resolve".into(),
            )
        })?;
        // RoundtripTest asserts the identity holds on a caller-supplied
        // corpus; the LensProperty carries the corpus size as a symbolic
        // domain descriptor so consumers can tell coverage apart.
        let property = || {
            VerificationProperty::LpRoundtrip(LpRoundtrip {
                domain: format!("corpus:{} records", self.corpus.len()),
                generator: None,
            })
        };

        for (i, source) in self.corpus.iter().enumerate() {
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

            if back.source_record != *source {
                // `counterexample` is a cid-link referring to a
                // stored counterexample blob; without a content-
                // addressed store the runner can't produce one,
                // so surface the corpus index via tracing and
                // leave the lexicon field empty.
                tracing::info!(corpus_index = i, "roundtrip verification falsified",);
                return Ok(build_verification(
                    target,
                    self,
                    VerificationResult::Falsified,
                    None,
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
    use idiolect_records::generated::dev::idiolect::defs::LensRef;
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

    fn stage_identity() -> (
        idiolect_records::AtUri,
        InMemoryResolver,
        InMemorySchemaLoader,
    ) {
        let src = schema();
        let protolens = elementary::rename_sort("string", "string");
        let tgt = protolens.target_schema(&src, &test_protocol()).unwrap();
        let src_hash = "at://did:plc:x/dev.panproto.schema.schema/src-rt".to_owned();
        let tgt_hash = "at://did:plc:x/dev.panproto.schema.schema/tgt-rt".to_owned();
        let mut loader = InMemorySchemaLoader::new();
        loader.insert(src_hash.clone(), src);
        loader.insert(tgt_hash.clone(), tgt);

        let uri =
            idiolect_lens::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/rt").unwrap();
        let record = PanprotoLens {
            blob: Some(serde_json::to_value(&protolens).unwrap()),
            created_at: idiolect_records::Datetime::parse("2026-04-21T00:00:00.000Z")
                .expect("valid datetime"),
            laws_verified: Some(true),
            object_hash: "sha256:lens-rt".into(),
            round_trip_class: Some("isomorphism".into()),
            source_schema: idiolect_records::AtUri::parse(&src_hash).expect("valid at-uri"),
            target_schema: idiolect_records::AtUri::parse(&tgt_hash).expect("valid at-uri"),
        };
        let mut resolver = InMemoryResolver::new();
        resolver.insert(&uri, record);
        (uri, resolver, loader)
    }

    fn target(uri: idiolect_records::AtUri) -> VerificationTarget {
        VerificationTarget {
            lens: LensRef {
                cid: None,
                direction: None,
                uri: Some(uri),
            },
            verifier: idiolect_records::Did::parse("did:plc:verifier").expect("valid DID"),
            occurred_at: idiolect_records::Datetime::parse("2026-04-21T00:00:00Z")
                .expect("valid datetime"),
            tool_override: None,
        }
    }

    #[tokio::test]
    async fn holds_on_identity_roundtrip() {
        let (uri, resolver, loader) = stage_identity();
        let runner = RoundtripTestRunner::new(
            resolver,
            loader,
            test_protocol(),
            vec![
                serde_json::json!({ "text": "hello" }),
                serde_json::json!({ "text": "world" }),
            ],
        );
        assert_eq!(runner.corpus_size(), 2);
        let v = runner.run(&target(uri)).await.unwrap();
        assert_eq!(v.kind, VerificationKind::RoundtripTest);
        assert_eq!(v.result, VerificationResult::Holds);
        let VerificationProperty::LpRoundtrip(ref p) = v.property else {
            panic!("expected LpRoundtrip, got {:?}", v.property);
        };
        assert!(p.domain.contains("2 records"));
    }

    #[tokio::test]
    async fn empty_corpus_is_invalid_input() {
        let (uri, resolver, loader) = stage_identity();
        let runner = RoundtripTestRunner::new(resolver, loader, test_protocol(), Vec::new());
        let err = runner.run(&target(uri)).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn missing_lens_uri_is_invalid_input() {
        let (_uri, resolver, loader) = stage_identity();
        let runner = RoundtripTestRunner::new(
            resolver,
            loader,
            test_protocol(),
            vec![serde_json::json!({ "text": "x" })],
        );
        let mut target = target(
            idiolect_records::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/placeholder")
                .expect("valid"),
        );
        target.lens.uri = None;
        let err = runner.run(&target).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn unresolvable_lens_surfaces_lens_error() {
        let (_, _resolver, loader) = stage_identity();
        let runner = RoundtripTestRunner::new(
            InMemoryResolver::new(), // empty
            loader,
            test_protocol(),
            vec![serde_json::json!({ "text": "x" })],
        );
        let t = target(
            idiolect_records::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/missing")
                .expect("valid"),
        );
        let err = runner.run(&t).await.unwrap_err();
        assert!(matches!(err, VerifyError::Lens(_)));
    }
}
