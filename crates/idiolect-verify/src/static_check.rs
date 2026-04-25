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

use idiolect_lens::{Resolver, SchemaLoader, };
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
        let parsed = idiolect_lens::AtUri::parse(&lens_uri)?;
        let lens_record = self.resolver.resolve(&parsed).await?;

        // Load both schemas and validate each.
        let src = self.schema_loader.load(&lens_record.source_schema).await?;
        let tgt = self.schema_loader.load(&lens_record.target_schema).await?;

        let src_errors = validate(&src, &self.protocol);
        let tgt_errors = validate(&tgt, &self.protocol);

        if !src_errors.is_empty() || !tgt_errors.is_empty() {
            use std::fmt::Write as _;
            let mut detail = String::new();
            if !src_errors.is_empty() {
                write!(
                    detail,
                    "source schema {}: {} validation error(s); ",
                    lens_record.source_schema,
                    src_errors.len()
                )
                .expect("write to String");
            }
            if !tgt_errors.is_empty() {
                write!(
                    detail,
                    "target schema {}: {} validation error(s)",
                    lens_record.target_schema,
                    tgt_errors.len()
                )
                .expect("write to String");
            }
            return Ok(build_verification(
                target,
                self,
                VerificationResult::Falsified,
                Some(detail),
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

    fn stage(valid_target: bool) -> (String, InMemoryResolver, InMemorySchemaLoader) {
        let src = valid_schema();
        let protolens = elementary::rename_sort("string", "string");
        let tgt = protolens.target_schema(&src, &test_protocol()).unwrap();

        let src_hash = "sha256:src".to_owned();
        let tgt_hash = "sha256:tgt".to_owned();
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
            created_at: "2026-04-21T00:00:00.000Z".into(),
            laws_verified: Some(true),
            object_hash: "sha256:lens-sc".into(),
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
        assert!(v.counterexample.is_some());
    }

    #[tokio::test]
    async fn missing_lens_uri_is_invalid_input() {
        let (uri, resolver, loader) = stage(true);
        let _ = uri;
        let runner = StaticCheckRunner::new(resolver, loader, test_protocol());
        let mut t = target("ignored".into());
        t.lens.uri = None;
        let err = runner.run(&t).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidInput(_)));
    }
}
