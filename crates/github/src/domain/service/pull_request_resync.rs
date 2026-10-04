//! Explicit operator repair, separate from installation backfill and webhook sync.

use super::InstallationTokenConfig;
use crate::domain::{
    models::{
        AppJwt, EnrichedGithubPullRequest, GithubAppInstallationSource, GithubError,
        GithubInstallationAccessToken, GithubPullRequestFetchError, GithubPullRequestRef,
        GithubPullRequestSnapshot, PullRequestResyncPage, PullRequestResyncRequest, app_jwt,
    },
    ports::{GithubPullRequestClient, GithubPullRequestResync, GithubSyncClient, GithubSyncRepo},
};
use github_pull_requests::domain::{
    models::{
        ApprovedPullRequestMapping, PullRequestIdentityEvidence, PullRequestResyncCandidate,
        PullRequestResyncOutcome, PullRequestResyncResult, PullRequestResyncSnapshot,
        RepositorySlug,
    },
    ports::GithubPullRequestResyncStore,
};

#[cfg(test)]
mod test;

/// Coordinates authenticated live fetches with identity-scoped repair persistence.
pub struct PullRequestResyncService<Repo, Client, Store> {
    config: InstallationTokenConfig,
    repo: Repo,
    client: Client,
    store: Store,
}

impl<Repo, Client, Store> PullRequestResyncService<Repo, Client, Store> {
    /// Build a repair service using the same App credentials as ordinary sync.
    pub fn new(config: InstallationTokenConfig, repo: Repo, client: Client, store: Store) -> Self {
        Self {
            config,
            repo,
            client,
            store,
        }
    }
}

impl<Repo, Client, Store> GithubPullRequestResync for PullRequestResyncService<Repo, Client, Store>
where
    Repo: GithubSyncRepo,
    Client: GithubSyncClient + GithubPullRequestClient,
    Store: GithubPullRequestResyncStore,
{
    #[tracing::instrument(skip_all, err)]
    async fn resync_pull_requests(
        &self,
        request: PullRequestResyncRequest,
    ) -> Result<PullRequestResyncPage, GithubError> {
        let limit = validate_request(&request)?;
        let mut keys = self
            .store
            .resync_keys(request.after.as_deref(), limit + 1)
            .await
            .map_err(|error| GithubError::Internal(error.into()))?;
        let has_more = keys.len() > limit as usize;
        keys.truncate(limit as usize);
        let mut page = PullRequestResyncPage {
            dry_run: request.dry_run,
            results: Vec::new(),
            next_after: request.after.clone(),
            has_more,
            retry_original_page: false,
            retry_after_seconds: None,
        };
        let jwt = app_jwt(&self.config.client_id, &self.config.private_key_pem)?;
        for key in keys {
            let attempt = self.repair_key(&jwt, &request, &key).await?;
            if record_attempt(&mut page, key, attempt) {
                break;
            }
        }
        if !page.has_more {
            page.next_after = None;
        }
        Ok(page)
    }
}

impl<Repo, Client, Store> PullRequestResyncService<Repo, Client, Store>
where
    Repo: GithubSyncRepo,
    Client: GithubSyncClient + GithubPullRequestClient,
    Store: GithubPullRequestResyncStore,
{
    async fn repair_key(
        &self,
        jwt: &AppJwt,
        request: &PullRequestResyncRequest,
        key: &str,
    ) -> Result<ResyncAttempt, GithubError> {
        let candidate = self
            .store
            .resync_candidate(key)
            .await
            .map_err(|error| GithubError::Internal(error.into()))?;
        let source_ids = candidate.sources.iter().map(|source| source.id).collect();
        let mut attempt = self.repair_candidate(jwt, request, candidate).await?;
        attempt.result.source_ids = source_ids;
        Ok(attempt)
    }

    async fn repair_candidate(
        &self,
        jwt: &AppJwt,
        request: &PullRequestResyncRequest,
        candidate: PullRequestResyncCandidate,
    ) -> Result<ResyncAttempt, GithubError> {
        let key = &candidate.github_key;
        if candidate.already_present {
            return Ok(ResyncAttempt::outcome(
                key,
                PullRequestResyncOutcome::AlreadyPresent,
            ));
        }
        let Some((slug, number)) = parse_key(key) else {
            return Ok(ResyncAttempt::outcome(
                key,
                PullRequestResyncOutcome::Invalid,
            ));
        };
        let Some(token) = self.resolve_access(jwt, &slug, &candidate).await? else {
            return Ok(ResyncAttempt::outcome(
                key,
                PullRequestResyncOutcome::Unavailable,
            ));
        };
        let approved_mapping = request
            .approved_mappings
            .iter()
            .find(|mapping| mapping.github_key.eq_ignore_ascii_case(key))
            .cloned();
        let include_comment_ids = needs_comment_ids(&candidate, approved_mapping.as_ref());
        let snapshot = match self
            .client
            .fetch_pull_request(
                &token.token,
                &slug.owner,
                &slug.name,
                number,
                include_comment_ids,
            )
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => return Ok(ResyncAttempt::fetch_error(key, error)),
        };
        // The GitHub client returns facts only. Historical binding belongs to repair policy.
        let snapshot = repair_snapshot(&slug, number, snapshot, approved_mapping);
        let result = self
            .store
            .resync_verified_snapshot(candidate, snapshot, request.dry_run)
            .await
            .map_err(|error| GithubError::Internal(error.into()))?;
        Ok(ResyncAttempt {
            result,
            retry_after_seconds: None,
        })
    }

    async fn resolve_access(
        &self,
        jwt: &AppJwt,
        slug: &RepositorySlug,
        candidate: &PullRequestResyncCandidate,
    ) -> Result<Option<GithubInstallationAccessToken>, GithubError> {
        let Some(installation) = self
            .client
            .get_repository_installation(jwt, &slug.owner, &slug.name)
            .await?
        else {
            return Ok(None);
        };
        let sources = self
            .repo
            .get_installation_sources(&installation.to_string())
            .await
            .map_err(|error| GithubError::Internal(error.into()))?;
        if !installation_covers_sources(candidate, &sources) {
            return Ok(None);
        }
        self.client
            .generate_scoped_installation_access_token(
                jwt,
                installation,
                &slug.name,
                &[("metadata", "read"), ("pull_requests", "read")],
            )
            .await
            .map(Some)
    }
}

fn record_attempt(page: &mut PullRequestResyncPage, key: String, attempt: ResyncAttempt) -> bool {
    let retry = matches!(
        attempt.result.outcome,
        PullRequestResyncOutcome::RateLimited
            | PullRequestResyncOutcome::RetryableFailure
            | PullRequestResyncOutcome::SourceChanged
    );
    page.results.push(attempt.result);
    if retry {
        page.retry_original_page = true;
        page.has_more = true;
        page.retry_after_seconds = attempt.retry_after_seconds;
        return true;
    }
    page.next_after = Some(key);
    false
}

struct ResyncAttempt {
    result: PullRequestResyncResult,
    retry_after_seconds: Option<u64>,
}

impl ResyncAttempt {
    fn outcome(key: &str, outcome: PullRequestResyncOutcome) -> Self {
        Self {
            result: PullRequestResyncResult {
                github_key: key.to_string(),
                outcome,
                proofs: Vec::new(),
                source_ids: Vec::new(),
                rejections: Vec::new(),
            },
            retry_after_seconds: None,
        }
    }

    fn fetch_error(key: &str, error: GithubPullRequestFetchError) -> Self {
        use GithubPullRequestFetchError::{Incomplete, RateLimited, Retryable, Unavailable};
        let (outcome, retry_after_seconds) = match error {
            Unavailable => (PullRequestResyncOutcome::Unavailable, None),
            Incomplete => (PullRequestResyncOutcome::Invalid, None),
            Retryable => (PullRequestResyncOutcome::RetryableFailure, None),
            RateLimited {
                retry_after_seconds,
            } => (
                PullRequestResyncOutcome::RateLimited,
                Some(retry_after_seconds),
            ),
        };
        Self {
            retry_after_seconds,
            ..Self::outcome(key, outcome)
        }
    }
}

fn installation_covers_sources(
    candidate: &PullRequestResyncCandidate,
    sources: &[GithubAppInstallationSource],
) -> bool {
    !candidate.sources.is_empty()
        && candidate.sources.iter().all(|record| {
            sources.iter().any(|source| match source {
                GithubAppInstallationSource::User(id) => {
                    record.stored_for_auth_entity == "user" && &record.stored_for_id == id
                }
                GithubAppInstallationSource::Team(id) => {
                    record.stored_for_auth_entity == "team"
                        && record.stored_for_id == id.to_string()
                }
            })
        })
}

fn needs_comment_ids(
    candidate: &PullRequestResyncCandidate,
    approved_mapping: Option<&ApprovedPullRequestMapping>,
) -> bool {
    candidate.sources.iter().any(|source| {
        if source
            .metadata
            .get("repositoryId")
            .is_some_and(|value| !value.is_null())
        {
            return false;
        }
        if approved_mapping.is_some_and(|mapping| mapping.source_ids.contains(&source.id)) {
            return false;
        }
        source
            .metadata
            .get("comments")
            .and_then(|value| value.as_array())
            .is_some_and(|comments| comments.iter().any(is_identity_comment))
    })
}

fn is_identity_comment(comment: &serde_json::Value) -> bool {
    matches!(
        comment.get("source").and_then(|value| value.as_str()),
        Some("issue_comment" | "review_comment")
    ) && comment
        .get("id")
        .and_then(|value| value.as_u64())
        .is_some_and(|id| id > 0)
}

fn repair_snapshot(
    slug: &RepositorySlug,
    number: u64,
    snapshot: GithubPullRequestSnapshot,
    approved_mapping: Option<ApprovedPullRequestMapping>,
) -> PullRequestResyncSnapshot {
    let reference = GithubPullRequestRef {
        github_key: format!("{}/{}/pull/{number}", slug.owner, slug.name),
        owner: slug.owner.clone(),
        repo: slug.name.clone(),
        number,
        url: format!(
            "https://github.com/{}/{}/pull/{number}",
            slug.owner, slug.name
        ),
        display_name: format!("{}/{}#{number}", slug.owner, slug.name),
    };
    PullRequestResyncSnapshot {
        pull_request: EnrichedGithubPullRequest::from_details(reference, snapshot.details),
        evidence: PullRequestIdentityEvidence {
            issue_comment_ids: snapshot.resource_ids.issue_comment_ids,
            review_comment_ids: snapshot.resource_ids.review_comment_ids,
            review_ids: snapshot.resource_ids.review_ids,
        },
        approved_mapping,
    }
}

fn validate_request(request: &PullRequestResyncRequest) -> Result<u32, GithubError> {
    if request
        .after
        .as_ref()
        .is_some_and(|after| after.len() > 512 || after != &after.to_ascii_lowercase())
    {
        return Err(GithubError::InvalidPullRequestResyncRequest);
    }
    if !request.approved_mappings.is_empty() && !request.approve_mappings {
        return Err(GithubError::InvalidPullRequestResyncRequest);
    }
    if request.approved_mappings.len() > 5 {
        return Err(GithubError::InvalidPullRequestResyncRequest);
    }
    let mut keys = std::collections::HashSet::new();
    for mapping in &request.approved_mappings {
        if !valid_mapping(mapping) || !keys.insert(mapping.github_key.to_ascii_lowercase()) {
            return Err(GithubError::InvalidPullRequestResyncRequest);
        }
    }
    Ok(request.limit.unwrap_or(1).clamp(1, 5))
}

fn valid_mapping(mapping: &ApprovedPullRequestMapping) -> bool {
    if mapping.repository_id == 0 || i64::try_from(mapping.repository_id).is_err() {
        return false;
    }
    if parse_key(&mapping.github_key.to_ascii_lowercase()).is_none()
        || mapping.source_ids.is_empty()
        || mapping.source_ids.len() > 128
    {
        return false;
    }
    let unique_ids = mapping
        .source_ids
        .iter()
        .collect::<std::collections::HashSet<_>>();
    unique_ids.len() == mapping.source_ids.len()
}

pub(crate) fn parse_key(key: &str) -> Option<(RepositorySlug, u64)> {
    let (repository, number) = key.split_once("/pull/")?;
    let slug = RepositorySlug::parse(repository)?;
    let number = number
        .parse::<u64>()
        .ok()
        .filter(|number| *number > 0 && i64::try_from(*number).is_ok())?;
    if key != format!("{repository}/pull/{number}") {
        return None;
    }
    Some((slug, number))
}
