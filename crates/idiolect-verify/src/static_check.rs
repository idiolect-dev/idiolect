//! [`StaticCheckRunner`]: runs panproto's schema validator on the
//! lens's source and target schemas.
//!
//! `static-check` in the idiolect taxonomy means: the lens's declared
//! schemas must themselves be valid panproto graphs under the declared
//! protocol. A lens that claims to translate between two schemas
//! whose graphs are invalid can never be formally sound; catching this
//! at verification time is cheap.
//!
//! The runner does NOT run the lens itself — that's
//! [`RoundtripTestRunner`](crate::RoundtripTestRunner)'s job. This
//! runner only validates the static shape of the surrounding graphs.

use idiolect_lens::{Resolver, SchemaLoader};
use idiolect_records::generated::dev::idiolect::defs::{LpChecker, Tool};
use idiolect_records::generated::dev::idiolect::verification::{
    Verification, VerificationKind, VerificationProperty, VerificationResult,
};
use panproto_schema::{Protocol, validate};

use crate::error::{VerifyError, VerifyResult};
use crate::runner::{VerificationRunner, VerificationTarget, build_verification};

/// Runner for the `static-check` verification kind.
pub struct StaticCheckRunner<R, L> {
    resolver: R,
    schema_loader: L,
    protocol: Protocol,
}

impl<R, L> StaticCheckRunner<R, L> {
    /// Construct a runner. The protocol is used as
    /// `validate::validate`'s second argument — normally the atproto
    /// protocol or a project-specific one.
    pub const fn new(resolver: R, schema_loader: L, protocol: Protocol) -> Self {
        Self {
            resolver,
            schema_loader,
            protocol,
        }
    }
}

impl<R, L> VerificationRunner for StaticCheckRunner<R, L>
where
    R: Resolver,
    L: SchemaLoader,
{
    fn kind(&self) -> VerificationKind {
        VerificationKind::StaticCheck
    }

    fn tool(&self) -> Tool {
        Tool {
            commit: None,
            name: "idiolect-verify/static-check".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }

    async fn run(&self, target: &VerificationTarget) -> VerifyResult<Verification> {
        let lens_uri = target.lens.uri.clone().ok_or_else(|| {
            VerifyError::InvalidInput(
                "target.lens has no uri; static-check needs an at-uri to resolve".into(),
            )
        })?;

        // Resolve the lens record so we can pull source+target schema hashes.
        let lens_record = self.resolver.resolve(&lens_uri).await?;

        // Load both schemas and validate each.
        let src = self
            .schema_loader
            .load(lens_record.source_schema.as_str())
            .await?;
        let tgt = self
            .schema_loader
            .load(lens_record.target_schema.as_str())
            .await?;

        let src_errors = validate(&src, &self.protocol);
        let tgt_errors = validate(&tgt, &self.protocol);

        if !src_errors.is_empty() || !tgt_errors.is_empty() {
            // The lexicon's `counterexample` is a cid-link referring
            // to a stored counterexample blob; without a content-
            // addressed store the runner can't produce one, so
            // surface the per-schema error count through tracing and
            // leave the lexicon field empty.
            tracing::info!(
                source_schema = %lens_record.source_schema,
                source_errors = src_errors.len(),
                target_schema = %lens_record.target_schema,
                target_errors = tgt_errors.len(),
                "static-check verification falsified",
            );
            return Ok(build_verification(
                target,
                self,
                VerificationResult::Falsified,
                None,
                self.property(),
            ));
        }

        Ok(build_verification(
            target,
            self,
            VerificationResult::Holds,
            None,
            self.property(),
        ))
    }
}

impl<R, L> StaticCheckRunner<R, L> {
    /// `StaticCheck` asserts the lens's schemas validate under a named
    /// checker + protocol pair; encode that as [`LpChecker`].
    fn property(&self) -> VerificationProperty {
        VerificationProperty::LpChecker(LpChecker {
            checker: "panproto::validate".to_owned(),
            ruleset: Some(self.protocol.name.clone()),
            version: None,
        })
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

    fn valid_schema() -> Schema {
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

    fn stage(
        valid_target: bool,
    ) -> (
        idiolect_records::AtUri,
        InMemoryResolver,
        InMemorySchemaLoader,
    ) {
        let src = valid_schema();
        let protolens = elementary::rename_sort("string", "string");
        let tgt = protolens.target_schema(&src, &test_protocol()).unwrap();

        let src_hash = "at://did:plc:x/dev.panproto.schema.schema/src".to_owned();
        let tgt_hash = "at://did:plc:x/dev.panproto.schema.schema/tgt".to_owned();
        let mut loader = InMemorySchemaLoader::new();
        loader.insert(src_hash.clone(), src);
        if valid_target {
            loader.insert(tgt_hash.clone(), tgt);
        } else {
            // Register a schema whose constraint sort the protocol
            // does not know about, forcing validate::validate to
            // return an error.
            let bad = SchemaBuilder::new(&test_protocol())
                .entry("body")
                .vertex("body", "object", None)
                .unwrap()
                .constraint("body", "not-a-real-sort", "x")
                .build()
                .unwrap();
            loader.insert(tgt_hash.clone(), bad);
        }

        let uri =
            idiolect_lens::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/sc").unwrap();
        let record = PanprotoLens {
            blob: Some(serde_json::to_value(&protolens).unwrap()),
            created_at: idiolect_records::Datetime::parse("2026-04-21T00:00:00.000Z")
                .expect("valid datetime"),
            laws_verified: Some(true),
            object_hash: "sha256:lens-sc".into(),
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
    async fn holds_when_both_schemas_validate() {
        let (uri, resolver, loader) = stage(true);
        let runner = StaticCheckRunner::new(resolver, loader, test_protocol());
        let v = runner.run(&target(uri)).await.unwrap();
        assert_eq!(v.kind, VerificationKind::StaticCheck);
        assert_eq!(v.result, VerificationResult::Holds);
    }

    #[tokio::test]
    async fn falsified_when_target_schema_has_invalid_constraint() {
        // Default Protocol accepts any constraint sort. Build a
        // restrictive protocol whose constraint_sorts list does NOT
        // include "not-a-real-sort" so the validator rejects it.
        let mut restrictive = test_protocol();
        restrictive.name = "restrictive".to_owned();
        restrictive.constraint_sorts = vec!["minLength".into()];
        restrictive.obj_kinds = vec!["object".into()];
        let (uri, resolver, loader) = stage(false);
        let runner = StaticCheckRunner::new(resolver, loader, restrictive);
        let v = runner.run(&target(uri)).await.unwrap();
        assert_eq!(v.result, VerificationResult::Falsified);
        // The lexicon's `counterexample` is a cid-link, not a free-
        // form description. Until a content-addressed store is wired
        // in, the runner emits the falsifying detail through tracing
        // and leaves the lexicon field empty.
        assert!(v.counterexample.is_none());
    }

    #[tokio::test]
    async fn missing_lens_uri_is_invalid_input() {
        let (uri, resolver, loader) = stage(true);
        let _ = uri;
        let runner = StaticCheckRunner::new(resolver, loader, test_protocol());
        let mut t = target(
            idiolect_records::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/placeholder")
                .expect("valid"),
        );
        t.lens.uri = None;
        let err = runner.run(&t).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidInput(_)));
    }
}
