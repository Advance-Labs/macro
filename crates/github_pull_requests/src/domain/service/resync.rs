//! Repair policy: prove every sibling before changing any source metadata.

use foreign_entity::domain::ports::ForeignEntityMetadataService;

use super::{GithubPullRequestServiceImpl, repository_error};
use crate::domain::{
    models::{
        GITHUB_PULL_REQUEST_FOREIGN_ENTITY_SOURCE, GithubPullRequestError, GithubPullRequestRow,
        PullRequestIndexOutcome, PullRequestProofKind, PullRequestResyncCandidate,
        PullRequestResyncOutcome, PullRequestResyncResult, PullRequestResyncSnapshot,
        PullRequestSourceProof, RESYNC_SOURCE_LIMIT, RepositorySlug,
    },
    ports::{GithubPullRequestResyncRepository, GithubPullRequestResyncStore},
};

#[cfg(test)]
mod test;

impl<F, R> GithubPullRequestResyncStore for GithubPullRequestServiceImpl<F, R>
where
    F: ForeignEntityMetadataService,
    R: GithubPullRequestResyncRepository,
{
    #[tracing::instrument(skip(self), err)]
    async fn resync_keys(
        &self,
        after: Option<&str>,
        limit: u32,
    ) -> Result<Vec<String>, GithubPullRequestError> {
        self.repo
            .resync_keys(after, limit)
            .await
            .map_err(repository_error)
    }

    #[tracing::instrument(skip(self), err)]
    async fn resync_candidate(
        &self,
        key: &str,
    ) -> Result<PullRequestResyncCandidate, GithubPullRequestError> {
        self.repo
            .resync_candidate(key)
            .await
            .map_err(repository_error)
    }

    #[tracing::instrument(skip_all, err)]
    async fn resync_verified_snapshot(
        &self,
        candidate: PullRequestResyncCandidate,
        snapshot: PullRequestResyncSnapshot,
        dry_run: bool,
    ) -> Result<PullRequestResyncResult, GithubPullRequestError> {
        let mut result = PullRequestResyncResult {
            github_key: candidate.github_key.clone(),
            outcome: PullRequestResyncOutcome::AlreadyPresent,
            proofs: Vec::new(),
            source_ids: candidate.sources.iter().map(|source| source.id).collect(),
            rejections: Vec::new(),
        };
        if candidate.already_present {
            return Ok(result);
        }
        match verify_sources(&candidate, &snapshot) {
            Ok(proofs) => result.proofs = proofs,
            Err(outcome) => {
                result.outcome = outcome;
                for source in &candidate.sources {
                    let proof = validate_group(&candidate, &snapshot)
                        .and_then(|repository_id| source_proof(source, &snapshot, repository_id));
                    match proof {
                        Ok(proof) => result.proofs.push(proof),
                        Err(outcome) => result.rejections.push(
                            crate::domain::models::PullRequestSourceRejection {
                                source_id: source.id,
                                outcome,
                            },
                        ),
                    }
                }
                return Ok(result);
            }
        }
        let mut metadata = snapshot
            .pull_request
            .foreign_entity_metadata(None)
            .map_err(|error| GithubPullRequestError::Repository(error.into()))?;
        // Discussion is omitted deliberately. Retain known participants only after every
        // historical source identity is proved, matching ordinary sync's participant union.
        let participants = candidate
            .sources
            .iter()
            .map(|source| &source.metadata)
            .chain(std::iter::once(&metadata))
            .filter_map(|metadata| {
                metadata
                    .get("participantGithubUserIds")
                    .and_then(|value| value.as_array())
            })
            .flatten()
            .filter_map(|value| value.as_str())
            .map(str::to_string)
            .collect::<std::collections::BTreeSet<_>>();
        metadata["participantGithubUserIds"] = serde_json::json!(participants);
        let Some(row) = GithubPullRequestRow::from_metadata(&metadata) else {
            result.outcome = PullRequestResyncOutcome::Invalid;
            return Ok(result);
        };
        if let Some(outcome) = self
            .repo
            .inspect_resync_row(&row)
            .await
            .map_err(repository_error)?
        {
            result.outcome = match outcome {
                PullRequestIndexOutcome::AlreadyPresent => PullRequestResyncOutcome::AlreadyPresent,
                PullRequestIndexOutcome::IdentityConflict => {
                    PullRequestResyncOutcome::IdentityConflict
                }
                PullRequestIndexOutcome::Inserted => unreachable!("inspection cannot insert"),
            };
            return Ok(result);
        }
        let mut sources = candidate.sources;
        if !dry_run {
            for source in &mut sources {
                let mut replacement = metadata.clone();
                // These collections were deliberately not fetched, not authoritatively emptied.
                // All source identities have already been proved independently.
                for field in ["comments", "checks"] {
                    if replacement
                        .get(field)
                        .is_none_or(serde_json::Value::is_null)
                        && let Some(stored) = source.metadata.get(field)
                    {
                        replacement[field] = stored.clone();
                    }
                }
                match self
                    .foreign_entity_service
                    .replace_metadata_if_unchanged(source, replacement)
                    .await?
                {
                    Some(updated) => *source = updated,
                    None => {
                        result.outcome = PullRequestResyncOutcome::SourceChanged;
                        return Ok(result);
                    }
                }
            }
        }
        result.outcome = self
            .repo
            .initialize_resynced_row(&row, &sources, dry_run)
            .await
            .map_err(repository_error)?;
        Ok(result)
    }
}

fn validate_group(
    candidate: &PullRequestResyncCandidate,
    snapshot: &PullRequestResyncSnapshot,
) -> Result<u64, PullRequestResyncOutcome> {
    use PullRequestResyncOutcome::{IdentityConflict, Invalid};
    let pull = &snapshot.pull_request;
    let repository_id = pull
        .repository_id
        .filter(|id| *id > 0 && i64::try_from(*id).is_ok())
        .ok_or(Invalid)?;
    if pull.number == 0
        || i64::try_from(pull.number).is_err()
        || pull.reviews.is_none()
        || pull.status.is_none()
        || !candidate.github_key.eq_ignore_ascii_case(&pull.github_key)
        || !pull.github_key.eq_ignore_ascii_case(&format!(
            "{}/{}/pull/{}",
            pull.owner, pull.repo, pull.number
        ))
        || RepositorySlug::parse(&format!("{}/{}", pull.owner, pull.repo)).is_none()
        || candidate.sources.is_empty()
        || candidate.sources.len() > RESYNC_SOURCE_LIMIT
    {
        return Err(Invalid);
    }
    if let Some(mapping) = &snapshot.approved_mapping {
        if mapping.repository_id != repository_id
            || !mapping.github_key.eq_ignore_ascii_case(&pull.github_key)
        {
            return Err(IdentityConflict);
        }
        if mapping.source_ids.is_empty()
            || mapping
                .source_ids
                .iter()
                .any(|id| !candidate.sources.iter().any(|source| source.id == *id))
        {
            return Err(Invalid);
        }
    }
    Ok(repository_id)
}

fn verify_sources(
    candidate: &PullRequestResyncCandidate,
    snapshot: &PullRequestResyncSnapshot,
) -> Result<Vec<PullRequestSourceProof>, PullRequestResyncOutcome> {
    let repository_id = validate_group(candidate, snapshot)?;
    candidate
        .sources
        .iter()
        .map(|source| source_proof(source, snapshot, repository_id))
        .collect()
}

fn source_proof(
    source: &foreign_entity::domain::models::ForeignEntity,
    snapshot: &PullRequestResyncSnapshot,
    repository_id: u64,
) -> Result<PullRequestSourceProof, PullRequestResyncOutcome> {
    use PullRequestResyncOutcome::{IdentityConflict, Invalid, Unverified};
    let pull = &snapshot.pull_request;
    let metadata = &source.metadata;
    if source.foreign_entity_source != GITHUB_PULL_REQUEST_FOREIGN_ENTITY_SOURCE
        || !source
            .foreign_entity_id
            .eq_ignore_ascii_case(&pull.github_key)
        || metadata
            .get("githubKey")
            .and_then(|v| v.as_str())
            .is_none_or(|key| !key.eq_ignore_ascii_case(&pull.github_key))
        || metadata
            .get("owner")
            .and_then(|v| v.as_str())
            .is_none_or(|owner| !owner.eq_ignore_ascii_case(&pull.owner))
        || metadata
            .get("repo")
            .and_then(|v| v.as_str())
            .is_none_or(|repo| !repo.eq_ignore_ascii_case(&pull.repo))
        || metadata.get("number").and_then(|v| v.as_u64()) != Some(pull.number)
    {
        return Err(Invalid);
    }
    match metadata.get("repositoryId") {
        None | Some(serde_json::Value::Null) => comment_proof(source.id, metadata, snapshot)
            .or_else(|| {
                snapshot
                    .approved_mapping
                    .as_ref()
                    .filter(|mapping| mapping.source_ids.contains(&source.id))
                    .map(|_| PullRequestSourceProof {
                        source_id: source.id,
                        kind: PullRequestProofKind::ApprovedMapping,
                        resource_id: None,
                    })
            })
            .ok_or(Unverified),
        Some(value) => {
            let id = value
                .as_u64()
                .filter(|id| *id > 0 && i64::try_from(*id).is_ok())
                .ok_or(Invalid)?;
            if id != repository_id {
                return Err(IdentityConflict);
            }
            Ok(PullRequestSourceProof {
                source_id: source.id,
                kind: PullRequestProofKind::RepositoryId,
                resource_id: None,
            })
        }
    }
}

fn comment_proof(
    source_id: uuid::Uuid,
    metadata: &serde_json::Value,
    snapshot: &PullRequestResyncSnapshot,
) -> Option<PullRequestSourceProof> {
    for comment in metadata.get("comments")?.as_array()? {
        let Some(id) = comment
            .get("id")
            .and_then(|v| v.as_u64())
            .filter(|id| *id > 0)
        else {
            continue;
        };
        let kind = match comment.get("source").and_then(|v| v.as_str()) {
            Some("issue_comment") if snapshot.evidence.issue_comment_ids.contains(&id) => {
                PullRequestProofKind::IssueComment
            }
            Some("review_comment") if snapshot.evidence.review_comment_ids.contains(&id) => {
                PullRequestProofKind::ReviewComment
            }
            Some("review") if snapshot.evidence.review_ids.contains(&id) => {
                PullRequestProofKind::Review
            }
            _ => continue,
        };
        return Some(PullRequestSourceProof {
            source_id,
            kind,
            resource_id: Some(id),
        });
    }
    None
}
