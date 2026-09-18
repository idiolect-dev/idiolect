//! Configurable community governance and verification gates.

use std::collections::BTreeSet;

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::error::{CommunityError, CommunityResult};
use crate::model::{
    ChangePacket, ChangeStatus, Compatibility, DecisionStatus, GovernanceDecision, GovernanceModel,
    Review, ReviewStance, VerificationEvidence, VerificationOutcome, WorkspaceManifest,
};

/// Record or replace one review, then re-evaluate the packet under the
/// workspace's current governance policy.
///
/// # Errors
///
/// Returns an error when the reviewer is not an authority, does not hold the
/// claimed role, or the packet timestamp cannot be updated.
pub fn record_review(
    packet: &mut ChangePacket,
    manifest: &WorkspaceManifest,
    review: Review,
) -> CommunityResult<GovernanceDecision> {
    let authority = manifest
        .authorities
        .iter()
        .find(|candidate| candidate.did == review.reviewer)
        .ok_or_else(|| {
            CommunityError::Governance(format!("{} is not listed in authorities", review.reviewer))
        })?;
    if !authority.roles.iter().any(|role| role == &review.role) {
        return Err(CommunityError::Governance(format!(
            "{} does not hold role `{}`",
            review.reviewer, review.role
        )));
    }
    if let Some(existing) = packet
        .reviews
        .iter_mut()
        .find(|item| item.reviewer == review.reviewer && item.role == review.role)
    {
        *existing = review;
    } else {
        packet.reviews.push(review);
    }
    packet.updated_at = now()?;
    packet.decision = evaluate_governance(packet, manifest);
    packet.status = match packet.decision.status {
        DecisionStatus::Approved => ChangeStatus::Approved,
        DecisionStatus::Rejected => ChangeStatus::Rejected,
        DecisionStatus::Pending => ChangeStatus::Review,
    };
    Ok(packet.decision.clone())
}

/// Attach three-state verification evidence, replacing an earlier result from
/// the same tool for the same verification kind.
///
/// # Errors
///
/// Returns an error when the packet timestamp cannot be updated.
pub fn record_verification(
    packet: &mut ChangePacket,
    evidence: VerificationEvidence,
) -> CommunityResult<()> {
    if let Some(existing) = packet
        .verifications
        .iter_mut()
        .find(|item| item.kind == evidence.kind && item.tool == evidence.tool)
    {
        *existing = evidence;
    } else {
        packet.verifications.push(evidence);
    }
    packet.updated_at = now()?;
    Ok(())
}

/// Evaluate reviews without mutating the packet.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn evaluate_governance(
    packet: &ChangePacket,
    manifest: &WorkspaceManifest,
) -> GovernanceDecision {
    let policy = &manifest.governance;
    let approvals = count_u32(
        packet
            .reviews
            .iter()
            .filter(|r| r.stance == ReviewStance::Approve),
    );
    let rejections = count_u32(
        packet
            .reviews
            .iter()
            .filter(|r| r.stance == ReviewStance::Reject),
    );
    let participants = u32::try_from(
        packet
            .reviews
            .iter()
            .map(|r| r.reviewer.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
    )
    .unwrap_or(u32::MAX);
    let maintainer_approvals = count_u32(
        packet
            .reviews
            .iter()
            .filter(|r| r.stance == ReviewStance::Approve && r.role == "maintainer"),
    );
    let steward_approval = packet
        .reviews
        .iter()
        .any(|r| r.stance == ReviewStance::Approve && r.role == "steward");

    if participants < policy.quorum {
        return decision(
            DecisionStatus::Pending,
            format!(
                "{} of {} required participants have reviewed",
                participants, policy.quorum
            ),
        );
    }

    let steward_required = packet.consequences.data_loss_risk
        && policy
            .steward_review_for
            .iter()
            .any(|trigger| trigger == "data-loss")
        || packet.consequences.compatibility == Compatibility::Breaking
            && policy
                .steward_review_for
                .iter()
                .any(|trigger| trigger == "breaking");
    if steward_required && !steward_approval {
        return decision(
            DecisionStatus::Pending,
            "this change needs an approving steward because it is breaking or risks data loss",
        );
    }

    match policy.model {
        GovernanceModel::Maintainer => {
            if maintainer_approvals >= policy.min_approvals {
                decision(
                    DecisionStatus::Approved,
                    format!(
                        "maintainer threshold satisfied ({maintainer_approvals}/{})",
                        policy.min_approvals
                    ),
                )
            } else if rejections > 0 {
                decision(
                    DecisionStatus::Rejected,
                    "a reviewer rejected the proposal before the maintainer threshold was met",
                )
            } else {
                decision(
                    DecisionStatus::Pending,
                    format!(
                        "{} more maintainer approval(s) required",
                        policy.min_approvals.saturating_sub(maintainer_approvals)
                    ),
                )
            }
        }
        GovernanceModel::Consent => {
            if rejections > 0 {
                decision(
                    DecisionStatus::Rejected,
                    "consent is held by an explicit objection",
                )
            } else if approvals >= policy.min_approvals {
                decision(
                    DecisionStatus::Approved,
                    format!("consent threshold satisfied ({approvals} approvals)"),
                )
            } else {
                decision(
                    DecisionStatus::Pending,
                    format!(
                        "{} more expression(s) of consent required",
                        policy.min_approvals.saturating_sub(approvals)
                    ),
                )
            }
        }
        GovernanceModel::Vote => {
            let decisive = approvals + rejections;
            if decisive == 0 {
                return decision(
                    DecisionStatus::Pending,
                    "no approving or rejecting votes have been cast",
                );
            }
            let ratio = f64::from(approvals) / f64::from(decisive);
            if ratio >= policy.approval_threshold && approvals >= policy.min_approvals {
                decision(
                    DecisionStatus::Approved,
                    format!("approval ratio {ratio:.3} meets the configured threshold"),
                )
            } else {
                decision(
                    DecisionStatus::Rejected,
                    format!("approval ratio {ratio:.3} is below the configured threshold"),
                )
            }
        }
        GovernanceModel::Steward => {
            if steward_approval {
                decision(DecisionStatus::Approved, "an authorized steward approved")
            } else {
                decision(
                    DecisionStatus::Pending,
                    "an approving steward review is required",
                )
            }
        }
        GovernanceModel::Hybrid => {
            if steward_approval && maintainer_approvals >= policy.min_approvals {
                decision(
                    DecisionStatus::Approved,
                    "maintainer threshold and steward approval are both satisfied",
                )
            } else {
                decision(
                    DecisionStatus::Pending,
                    "hybrid policy requires both the maintainer threshold and a steward approval",
                )
            }
        }
    }
}

/// Check whether all release-required verifications are conclusive and hold.
///
/// # Errors
///
/// Returns an error for refuted or incomplete evidence and when any required
/// verification kind lacks a verified result.
pub fn verification_gate(
    packet: &ChangePacket,
    manifest: &WorkspaceManifest,
) -> CommunityResult<()> {
    for evidence in &packet.verifications {
        match evidence.outcome {
            VerificationOutcome::Refuted => {
                return Err(CommunityError::Governance(format!(
                    "verification `{}` was refuted",
                    evidence.kind
                )));
            }
            VerificationOutcome::Incomplete => {
                return Err(CommunityError::Governance(format!(
                    "verification `{}` is incomplete, not passing",
                    evidence.kind
                )));
            }
            VerificationOutcome::Verified => {}
        }
    }
    for required in &manifest.governance.required_verifications {
        if !packet
            .verifications
            .iter()
            .any(|e| e.kind == *required && e.outcome == VerificationOutcome::Verified)
        {
            return Err(CommunityError::Governance(format!(
                "required verification `{required}` has not been established"
            )));
        }
    }
    Ok(())
}

fn decision(status: DecisionStatus, reason: impl Into<String>) -> GovernanceDecision {
    GovernanceDecision {
        status,
        reason: reason.into(),
        deliberation: None,
    }
}

fn count_u32<T>(items: impl Iterator<Item = T>) -> u32 {
    u32::try_from(items.count()).unwrap_or(u32::MAX)
}

pub(crate) fn now() -> CommunityResult<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| CommunityError::Invalid(format!("format current time: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn packet() -> ChangePacket {
        ChangePacket {
            format_version: 1,
            id: "change-1".into(),
            community: "did:plc:community".into(),
            title: "Rename member".into(),
            summary: "Use participant terminology".into(),
            author: "did:plc:alice".into(),
            status: ChangeStatus::Review,
            source: SchemaEndpoint {
                protocol: "atproto".into(),
                path: "old.json".into(),
                digest: "sha256:old".into(),
            },
            target: SchemaEndpoint {
                protocol: "atproto".into(),
                path: "new.json".into(),
                digest: "sha256:new".into(),
            },
            consequences: ConsequenceReport {
                compatibility: Compatibility::BackwardCompatible,
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
            rollback: String::new(),
            reviews: vec![],
            decision: GovernanceDecision::default(),
            verifications: vec![],
            created_at: "2026-09-18T00:00:00Z".into(),
            updated_at: "2026-09-18T00:00:00Z".into(),
        }
    }

    fn manifest() -> WorkspaceManifest {
        WorkspaceManifest {
            manifest_version: 1,
            community: CommunityIdentity {
                name: "Community".into(),
                did: "did:plc:community".into(),
                description: String::new(),
                record: None,
            },
            packages: vec![],
            authorities: vec![Authority {
                did: "did:plc:alice".into(),
                roles: vec!["maintainer".into()],
            }],
            governance: GovernancePolicy::default(),
            resources: ResourcePolicy::default(),
            federation: vec![],
            release: ReleasePolicy::default(),
            exit: ExitPolicy::default(),
        }
    }

    #[test]
    fn maintainer_review_approves_packet() {
        let mut packet = packet();
        let decision = record_review(
            &mut packet,
            &manifest(),
            Review {
                reviewer: "did:plc:alice".into(),
                role: "maintainer".into(),
                stance: ReviewStance::Approve,
                comment: None,
                reviewed_at: "2026-09-18T00:00:00Z".into(),
            },
        )
        .unwrap();
        assert_eq!(decision.status, DecisionStatus::Approved);
        assert_eq!(packet.status, ChangeStatus::Approved);
    }

    #[test]
    fn incomplete_evidence_never_passes() {
        let mut packet = packet();
        packet.verifications.push(VerificationEvidence {
            kind: "schema-compatibility".into(),
            outcome: VerificationOutcome::Incomplete,
            tool: "test".into(),
            evidence: None,
            verified_at: "2026-09-18T00:00:00Z".into(),
        });
        assert!(verification_gate(&packet, &manifest()).is_err());
    }
}
