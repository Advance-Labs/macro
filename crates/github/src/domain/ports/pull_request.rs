//! Live GitHub reads, separate from historical verification and persistence.

use std::future::Future;

use crate::domain::models::{GithubPullRequestFetchError, GithubPullRequestSnapshot};

/// Fetch structural metadata and complete reviews using an authorized GitHub token.
pub trait GithubPullRequestClient: Send + Sync + 'static {
    /// Read one PR with bounded pagination and rate-limit reporting.
    /// Discussion bodies, checks, and patches are not included in the snapshot.
    fn fetch_pull_request(
        &self,
        access_token: &str,
        owner: &str,
        repo: &str,
        number: u64,
        include_comment_ids: bool,
    ) -> impl Future<Output = Result<GithubPullRequestSnapshot, GithubPullRequestFetchError>> + Send;
}
