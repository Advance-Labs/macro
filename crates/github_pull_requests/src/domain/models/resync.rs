//! Identity evidence and conditional persistence for legacy PR repair.

use std::collections::BTreeSet;

use foreign_entity::domain::models::ForeignEntity;
use serde::Serialize;
use uuid::Uuid;

use super::EnrichedGithubPullRequest;

/// Maximum source records accepted for one shared PR key.
pub const RESYNC_SOURCE_LIMIT: usize = 128;

/// Complete source snapshots read before fetching GitHub metadata.
#[derive(Debug, Clone)]
pub struct PullRequestResyncCandidate {
    /// Case-insensitive shared key.
    pub github_key: String,
    /// Existing source snapshots, including every casing variant.
    pub sources: Vec<ForeignEntity>,
    /// Whether a typed row already exists under this key.
    pub already_present: bool,
}

/// Resource-specific IDs read from the exact live PR.
#[derive(Debug, Clone, Default)]
pub struct PullRequestIdentityEvidence {
    /// Issue comment IDs, never compared with review comment IDs.
    pub issue_comment_ids: BTreeSet<u64>,
    /// Inline review comment IDs.
    pub review_comment_ids: BTreeSet<u64>,
    /// Submitted review IDs.
    pub review_ids: BTreeSet<u64>,
}

/// Explicit operator assertion covering exact historical source records.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovedPullRequestMapping {
    /// Original case-insensitive PR key, including its number.
    pub github_key: String,
    /// Verified positive target repository ID.
    pub repository_id: u64,
    /// Exact source UUIDs whose original repository identity the operator verified.
    pub source_ids: Vec<Uuid>,
}
/// Fresh metadata and independently fetched identity evidence.
#[derive(Debug, Clone)]
pub struct PullRequestResyncSnapshot {
    /// Current metadata from GitHub; reviews must be complete.
    pub pull_request: EnrichedGithubPullRequest,
    /// Evidence for sources without repository IDs.
    pub evidence: PullRequestIdentityEvidence,
    /// Explicit approved binding; GitHub readers always leave this absent.
    pub approved_mapping: Option<ApprovedPullRequestMapping>,
}

/// The evidence used to bind one source to a live repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestSourceProof {
    /// Source UUID covered by this proof.
    pub source_id: Uuid,
    /// Resource type, or stored repository identity.
    pub kind: PullRequestProofKind,
    /// Matching GitHub resource ID, absent for stored repository identity.
    pub resource_id: Option<u64>,
}

/// Accepted forms of historical repository identity evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PullRequestProofKind {
    /// Positive matching repository ID already stored on this source.
    RepositoryId,
    /// Issue comment ID belonging to the fetched PR.
    IssueComment,
    /// Inline review comment ID belonging to the fetched PR.
    ReviewComment,
    /// Submitted review ID belonging to the fetched PR.
    Review,
    /// An explicit operator-approved binding of the exact source UUID.
    ApprovedMapping,
}

/// Terminal and retryable outcomes for one shared PR key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PullRequestResyncOutcome {
    /// Dry-run verified the complete source group and insertion preconditions.
    Ready,
    /// A missing typed row was initialized.
    Inserted,
    /// An existing typed row was left untouched.
    AlreadyPresent,
    /// At least one source lacks historical identity evidence.
    Unverified,
    /// A stored or typed identity contradicts the live identity.
    IdentityConflict,
    /// A source or group membership changed; retry from new snapshots.
    SourceChanged,
    /// A malformed key, metadata identity, or oversized group was rejected.
    Invalid,
    /// GitHub or current installation associations cannot supply this PR.
    Unavailable,
    /// A transient external error requires retrying the original page.
    RetryableFailure,
    /// GitHub requires a pause before retrying the original page.
    RateLimited,
}

/// An individual source whose identity validation failed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestSourceRejection {
    /// Exact source UUID requiring investigation.
    pub source_id: Uuid,
    /// Validation failure for this source or its complete group.
    pub outcome: PullRequestResyncOutcome,
}

/// Auditable result without source metadata or credentials.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestResyncResult {
    /// Shared key processed.
    pub github_key: String,
    /// What happened to this source group.
    pub outcome: PullRequestResyncOutcome,
    /// Evidence for individually verified sources, including on partial failure.
    pub proofs: Vec<PullRequestSourceProof>,
    /// Every source UUID considered, including rejected and skipped sources.
    pub source_ids: Vec<Uuid>,
    /// Specific identity validation failures; verified sibling proofs remain available.
    pub rejections: Vec<PullRequestSourceRejection>,
}
