//! Operator-facing, paginated legacy PR repair models.

use github_pull_requests::domain::models::PullRequestResyncResult;

/// One bounded page of stored PR keys, never an import of all GitHub history.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PullRequestResyncRequest {
    /// Exclusive normalized key cursor; absent starts the scan.
    pub after: Option<String>,
    /// Keys to attempt, clamped to one through five.
    pub limit: Option<u32>,
    /// No writes unless explicitly false.
    #[serde(default = "dry_run_default")]
    pub dry_run: bool,
    /// Explicit bindings for exact source UUIDs; never inferred from a repository name.
    #[serde(default)]
    pub approved_mappings: Vec<github_pull_requests::domain::models::ApprovedPullRequestMapping>,
    /// Explicit opt-in is required when approved_mappings is nonempty.
    #[serde(default)]
    pub approve_mappings: bool,
}

fn dry_run_default() -> bool {
    true
}

/// A repair page with explicit resumability and per-source proof reporting.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestResyncPage {
    /// Whether this page performed only validation.
    pub dry_run: bool,
    /// Individual PR results, without source content or credentials.
    pub results: Vec<PullRequestResyncResult>,
    /// Continue after this key only when retry_original_page is false.
    pub next_after: Option<String>,
    /// More keys remain, or processing stopped for a retryable failure.
    pub has_more: bool,
    /// Retain and retry the original request instead of advancing the cursor.
    pub retry_original_page: bool,
    /// Minimum GitHub retry delay when rate limiting interrupted processing.
    pub retry_after_seconds: Option<u64>,
}
