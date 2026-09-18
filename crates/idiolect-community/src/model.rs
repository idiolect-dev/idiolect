//! Serializable community workspace and lifecycle models.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Current on-disk workspace format.
pub const MANIFEST_VERSION: u32 = 1;

/// Top-level `idiolect.toml` document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceManifest {
    /// Manifest format version.
    #[serde(default = "default_manifest_version")]
    pub manifest_version: u32,
    /// Community identity and description.
    pub community: CommunityIdentity,
    /// Protocol packages governed in this workspace.
    #[serde(default)]
    pub packages: Vec<PackageSpec>,
    /// Parties authorized to review or publish changes.
    #[serde(default)]
    pub authorities: Vec<Authority>,
    /// Decision rule for proposed changes.
    #[serde(default)]
    pub governance: GovernancePolicy,
    /// Limits passed to schema and verification operations.
    #[serde(default)]
    pub resources: ResourcePolicy,
    /// Relationships to other community workspaces.
    #[serde(default)]
    pub federation: Vec<FederationDependency>,
    /// Rules for constructing releases.
    #[serde(default)]
    pub release: ReleasePolicy,
    /// Files included in a portable exit export.
    #[serde(default)]
    pub exit: ExitPolicy,
}

const fn default_manifest_version() -> u32 {
    MANIFEST_VERSION
}

/// Human and cryptographic identity of a community.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityIdentity {
    /// Human-readable name.
    pub name: String,
    /// DID controlled by the community or its delegated service.
    pub did: String,
    /// Plain-language purpose and scope.
    pub description: String,
    /// Optional published `dev.idiolect.community` record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<String>,
}

/// One schema or vocabulary package governed by a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageSpec {
    /// Stable local package name.
    pub name: String,
    /// Path relative to the manifest.
    pub path: String,
    /// Panproto protocol registry key.
    #[serde(default = "default_protocol")]
    pub protocol: String,
    /// Whether documents in this package may reference one another.
    #[serde(default)]
    pub cross_document: bool,
}

fn default_protocol() -> String {
    "atproto".to_owned()
}

/// An authorized participant and the roles they may exercise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Authority {
    /// Participant DID.
    pub did: String,
    /// Roles such as `member`, `maintainer`, or `steward`.
    #[serde(default)]
    pub roles: Vec<String>,
}

/// Supported governance decision models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum GovernanceModel {
    /// A minimum number of maintainers must approve.
    #[default]
    Maintainer,
    /// Any objection holds the proposal; the configured quorum must consent.
    Consent,
    /// A quorum votes and the approval ratio must reach the threshold.
    Vote,
    /// At least one steward must approve.
    Steward,
    /// Maintainer threshold plus explicit steward approval.
    Hybrid,
}

/// Configurable decision rule for community changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernancePolicy {
    /// Decision model.
    #[serde(default)]
    pub model: GovernanceModel,
    /// Minimum number of approving reviews.
    #[serde(default = "default_one")]
    pub min_approvals: u32,
    /// Minimum number of participating reviewers.
    #[serde(default = "default_one")]
    pub quorum: u32,
    /// Approval ratio for `vote`, in the inclusive range 0–1.
    #[serde(default = "default_threshold")]
    pub approval_threshold: f64,
    /// Minimum public review period before release.
    #[serde(default)]
    pub review_period_days: u32,
    /// Consequence flags that require a steward review.
    #[serde(default = "default_steward_triggers")]
    pub steward_review_for: Vec<String>,
    /// Verification kinds required before release.
    #[serde(default)]
    pub required_verifications: Vec<String>,
}

impl Default for GovernancePolicy {
    fn default() -> Self {
        Self {
            model: GovernanceModel::Maintainer,
            min_approvals: 1,
            quorum: 1,
            approval_threshold: 0.5,
            review_period_days: 3,
            steward_review_for: default_steward_triggers(),
            required_verifications: vec!["schema-compatibility".to_owned()],
        }
    }
}

const fn default_one() -> u32 {
    1
}

const fn default_threshold() -> f64 {
    0.5
}

fn default_steward_triggers() -> Vec<String> {
    vec!["breaking".to_owned(), "data-loss".to_owned()]
}

/// Shared resource limits for untrusted community input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePolicy {
    /// Maximum number of schema documents in one operation.
    #[serde(default = "default_documents")]
    pub max_documents: u32,
    /// Maximum total schema bytes in one operation.
    #[serde(default = "default_schema_bytes")]
    pub max_schema_bytes: u64,
    /// Maximum migration-search work units.
    #[serde(default = "default_search_steps")]
    pub max_search_steps: u64,
    /// Maximum verification cases in one run.
    #[serde(default = "default_verification_cases")]
    pub max_verification_cases: u64,
}

impl Default for ResourcePolicy {
    fn default() -> Self {
        Self {
            max_documents: default_documents(),
            max_schema_bytes: default_schema_bytes(),
            max_search_steps: default_search_steps(),
            max_verification_cases: default_verification_cases(),
        }
    }
}

const fn default_documents() -> u32 {
    256
}
const fn default_schema_bytes() -> u64 {
    16 * 1024 * 1024
}
const fn default_search_steps() -> u64 {
    100_000
}
const fn default_verification_cases() -> u64 {
    10_000
}

/// How a workspace relates to an upstream or peer community.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FederationDependency {
    /// Published community record or DID.
    pub community: String,
    /// Relationship: `follows`, `extends`, `bridges`, or `forked-from`.
    pub relation: String,
    /// Release requirement or exact release identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    /// Update policy: `review`, `follow-compatible`, or `pinned`.
    #[serde(default = "default_update_policy")]
    pub update_policy: String,
    /// Published mapping or lens records bridging the communities.
    #[serde(default)]
    pub mappings: Vec<String>,
}

fn default_update_policy() -> String {
    "review".to_owned()
}

/// Release construction and signing rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleasePolicy {
    /// Relative directory for release bundles.
    #[serde(default = "default_release_dir")]
    pub directory: String,
    /// Minimum number of valid detached signatures.
    #[serde(default = "default_one")]
    pub min_signatures: u32,
    /// Optional PDS or registry publication target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication_target: Option<String>,
}

impl Default for ReleasePolicy {
    fn default() -> Self {
        Self {
            directory: default_release_dir(),
            min_signatures: 1,
            publication_target: None,
        }
    }
}

fn default_release_dir() -> String {
    ".idiolect/releases".to_owned()
}

/// Portable export policy.
// These independent inclusions intentionally remain explicit manifest flags.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitPolicy {
    /// Include change and governance history.
    #[serde(default = "default_true")]
    pub include_history: bool,
    /// Include schemas and vocabularies.
    #[serde(default = "default_true")]
    pub include_packages: bool,
    /// Include migration-run evidence.
    #[serde(default = "default_true")]
    pub include_migrations: bool,
    /// Include release bundles and signatures.
    #[serde(default = "default_true")]
    pub include_releases: bool,
}

impl Default for ExitPolicy {
    fn default() -> Self {
        Self {
            include_history: true,
            include_packages: true,
            include_migrations: true,
            include_releases: true,
        }
    }
}

const fn default_true() -> bool {
    true
}

/// Lifecycle state of a community change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeStatus {
    /// Author is still preparing the packet.
    #[default]
    Draft,
    /// Packet is open for review.
    Review,
    /// Governance requirements were satisfied.
    Approved,
    /// Governance rejected the change.
    Rejected,
    /// A release contains the change.
    Released,
    /// A newer proposal replaces this one.
    Superseded,
}

/// A schema endpoint used in a change analysis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaEndpoint {
    /// Panproto protocol registry key.
    pub protocol: String,
    /// Path supplied to the analyser.
    pub path: String,
    /// SHA-256 digest of source bytes.
    pub digest: String,
}

/// Compatibility class exposed to community tooling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Compatibility {
    /// No compatibility-relevant changes.
    FullyCompatible,
    /// Existing records remain accepted by the new schema.
    BackwardCompatible,
    /// Some existing records or consumers may break.
    Breaking,
}

/// Panproto optic classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OpticClass {
    /// Bidirectional isomorphism.
    Iso,
    /// Projection that needs complement data for faithful reversal.
    Lens,
    /// Injection into a larger variant space.
    Prism,
    /// Partial lens/prism composition.
    Affine,
    /// Multi-focus transformation.
    Traversal,
}

/// One machine-detected schema change and its consequence level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeImpact {
    /// Stable human-readable change description.
    pub description: String,
    /// Whether Panproto classified it as breaking.
    pub breaking: bool,
}

/// Human-oriented interpretation of formal compatibility evidence.
// Each flag names an independent participant-facing consequence.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsequenceReport {
    /// Forward compatibility classification.
    pub compatibility: Compatibility,
    /// Automatically derived optic class, when a chain was found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optic_class: Option<OpticClass>,
    /// Existing records are accepted by the target schema.
    pub existing_records_readable: bool,
    /// Records written only to the target schema are accepted by the source.
    pub old_readers_accept_new_records: bool,
    /// Reverse translation requires retained complement data.
    pub requires_complement: bool,
    /// A migration chain must be authored or reviewed manually.
    pub requires_manual_chain: bool,
    /// The change may drop or hide information without complement retention.
    pub data_loss_risk: bool,
    /// Structured changes from the compatibility engine.
    pub changes: Vec<ChangeImpact>,
    /// Plain-language conclusions for non-specialist reviewers.
    pub messages: Vec<String>,
}

/// Review stance on a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewStance {
    /// Accept the proposed change.
    Approve,
    /// Object to the proposed change.
    Reject,
    /// Participate without choosing approve or reject.
    Abstain,
}

/// One attributable governance review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    /// Reviewer DID.
    pub reviewer: String,
    /// Role exercised for this review.
    pub role: String,
    /// Review stance.
    pub stance: ReviewStance,
    /// Optional rationale or condition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// RFC-3339 review time.
    pub reviewed_at: String,
}

/// Evaluated governance outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DecisionStatus {
    /// More participation or evidence is needed.
    #[default]
    Pending,
    /// Policy accepted the change.
    Approved,
    /// Policy rejected the change.
    Rejected,
}

/// Current decision and an explanation suitable for UI display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceDecision {
    /// Evaluated status.
    pub status: DecisionStatus,
    /// Plain-language explanation.
    pub reason: String,
    /// Optional published deliberation record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deliberation: Option<String>,
}

/// Three-state verification result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationOutcome {
    /// The stated property was established.
    Verified,
    /// A counterexample or violation was found.
    Refuted,
    /// The operation ended without a conclusion.
    Incomplete,
}

/// Verification evidence attached to a change packet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationEvidence {
    /// Verification kind, such as `schema-compatibility`.
    pub kind: String,
    /// Three-state outcome.
    pub outcome: VerificationOutcome,
    /// Tool and version that produced the result.
    pub tool: String,
    /// Optional evidence file or published record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    /// RFC-3339 verification time.
    pub verified_at: String,
}

/// A governed, reviewable unit of community change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangePacket {
    /// File-format version.
    pub format_version: u32,
    /// Workspace-local stable identifier.
    pub id: String,
    /// Community DID.
    pub community: String,
    /// Short proposal title.
    pub title: String,
    /// Plain-language intent.
    pub summary: String,
    /// Proposer DID.
    pub author: String,
    /// Lifecycle status.
    pub status: ChangeStatus,
    /// Source schema.
    pub source: SchemaEndpoint,
    /// Target schema.
    pub target: SchemaEndpoint,
    /// Formal evidence translated into consequences.
    pub consequences: ConsequenceReport,
    /// Known applications, collections, or groups affected.
    #[serde(default)]
    pub affected: Vec<String>,
    /// Rollback procedure or conditions.
    #[serde(default)]
    pub rollback: String,
    /// Reviews recorded under the workspace policy.
    #[serde(default)]
    pub reviews: Vec<Review>,
    /// Current evaluated decision.
    #[serde(default)]
    pub decision: GovernanceDecision,
    /// Verification evidence.
    #[serde(default)]
    pub verifications: Vec<VerificationEvidence>,
    /// RFC-3339 creation time.
    pub created_at: String,
    /// RFC-3339 last update time.
    pub updated_at: String,
}

/// State of a durable migration operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MigrationStatus {
    /// Run has not started.
    #[default]
    Planned,
    /// Records are being processed.
    Running,
    /// Operator paused the run.
    Paused,
    /// Every record was processed without unresolved failures.
    Completed,
    /// One or more unresolved failures stopped completion.
    Failed,
    /// Changes were reversed.
    RolledBack,
}

/// One resumable migration checkpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationCheckpoint {
    /// Opaque cursor supplied by the data source.
    pub cursor: String,
    /// Number of records processed at this cursor.
    pub processed: u64,
    /// RFC-3339 checkpoint time.
    pub recorded_at: String,
}

/// One failed record sample.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationFailure {
    /// Record identifier or source path.
    pub record: String,
    /// Stable failure message.
    pub reason: String,
}

/// Durable state for an operational migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationRun {
    /// Run identifier.
    pub id: String,
    /// Change packet identifier.
    pub change: String,
    /// Optional published lens record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lens: Option<String>,
    /// Current state.
    pub status: MigrationStatus,
    /// Expected number of records, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Successfully processed records.
    pub processed: u64,
    /// Failed records.
    pub failed: u64,
    /// Resume checkpoints.
    #[serde(default)]
    pub checkpoints: Vec<MigrationCheckpoint>,
    /// Bounded failure samples.
    #[serde(default)]
    pub failures: Vec<MigrationFailure>,
    /// RFC-3339 creation time.
    pub created_at: String,
    /// RFC-3339 last update time.
    pub updated_at: String,
}

/// One file or record included in a release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseArtifact {
    /// Relative path or published URI.
    pub location: String,
    /// SHA-256 content digest.
    pub digest: String,
    /// Media type when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
}

/// Detached signature over a release bundle's canonical digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseSignature {
    /// Signer DID.
    pub signer: String,
    /// Signature algorithm (`ES256`).
    pub algorithm: String,
    /// Base64url SEC1 encoded public key.
    pub public_key: String,
    /// Base64url fixed-width signature bytes.
    pub signature: String,
    /// RFC-3339 signing time.
    pub signed_at: String,
}

/// Immutable, signed community release bundle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityRelease {
    /// Bundle format version.
    pub format_version: u32,
    /// Community DID.
    pub community: String,
    /// Community release version.
    pub version: String,
    /// SHA-256 digest of the manifest snapshot.
    pub manifest_digest: String,
    /// Manifest captured at release time.
    pub manifest: WorkspaceManifest,
    /// Approved change packets in the release.
    pub changes: Vec<ChangePacket>,
    /// Additional artifacts and their digests.
    #[serde(default)]
    pub artifacts: Vec<ReleaseArtifact>,
    /// Upstream community release selections.
    #[serde(default)]
    pub dependencies: Vec<FederationDependency>,
    /// SHA-256 digest of canonical unsigned bundle bytes.
    pub digest: String,
    /// Detached signatures.
    #[serde(default)]
    pub signatures: Vec<ReleaseSignature>,
    /// RFC-3339 release creation time.
    pub created_at: String,
}

/// Severity of a workspace diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticSeverity {
    /// Operation cannot proceed safely.
    Error,
    /// Configuration is valid but likely incomplete.
    Warning,
    /// Useful non-blocking information.
    Info,
}

/// One actionable workspace diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    /// Diagnostic severity.
    pub severity: DiagnosticSeverity,
    /// Stable machine-readable code.
    pub code: String,
    /// Plain-language explanation.
    pub message: String,
    /// Suggested next action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

/// Result of `doctor` and `check`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    /// Whether blocking errors were absent.
    pub healthy: bool,
    /// Diagnostics ordered by discovery.
    pub diagnostics: Vec<Diagnostic>,
    /// Counts of local artifacts by kind.
    #[serde(default)]
    pub inventory: BTreeMap<String, u64>,
}
