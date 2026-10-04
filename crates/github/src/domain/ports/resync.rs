//! Operator-facing repair orchestration.

use crate::domain::models::{GithubError, PullRequestResyncPage, PullRequestResyncRequest};
use std::future::Future;

/// Internal paginated repair, distinct from ordinary sync and indexing.
pub trait GithubPullRequestResync: Send + Sync + 'static {
    /// Fetch, verify, and optionally repair one page of stored PR references.
    fn resync_pull_requests(
        &self,
        request: PullRequestResyncRequest,
    ) -> impl Future<Output = Result<PullRequestResyncPage, GithubError>> + Send;
}
