use super::*;
use serde_json::json;

#[test]
fn requests_default_to_dry_run_and_bound_page_size() {
    let mut request: PullRequestResyncRequest = serde_json::from_value(json!({})).unwrap();
    assert!(request.dry_run);
    assert!(!request.approve_mappings);
    assert_eq!(validate_request(&request).unwrap(), 1);
    request.limit = Some(1000);
    assert_eq!(validate_request(&request).unwrap(), 5);
    assert!(serde_json::from_value::<PullRequestResyncRequest>(json!({"apply":true})).is_err());
}

#[test]
fn approved_bindings_require_opt_in_and_positive_exact_scopes() {
    let mut request: PullRequestResyncRequest = serde_json::from_value(json!({"approvedMappings":[{
        "githubKey":"macro/app/pull/7","repositoryId":42,"sourceIds":["00000000-0000-0000-0000-000000000001"]
    }]})).unwrap();
    assert!(matches!(
        validate_request(&request),
        Err(GithubError::InvalidPullRequestResyncRequest)
    ));
    request.approve_mappings = true;
    assert!(validate_request(&request).is_ok());
    request.approved_mappings[0].repository_id = 0;
    assert!(validate_request(&request).is_err());
    request.approved_mappings[0].repository_id = 42;
    let id = request.approved_mappings[0].source_ids[0];
    request.approved_mappings[0].source_ids.push(id);
    assert!(validate_request(&request).is_err());
}

#[test]
fn malformed_keys_and_noncanonical_numbers_never_reach_github() {
    for key in [
        "macro/app/pull/0",
        "macro/app/pull/01",
        "macro/app/pull/18446744073709551615",
        "macro/app/pull/7/extra",
        "macro/app/issues/7",
    ] {
        assert!(parse_key(key).is_none(), "{key}");
    }
    assert!(parse_key("macro/app/pull/7").is_some());
}

fn candidate() -> PullRequestResyncCandidate {
    let now = chrono::Utc::now();
    PullRequestResyncCandidate {
        github_key: "macro/app/pull/7".to_string(),
        already_present: false,
        sources: vec![foreign_entity::domain::models::ForeignEntity {
            id: uuid::Uuid::now_v7(),
            foreign_entity_id: "macro/app/pull/7".to_string(),
            foreign_entity_source: "github_pull_request".to_string(),
            metadata: json!({"comments": [{"id": 55, "source": "issue_comment"}]}),
            stored_for_id: "macro|test@example.com".to_string(),
            stored_for_auth_entity: "user".to_string(),
            created_at: now,
            updated_at: now,
        }],
    }
}

#[test]
fn installation_access_must_cover_every_source_and_its_auth_entity() {
    let mut candidate = candidate();
    let user = GithubAppInstallationSource::User(candidate.sources[0].stored_for_id.clone());
    assert!(installation_covers_sources(&candidate, &[user.clone()]));
    assert!(!installation_covers_sources(&candidate, &[]));
    candidate.sources[0].stored_for_auth_entity = "team".to_string();
    assert!(!installation_covers_sources(&candidate, &[user.clone()]));
    let team_id = uuid::Uuid::now_v7();
    candidate.sources[0].stored_for_id = team_id.to_string();
    assert!(installation_covers_sources(
        &candidate,
        &[GithubAppInstallationSource::Team(team_id)]
    ));
    let mut sibling = candidate.sources[0].clone();
    sibling.id = uuid::Uuid::now_v7();
    sibling.stored_for_auth_entity = "user".to_string();
    sibling.stored_for_id = "macro|other@example.com".to_string();
    candidate.sources.push(sibling);
    assert!(!installation_covers_sources(
        &candidate,
        &[GithubAppInstallationSource::Team(team_id), user]
    ));
    candidate.sources.clear();
    assert!(!installation_covers_sources(&candidate, &[]));
}

#[test]
fn comment_ids_are_requested_only_for_sources_that_need_them() {
    let mut candidate = candidate();
    assert!(needs_comment_ids(&candidate, None));
    candidate.sources[0].metadata["repositoryId"] = json!(42);
    assert!(!needs_comment_ids(&candidate, None));
    candidate.sources[0].metadata["repositoryId"] = serde_json::Value::Null;
    let mapping = ApprovedPullRequestMapping {
        github_key: candidate.github_key.clone(),
        repository_id: 42,
        source_ids: vec![candidate.sources[0].id],
    };
    assert!(!needs_comment_ids(&candidate, Some(&mapping)));
    let mut sibling = candidate.sources[0].clone();
    sibling.id = uuid::Uuid::now_v7();
    candidate.sources.push(sibling);
    assert!(needs_comment_ids(&candidate, Some(&mapping)));
    candidate.sources[1].metadata["comments"] = json!([
        {"id": 66, "source": "review"},
        {"id": 0, "source": "issue_comment"}
    ]);
    assert!(!needs_comment_ids(&candidate, Some(&mapping)));
}

#[test]
fn generic_fetch_failures_retain_repair_retry_timing() {
    let attempt = ResyncAttempt::fetch_error(
        "macro/app/pull/7",
        GithubPullRequestFetchError::RateLimited {
            retry_after_seconds: 120,
        },
    );
    assert_eq!(
        attempt.result.outcome,
        PullRequestResyncOutcome::RateLimited
    );
    assert_eq!(attempt.retry_after_seconds, Some(120));
    for (error, outcome) in [
        (
            GithubPullRequestFetchError::Unavailable,
            PullRequestResyncOutcome::Unavailable,
        ),
        (
            GithubPullRequestFetchError::Incomplete,
            PullRequestResyncOutcome::Invalid,
        ),
        (
            GithubPullRequestFetchError::Retryable,
            PullRequestResyncOutcome::RetryableFailure,
        ),
    ] {
        let attempt = ResyncAttempt::fetch_error("macro/app/pull/7", error);
        assert_eq!(attempt.result.outcome, outcome);
        assert_eq!(attempt.retry_after_seconds, None);
    }
}

#[test]
fn duplicate_case_variant_approved_keys_are_rejected() {
    let candidate = candidate();
    let mapping = ApprovedPullRequestMapping {
        github_key: candidate.github_key.clone(),
        repository_id: 42,
        source_ids: vec![candidate.sources[0].id],
    };
    let mut duplicate = mapping.clone();
    duplicate.github_key = "Macro/App/pull/7".to_string();
    let request = PullRequestResyncRequest {
        after: None,
        limit: None,
        dry_run: true,
        approved_mappings: vec![mapping, duplicate],
        approve_mappings: true,
    };
    assert!(matches!(
        validate_request(&request),
        Err(GithubError::InvalidPullRequestResyncRequest)
    ));
}

#[test]
fn retryable_attempts_do_not_advance_the_cursor() {
    for attempt in [
        ResyncAttempt::fetch_error(
            "macro/app/pull/7",
            GithubPullRequestFetchError::RateLimited {
                retry_after_seconds: 120,
            },
        ),
        ResyncAttempt::fetch_error("macro/app/pull/7", GithubPullRequestFetchError::Retryable),
        ResyncAttempt::outcome("macro/app/pull/7", PullRequestResyncOutcome::SourceChanged),
    ] {
        let expected_delay = attempt.retry_after_seconds;
        let mut page = PullRequestResyncPage {
            dry_run: true,
            results: Vec::new(),
            next_after: Some("macro/app/pull/6".to_string()),
            has_more: false,
            retry_original_page: false,
            retry_after_seconds: None,
        };
        assert!(record_attempt(
            &mut page,
            "macro/app/pull/7".to_string(),
            attempt
        ));
        assert_eq!(page.next_after.as_deref(), Some("macro/app/pull/6"));
        assert!(page.retry_original_page);
        assert!(page.has_more);
        assert_eq!(page.retry_after_seconds, expected_delay);
        assert_eq!(page.results.len(), 1);
    }
}

#[test]
fn terminal_attempts_advance_the_cursor() {
    for outcome in [
        PullRequestResyncOutcome::AlreadyPresent,
        PullRequestResyncOutcome::Unavailable,
        PullRequestResyncOutcome::Unverified,
        PullRequestResyncOutcome::Invalid,
        PullRequestResyncOutcome::Ready,
        PullRequestResyncOutcome::Inserted,
    ] {
        let mut page = PullRequestResyncPage {
            dry_run: true,
            results: Vec::new(),
            next_after: None,
            has_more: true,
            retry_original_page: false,
            retry_after_seconds: None,
        };
        let attempt = ResyncAttempt::outcome("macro/app/pull/7", outcome);
        assert!(!record_attempt(
            &mut page,
            "macro/app/pull/7".to_string(),
            attempt
        ));
        assert_eq!(page.next_after.as_deref(), Some("macro/app/pull/7"));
        assert!(!page.retry_original_page);
        assert_eq!(page.results.len(), 1);
    }
}

#[test]
fn generic_snapshot_becomes_a_repair_command_only_in_the_repair_service() {
    let mut snapshot = GithubPullRequestSnapshot {
        details: serde_json::from_value(json!({
            "title": "fresh",
            "state": "closed",
            "repository_id": 42,
            "merged_at": "2026-01-01T00:00:00Z",
            "additions": 1,
            "deletions": 0,
            "reviews": []
        }))
        .unwrap(),
        resource_ids: Default::default(),
    };
    snapshot.resource_ids.issue_comment_ids.insert(55);
    snapshot.resource_ids.review_comment_ids.insert(77);
    snapshot.resource_ids.review_ids.insert(66);
    let candidate = candidate();
    let mapping = ApprovedPullRequestMapping {
        github_key: candidate.github_key.clone(),
        repository_id: 42,
        source_ids: vec![candidate.sources[0].id],
    };
    let repair = repair_snapshot(
        &RepositorySlug::parse("macro/app").unwrap(),
        7,
        snapshot,
        Some(mapping.clone()),
    );
    assert_eq!(repair.pull_request.github_key, "macro/app/pull/7");
    assert_eq!(repair.pull_request.repository_id, Some(42));
    assert_eq!(
        repair.pull_request.status,
        Some(crate::domain::models::GithubPullRequestStatus::Merged)
    );
    assert_eq!(repair.evidence.issue_comment_ids, [55].into());
    assert_eq!(repair.evidence.review_comment_ids, [77].into());
    assert_eq!(repair.evidence.review_ids, [66].into());
    assert_eq!(
        repair.approved_mapping.unwrap().source_ids,
        mapping.source_ids
    );
}
