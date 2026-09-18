//! Signed, immutable community release bundles.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use p256::SecretKey;
use p256::ecdsa::signature::{Signer, Verifier};
use p256::ecdsa::{Signature, SigningKey, VerifyingKey};
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

use crate::error::{CommunityError, CommunityResult};
use crate::governance::{now, verification_gate};
use crate::model::{
    ChangePacket, ChangeStatus, CommunityRelease, DecisionStatus, ReleaseArtifact,
    ReleaseSignature, WorkspaceManifest,
};

/// Portable P-256 key used for detached community release signatures.
///
/// The encoded secret is sensitive. CLI-created key files are written with
/// owner-only permissions on Unix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunitySigningKey {
    /// Algorithm identifier (`ES256`).
    pub algorithm: String,
    /// Base64url raw P-256 secret scalar.
    pub secret_key: String,
    /// Base64url uncompressed SEC1 public point.
    pub public_key: String,
}

/// Generate a fresh P-256 community signing key.
#[must_use]
pub fn generate_signing_key() -> CommunitySigningKey {
    let signing = SigningKey::random(&mut thread_rng());
    let secret = SecretKey::from(&signing);
    let public = signing.verifying_key().to_encoded_point(false);
    CommunitySigningKey {
        algorithm: "ES256".to_owned(),
        secret_key: URL_SAFE_NO_PAD.encode(secret.to_bytes()),
        public_key: URL_SAFE_NO_PAD.encode(public.as_bytes()),
    }
}

/// Construct an unsigned release after enforcing governance, review-period,
/// and verification requirements for every included change.
///
/// # Errors
///
/// Returns an error for an invalid version, empty or unapproved change set,
/// failed verification gate, incomplete review period, or serialization error.
pub fn create_release(
    manifest: &WorkspaceManifest,
    version: impl Into<String>,
    changes: &[ChangePacket],
    artifacts: Vec<ReleaseArtifact>,
) -> CommunityResult<CommunityRelease> {
    let version = version.into();
    validate_version(&version)?;
    if changes.is_empty() {
        return Err(CommunityError::Governance(
            "a release must contain at least one approved change".to_owned(),
        ));
    }
    let now_time = OffsetDateTime::now_utc();
    for packet in changes {
        if packet.decision.status != DecisionStatus::Approved
            || !matches!(
                packet.status,
                ChangeStatus::Approved | ChangeStatus::Released
            )
        {
            return Err(CommunityError::Governance(format!(
                "change `{}` is not approved",
                packet.id
            )));
        }
        verification_gate(packet, manifest)?;
        let created = OffsetDateTime::parse(&packet.created_at, &Rfc3339).map_err(|e| {
            CommunityError::Invalid(format!("change `{}` creation time: {e}", packet.id))
        })?;
        let required = Duration::days(i64::from(manifest.governance.review_period_days));
        if now_time - created < required {
            return Err(CommunityError::Governance(format!(
                "change `{}` has not completed the {}-day review period",
                packet.id, manifest.governance.review_period_days
            )));
        }
    }
    let manifest_bytes = toml::to_string(manifest)?.into_bytes();
    let mut release = CommunityRelease {
        format_version: 1,
        community: manifest.community.did.clone(),
        version,
        manifest_digest: sha256(&manifest_bytes),
        manifest: manifest.clone(),
        changes: changes.to_vec(),
        artifacts,
        dependencies: manifest.federation.clone(),
        digest: String::new(),
        signatures: Vec::new(),
        created_at: now()?,
    };
    release.digest = canonical_digest(&release)?;
    Ok(release)
}

/// Append an ES256 detached signature over the release's canonical digest.
///
/// # Errors
///
/// Returns an error when the key is unsupported or inconsistent, the release
/// digest is stale, signing material is invalid, or the timestamp fails.
pub fn sign_release(
    release: &mut CommunityRelease,
    signer: impl Into<String>,
    key: &CommunitySigningKey,
) -> CommunityResult<ReleaseSignature> {
    if key.algorithm != "ES256" {
        return Err(CommunityError::Signature(format!(
            "unsupported key algorithm `{}`",
            key.algorithm
        )));
    }
    let expected = canonical_digest(release)?;
    if release.digest != expected {
        return Err(CommunityError::Signature(
            "release digest does not match its content".to_owned(),
        ));
    }
    let secret_bytes = URL_SAFE_NO_PAD
        .decode(&key.secret_key)
        .map_err(|e| CommunityError::Signature(format!("decode secret key: {e}")))?;
    let signing = SigningKey::from_slice(&secret_bytes)
        .map_err(|e| CommunityError::Signature(format!("load secret key: {e}")))?;
    let derived =
        URL_SAFE_NO_PAD.encode(signing.verifying_key().to_encoded_point(false).as_bytes());
    if derived != key.public_key {
        return Err(CommunityError::Signature(
            "public key does not correspond to secret key".to_owned(),
        ));
    }
    let signature: Signature = signing.sign(release.digest.as_bytes());
    let record = ReleaseSignature {
        signer: signer.into(),
        algorithm: "ES256".to_owned(),
        public_key: key.public_key.clone(),
        signature: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
        signed_at: now()?,
    };
    release
        .signatures
        .retain(|existing| existing.signer != record.signer);
    release.signatures.push(record.clone());
    Ok(record)
}

/// Verify the bundle digest and every detached signature.
///
/// Returns the number of distinct valid signers. Callers compare this with the
/// release policy's `min_signatures` threshold.
///
/// # Errors
///
/// Returns an error when the bundle digest differs from its content or any
/// detached signature or public key is malformed or invalid.
pub fn verify_release(release: &CommunityRelease) -> CommunityResult<usize> {
    let expected = canonical_digest(release)?;
    if release.digest != expected {
        return Err(CommunityError::Signature(format!(
            "release digest mismatch: expected {expected}, found {}",
            release.digest
        )));
    }
    let mut signers = std::collections::BTreeSet::new();
    for item in &release.signatures {
        if item.algorithm != "ES256" {
            return Err(CommunityError::Signature(format!(
                "unsupported signature algorithm `{}`",
                item.algorithm
            )));
        }
        let public = URL_SAFE_NO_PAD
            .decode(&item.public_key)
            .map_err(|e| CommunityError::Signature(format!("decode public key: {e}")))?;
        let verifying = VerifyingKey::from_sec1_bytes(&public)
            .map_err(|e| CommunityError::Signature(format!("load public key: {e}")))?;
        let signature_bytes = URL_SAFE_NO_PAD
            .decode(&item.signature)
            .map_err(|e| CommunityError::Signature(format!("decode signature: {e}")))?;
        let signature = Signature::from_slice(&signature_bytes)
            .map_err(|e| CommunityError::Signature(format!("load signature: {e}")))?;
        verifying
            .verify(release.digest.as_bytes(), &signature)
            .map_err(|e| {
                CommunityError::Signature(format!("signature from {} is invalid: {e}", item.signer))
            })?;
        signers.insert(item.signer.as_str());
    }
    Ok(signers.len())
}

/// Digest one release artifact from bytes.
#[must_use]
pub fn artifact(
    location: impl Into<String>,
    bytes: &[u8],
    media_type: Option<String>,
) -> ReleaseArtifact {
    ReleaseArtifact {
        location: location.into(),
        digest: sha256(bytes),
        media_type,
    }
}

fn canonical_digest(release: &CommunityRelease) -> CommunityResult<String> {
    let mut unsigned = release.clone();
    unsigned.digest.clear();
    unsigned.signatures.clear();
    Ok(sha256(&serde_json::to_vec(&unsigned)?))
}

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn validate_version(version: &str) -> CommunityResult<()> {
    let core = version.strip_prefix('v').unwrap_or(version);
    let parts: Vec<_> = core.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|part| part.parse::<u64>().is_err()) {
        return Err(CommunityError::Invalid(format!(
            "release version `{version}` must be MAJOR.MINOR.PATCH"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn manifest() -> WorkspaceManifest {
        let governance = GovernancePolicy {
            review_period_days: 0,
            ..GovernancePolicy::default()
        };
        WorkspaceManifest {
            manifest_version: 1,
            community: CommunityIdentity {
                name: "Community".into(),
                did: "did:plc:community".into(),
                description: "test".into(),
                record: None,
            },
            packages: vec![],
            authorities: vec![],
            governance,
            resources: ResourcePolicy::default(),
            federation: vec![],
            release: ReleasePolicy::default(),
            exit: ExitPolicy::default(),
        }
    }

    fn change() -> ChangePacket {
        ChangePacket {
            format_version: 1,
            id: "change-1".into(),
            community: "did:plc:community".into(),
            title: "Change".into(),
            summary: "Summary".into(),
            author: "did:plc:alice".into(),
            status: ChangeStatus::Approved,
            source: SchemaEndpoint {
                protocol: "atproto".into(),
                path: "old".into(),
                digest: "sha256:old".into(),
            },
            target: SchemaEndpoint {
                protocol: "atproto".into(),
                path: "new".into(),
                digest: "sha256:new".into(),
            },
            consequences: ConsequenceReport {
                compatibility: Compatibility::FullyCompatible,
                optic_class: Some(OpticClass::Iso),
                existing_records_readable: true,
                old_readers_accept_new_records: true,
                requires_complement: false,
                requires_manual_chain: false,
                data_loss_risk: false,
                changes: vec![],
                messages: vec![],
            },
            affected: vec![],
            rollback: "Restore old schema".into(),
            reviews: vec![],
            decision: GovernanceDecision {
                status: DecisionStatus::Approved,
                reason: "approved".into(),
                deliberation: None,
            },
            verifications: vec![VerificationEvidence {
                kind: "schema-compatibility".into(),
                outcome: VerificationOutcome::Verified,
                tool: "idiolect".into(),
                evidence: None,
                verified_at: "2026-09-18T00:00:00Z".into(),
            }],
            created_at: "2026-09-17T00:00:00Z".into(),
            updated_at: "2026-09-18T00:00:00Z".into(),
        }
    }

    #[test]
    fn signed_release_verifies_and_detects_tampering() {
        let key = generate_signing_key();
        let mut release = create_release(&manifest(), "1.2.3", &[change()], vec![]).unwrap();
        sign_release(&mut release, "did:plc:alice", &key).unwrap();
        assert_eq!(verify_release(&release).unwrap(), 1);
        release.version = "1.2.4".into();
        assert!(verify_release(&release).is_err());
    }
}
