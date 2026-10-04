use super::*;
use crate::domain::models::{EnrichedGithubPullRequest, PullRequestIdentityEvidence};
use chrono::Utc;
use foreign_entity::domain::models::{ForeignEntity, ForeignEntityError};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

fn source(repository_id: Option<u64>) -> ForeignEntity {
    let now = Utc::now();
    ForeignEntity {
        id: uuid::Uuid::now_v7(),
        foreign_entity_id: "Macro/App/pull/7".into(),
        foreign_entity_source: GITHUB_PULL_REQUEST_FOREIGN_ENTITY_SOURCE.into(),
        metadata: json!({"githubKey":"macro/app/pull/7", "owner":"macro", "repo":"app", "number":7,
            "repositoryId":repository_id,"url":"https://github.com/macro/app/pull/7","displayName":"PR",
            "name":"old", "status":"open", "comments":[{"id":55,"source":"issue_comment"}]}),
        stored_for_id: "macro|test@example.com".into(),
        stored_for_auth_entity: "user".into(),
        created_at: now,
        updated_at: now,
    }
}

fn snapshot() -> PullRequestResyncSnapshot {
    let mut metadata = source(Some(42)).metadata;
    metadata["name"] = json!("fresh");
    metadata["labels"] = json!([{"name":"bug"}]);
    metadata["reviews"] = json!([]);
    metadata["comments"] = Value::Null;
    PullRequestResyncSnapshot {
        pull_request: serde_json::from_value::<EnrichedGithubPullRequest>(metadata).unwrap(),
        evidence: PullRequestIdentityEvidence {
            issue_comment_ids: [55].into(),
            ..Default::default()
        },
        approved_mapping: None,
    }
}

fn candidate(sources: Vec<ForeignEntity>) -> PullRequestResyncCandidate {
    PullRequestResyncCandidate {
        github_key: "macro/app/pull/7".into(),
        sources,
        already_present: false,
    }
}

#[test]
fn approved_mappings_cover_only_exact_sources_and_never_override_identity() {
    let mut source = source(None);
    source.metadata["comments"] = json!([]);
    let mut live = snapshot();
    live.approved_mapping = Some(crate::domain::models::ApprovedPullRequestMapping {
        github_key: "macro/app/pull/7".into(),
        repository_id: 42,
        source_ids: vec![source.id],
    });
    let verified = verify_sources(&candidate(vec![source.clone()]), &live).unwrap();
    assert_eq!(verified[0].kind, PullRequestProofKind::ApprovedMapping);
    let mut sibling = source.clone();
    sibling.id = uuid::Uuid::now_v7();
    assert_eq!(
        verify_sources(&candidate(vec![source.clone(), sibling]), &live),
        Err(PullRequestResyncOutcome::Unverified)
    );
    source.metadata["repositoryId"] = json!(99);
    assert_eq!(
        verify_sources(&candidate(vec![source.clone()]), &live),
        Err(PullRequestResyncOutcome::IdentityConflict)
    );
    source.metadata["repositoryId"] = Value::Null;
    live.approved_mapping.as_mut().unwrap().repository_id = 99;
    assert_eq!(
        verify_sources(&candidate(vec![source]), &live),
        Err(PullRequestResyncOutcome::IdentityConflict)
    );
}

#[test]
fn missing_id_requires_resource_specific_evidence() {
    let candidate = candidate(vec![source(None)]);
    let mut live = snapshot();
    let proofs = verify_sources(&candidate, &live).unwrap();
    assert_eq!(proofs[0].kind, PullRequestProofKind::IssueComment);
    live.evidence.issue_comment_ids.clear();
    live.evidence.review_comment_ids.insert(55);
    assert_eq!(
        verify_sources(&candidate, &live),
        Err(PullRequestResyncOutcome::Unverified)
    );
}

#[test]
fn verified_sibling_does_not_prove_an_unverified_sibling() {
    let mut unknown = source(None);
    unknown.metadata["comments"] = json!([]);
    assert_eq!(
        verify_sources(&candidate(vec![source(Some(42)), unknown]), &snapshot()),
        Err(PullRequestResyncOutcome::Unverified)
    );
}

#[test]
fn conflicting_or_invalid_id_never_falls_back_to_comment_proof() {
    let mut bad = source(Some(99));
    assert_eq!(
        verify_sources(&candidate(vec![bad.clone()]), &snapshot()),
        Err(PullRequestResyncOutcome::IdentityConflict)
    );
    for value in [json!(0), json!(-1), json!("42"), json!(u64::MAX)] {
        bad.metadata["repositoryId"] = value;
        assert_eq!(
            verify_sources(&candidate(vec![bad.clone()]), &snapshot()),
            Err(PullRequestResyncOutcome::Invalid)
        );
    }
}

#[test]
fn mismatched_metadata_and_incomplete_reviews_are_rejected() {
    let mut bad = source(Some(42));
    bad.metadata["number"] = json!(8);
    assert_eq!(
        verify_sources(&candidate(vec![bad]), &snapshot()),
        Err(PullRequestResyncOutcome::Invalid)
    );
    let mut incomplete = snapshot();
    incomplete.pull_request.reviews = None;
    assert_eq!(
        verify_sources(&candidate(vec![source(Some(42))]), &incomplete),
        Err(PullRequestResyncOutcome::Invalid)
    );
}

#[derive(Clone)]
struct Fake(Arc<Mutex<State>>);
struct State {
    sources: Vec<ForeignEntity>,
    updates: usize,
    fail_source: Option<uuid::Uuid>,
    row: Option<GithubPullRequestRow>,
    fail_final: bool,
}

impl ForeignEntityMetadataService for Fake {
    async fn replace_metadata_if_unchanged(
        &self,
        expected: &ForeignEntity,
        metadata: Value,
    ) -> Result<Option<ForeignEntity>, ForeignEntityError> {
        let mut state = self.0.lock().unwrap();
        if state.fail_source == Some(expected.id) {
            return Ok(None);
        }
        state.updates += 1;
        let Some(source) = state
            .sources
            .iter_mut()
            .find(|source| source.id == expected.id && *source == expected)
        else {
            return Ok(None);
        };
        source.metadata = metadata;
        Ok(Some(source.clone()))
    }
}

impl GithubPullRequestResyncRepository for Fake {
    type Err = anyhow::Error;
    async fn resync_keys(
        &self,
        _after: Option<&str>,
        _limit: u32,
    ) -> Result<Vec<String>, Self::Err> {
        Ok(vec![])
    }
    async fn resync_candidate(&self, key: &str) -> Result<PullRequestResyncCandidate, Self::Err> {
        Ok(PullRequestResyncCandidate {
            github_key: key.into(),
            sources: self.0.lock().unwrap().sources.clone(),
            already_present: false,
        })
    }
    async fn inspect_resync_row(
        &self,
        _row: &GithubPullRequestRow,
    ) -> Result<Option<PullRequestIndexOutcome>, Self::Err> {
        Ok(None)
    }
    async fn initialize_resynced_row(
        &self,
        row: &GithubPullRequestRow,
        sources: &[ForeignEntity],
        dry_run: bool,
    ) -> Result<PullRequestResyncOutcome, Self::Err> {
        let mut state = self.0.lock().unwrap();
        if state.fail_final || state.sources != sources {
            return Ok(PullRequestResyncOutcome::SourceChanged);
        }
        if dry_run {
            return Ok(PullRequestResyncOutcome::Ready);
        }
        state.row = Some(row.clone());
        Ok(PullRequestResyncOutcome::Inserted)
    }
}

fn service(sources: Vec<ForeignEntity>) -> (GithubPullRequestServiceImpl<Fake, Fake>, Fake) {
    let fake = Fake(Arc::new(Mutex::new(State {
        sources,
        updates: 0,
        fail_source: None,
        row: None,
        fail_final: false,
    })));
    (
        GithubPullRequestServiceImpl {
            foreign_entity_service: fake.clone(),
            repo: fake.clone(),
        },
        fake,
    )
}

#[tokio::test]
async fn dry_run_and_unverified_groups_never_update_sources() {
    let (service, fake) = service(vec![source(None)]);
    let before = fake.0.lock().unwrap().sources.clone();
    let ready = service
        .resync_verified_snapshot(candidate(before.clone()), snapshot(), true)
        .await
        .unwrap();
    assert_eq!(ready.outcome, PullRequestResyncOutcome::Ready);
    assert_eq!(fake.0.lock().unwrap().updates, 0);
    assert_eq!(fake.0.lock().unwrap().sources, before);
    let mut unknown = source(None);
    unknown.metadata["comments"] = json!([]);
    let rejected = service
        .resync_verified_snapshot(candidate(vec![unknown]), snapshot(), false)
        .await
        .unwrap();
    assert_eq!(rejected.outcome, PullRequestResyncOutcome::Unverified);
    assert_eq!(fake.0.lock().unwrap().updates, 0);
}

#[tokio::test]
async fn successful_repair_preserves_associations_and_omitted_discussion() {
    let original = source(None);
    let (service, fake) = service(vec![original.clone()]);
    let result = service
        .resync_verified_snapshot(candidate(vec![original.clone()]), snapshot(), false)
        .await
        .unwrap();
    assert_eq!(result.outcome, PullRequestResyncOutcome::Inserted);
    let state = fake.0.lock().unwrap();
    assert_eq!(state.sources[0].stored_for_id, original.stored_for_id);
    assert_eq!(
        state.sources[0].foreign_entity_id,
        original.foreign_entity_id
    );
    assert_eq!(state.sources[0].metadata["repositoryId"], json!(42));
    assert_eq!(
        state.sources[0].metadata["comments"],
        original.metadata["comments"]
    );
    assert_eq!(state.row.as_ref().unwrap().title.as_deref(), Some("fresh"));
}

#[tokio::test]
async fn partial_failure_does_not_initialize_and_retries_reread_sources() {
    let first = source(None);
    let second = source(None);
    let (service, fake) = service(vec![first.clone(), second.clone()]);
    fake.0.lock().unwrap().fail_source = Some(second.id);
    let failed = service
        .resync_verified_snapshot(candidate(vec![first, second]), snapshot(), false)
        .await
        .unwrap();
    assert_eq!(failed.outcome, PullRequestResyncOutcome::SourceChanged);
    assert!(fake.0.lock().unwrap().row.is_none());
    fake.0.lock().unwrap().fail_source = None;
    let reread = service.resync_candidate("macro/app/pull/7").await.unwrap();
    assert_eq!(
        service
            .resync_verified_snapshot(reread, snapshot(), false)
            .await
            .unwrap()
            .outcome,
        PullRequestResyncOutcome::Inserted
    );
}

#[tokio::test]
async fn rejected_groups_report_exact_sources_and_retain_verified_sibling_proofs() {
    let verified = source(Some(42));
    let mut unknown = source(None);
    unknown.metadata["comments"] = json!([]);
    let (service, fake) = service(vec![verified.clone(), unknown.clone()]);
    let result = service
        .resync_verified_snapshot(
            candidate(vec![verified.clone(), unknown.clone()]),
            snapshot(),
            false,
        )
        .await
        .unwrap();
    assert_eq!(result.outcome, PullRequestResyncOutcome::Unverified);
    assert_eq!(result.proofs.len(), 1);
    assert_eq!(result.proofs[0].source_id, verified.id);
    assert_eq!(result.rejections.len(), 1);
    assert_eq!(result.rejections[0].source_id, unknown.id);
    assert_eq!(result.source_ids.len(), 2);
    assert_eq!(fake.0.lock().unwrap().updates, 0);
}

#[tokio::test]
async fn final_membership_change_prevents_typed_initialization() {
    let source = source(Some(42));
    let (service, fake) = service(vec![source.clone()]);
    fake.0.lock().unwrap().fail_final = true;
    assert_eq!(
        service
            .resync_verified_snapshot(candidate(vec![source]), snapshot(), false)
            .await
            .unwrap()
            .outcome,
        PullRequestResyncOutcome::SourceChanged
    );
    assert!(fake.0.lock().unwrap().row.is_none());
}
