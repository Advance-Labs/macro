//! Live GitHub pull-request reads, independent of repair and persistence policy.

use std::collections::BTreeSet;

use super::GithubPullRequestDetails;

/// Resource IDs belonging to one live pull request, separated by GitHub namespace.
#[derive(Debug, Clone, Default)]
pub struct GithubPullRequestResourceIds {
    /// Issue comment IDs, populated only when comment IDs were requested.
    pub issue_comment_ids: BTreeSet<u64>,
    /// Inline review comment IDs, populated only when comment IDs were requested.
    pub review_comment_ids: BTreeSet<u64>,
    /// Review IDs from the complete reviews response.
    pub review_ids: BTreeSet<u64>,
}

/// Fresh structural metadata and complete reviews, without persistence instructions.
#[derive(Debug, Clone)]
pub struct GithubPullRequestSnapshot {
    /// PR metadata; discussion bodies and checks are deliberately not fetched.
    pub details: GithubPullRequestDetails,
    /// Namespaced IDs for callers that need to identify GitHub resources.
    pub resource_ids: GithubPullRequestResourceIds,
}

/// External read failures, without GitHub response bodies or credentials.
#[derive(Debug, thiserror::Error)]
pub enum GithubPullRequestFetchError {
    /// The token cannot access the requested pull request.
    #[error("GitHub PR unavailable")]
    Unavailable,
    /// GitHub requires a delay before another read.
    #[error("GitHub rate limit")]
    RateLimited {
        /// Minimum advertised delay, with a conservative fallback.
        retry_after_seconds: u64,
    },
    /// The bounded response is incomplete or contradicts the requested identity.
    #[error("GitHub snapshot is incomplete or has inconsistent identity")]
    Incomplete,
    /// A transient request, authentication, or decoding failure.
    #[error("GitHub snapshot fetch failed")]
    Retryable,
}
