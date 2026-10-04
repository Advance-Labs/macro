//! Bounded live snapshots, independent of repair verification and persistence.

use super::*;
use crate::domain::models::{
    GithubPullRequestFetchError, GithubPullRequestResourceIds, GithubPullRequestSnapshot,
};
use serde::de::DeserializeOwned;

const MAX_SNAPSHOT_PAGES: u32 = 10;

#[cfg(test)]
mod test;

pub(in crate::outbound) async fn fetch_snapshot(
    client: &reqwest::Client,
    api_base: &str,
    access_token: &str,
    owner: &str,
    repo: &str,
    number: u64,
    include_comment_ids: bool,
) -> Result<GithubPullRequestSnapshot, GithubPullRequestFetchError> {
    use GithubPullRequestFetchError::Incomplete;
    let core = fetch_pull_request_core(
        client,
        api_base,
        access_token,
        owner,
        repo,
        number,
        CoreReadPolicy::Snapshot,
    )
    .await
    .map_err(|error| {
        error
            .downcast::<GithubPullRequestFetchError>()
            .unwrap_or(GithubPullRequestFetchError::Retryable)
    })?;
    let prefix = format!("{api_base}/repos/{owner}/{repo}");
    let reviews: Vec<GithubReviewResponse> = pages(
        client,
        access_token,
        format!("{prefix}/pulls/{number}/reviews"),
    )
    .await?;
    let mut resource_ids = GithubPullRequestResourceIds::default();
    let mut participants = core.participant_ids();
    let mut decisions = Vec::new();
    for review in reviews {
        resource_ids.review_ids.insert(review.id);
        let Some(state) = review.state.as_deref() else {
            return Err(Incomplete);
        };
        if state == "PENDING" {
            continue;
        }
        let state = GithubPullRequestReviewState::from_github(state).ok_or(Incomplete)?;
        let user = review
            .user
            .as_ref()
            .and_then(|user| user.id)
            .ok_or(Incomplete)?;
        participants.insert(user);
        decisions.push(GithubPullRequestReview {
            reviewer_github_user_id: user.to_string(),
            reviewer_login: review.user.and_then(|user| user.login),
            state,
            submitted_at: review.submitted_at,
        });
    }
    if include_comment_ids {
        let issues: Vec<ResourceId> = pages(
            client,
            access_token,
            format!("{prefix}/issues/{number}/comments"),
        )
        .await?;
        let inline: Vec<ResourceId> = pages(
            client,
            access_token,
            format!("{prefix}/pulls/{number}/comments"),
        )
        .await?;
        resource_ids
            .issue_comment_ids
            .extend(issues.into_iter().map(|item| item.id));
        resource_ids
            .review_comment_ids
            .extend(inline.into_iter().map(|item| item.id));
    }
    let mut details = core.into_details();
    details.participant_github_user_ids =
        Some(participants.into_iter().map(|id| id.to_string()).collect());
    details.reviews = Some(latest_reviews(decisions));
    Ok(GithubPullRequestSnapshot {
        details,
        resource_ids,
    })
}

#[derive(serde::Deserialize)]
struct ResourceId {
    id: u64,
}

async fn pages<T: DeserializeOwned>(
    client: &reqwest::Client,
    token: &str,
    url: String,
) -> Result<Vec<T>, GithubPullRequestFetchError> {
    let mut items = Vec::new();
    for page in 1..=MAX_SNAPSHOT_PAGES {
        let batch: Vec<T> = get_json(
            client,
            token,
            format!("{url}?per_page={METADATA_PAGE_SIZE}&page={page}"),
        )
        .await?;
        let complete = batch.len() < usize::from(METADATA_PAGE_SIZE);
        items.extend(batch);
        if complete {
            return Ok(items);
        }
    }
    Err(GithubPullRequestFetchError::Incomplete)
}

pub(super) async fn get_json<T: DeserializeOwned>(
    client: &reqwest::Client,
    token: &str,
    url: String,
) -> Result<T, GithubPullRequestFetchError> {
    use GithubPullRequestFetchError::{Incomplete, RateLimited, Retryable, Unavailable};
    let response = github_get(client, token, url.clone())
        .send()
        .await
        .map_err(|_| Retryable)?;
    if response.url().as_str() != url || response.status().is_redirection() {
        return Err(Incomplete);
    }
    let status = response.status().as_u16();
    let headers = response.headers();
    // Reserve capacity for normal sync instead of consuming the installation's last requests.
    if response.status().is_success()
        && headers
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .is_some_and(|remaining| remaining <= 100)
    {
        let delay = headers
            .get("x-ratelimit-reset")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<i64>().ok())
            .map(|reset| reset.saturating_sub(chrono::Utc::now().timestamp()).max(1) as u64)
            .unwrap_or(60);
        return Err(RateLimited {
            retry_after_seconds: delay,
        });
    }
    // Secondary rate limits can omit quota headers. An ambiguous 403 must pause the caller.
    if status == 429 || status == 403 {
        let delay = headers
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .or_else(|| {
                headers
                    .get("x-ratelimit-reset")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<i64>().ok())
                    .map(|reset| reset.saturating_sub(chrono::Utc::now().timestamp()).max(1) as u64)
            })
            .unwrap_or(60)
            .max(1);
        return Err(RateLimited {
            retry_after_seconds: delay,
        });
    }
    if status == 404 || status == 410 {
        return Err(Unavailable);
    }
    if !response.status().is_success() {
        return Err(Retryable);
    }
    response.json().await.map_err(|_| Retryable)
}
