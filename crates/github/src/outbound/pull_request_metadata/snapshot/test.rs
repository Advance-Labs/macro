use super::*;
use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode, Uri},
    response::IntoResponse,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

type Responses = HashMap<String, (StatusCode, HeaderMap, Value)>;

async fn server(
    responses: Responses,
) -> (String, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let state = (Arc::new(responses), requests.clone());
    let app = Router::new()
        .fallback(
            |State((responses, requests)): State<(Arc<Responses>, Arc<Mutex<Vec<String>>>)>,
             uri: Uri| async move {
                let key = uri.path_and_query().unwrap().as_str().to_string();
                requests.lock().unwrap().push(key.clone());
                let (status, headers, body) = responses.get(&key).cloned().unwrap_or((
                    StatusCode::NOT_FOUND,
                    HeaderMap::new(),
                    Value::Null,
                ));
                (status, headers, axum::Json(body)).into_response()
            },
        )
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (base, requests, task)
}

fn core() -> Value {
    json!({
        "number": 7,
        "title": "fresh",
        "state": "closed",
        "merged_at": "2026-01-01T00:00:00Z",
        "additions": 1,
        "deletions": 0,
        "body": null,
        "user": {"id": 10, "login": "author"},
        "head": {"ref": "feature", "sha": "head"},
        "base": {
            "ref": "main", "sha": "base",
            "repo": {"id": 42, "full_name": "macro/app"}
        },
        "draft": false,
        "assignees": [{"id": 11, "login": "assigned"}],
        "labels": [{"name": "bug", "color": "ffffff"}],
        "requested_reviewers": []
    })
}

fn response(body: Value) -> (StatusCode, HeaderMap, Value) {
    (StatusCode::OK, HeaderMap::new(), body)
}
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

#[tokio::test]
async fn structural_snapshot_includes_merged_prs_without_discussion_or_checks() {
    let (base, requests, task) = server(HashMap::from([
        ("/repos/macro/app/pulls/7".into(), response(core())),
        (
            "/repos/macro/app/pulls/7/reviews?per_page=100&page=1".into(),
            response(json!([])),
        ),
    ]))
    .await;
    let live = fetch_snapshot(&client(), &base, "secret", "macro", "app", 7, false)
        .await
        .unwrap();
    assert_eq!(live.details.repository_id, Some(42));
    assert_eq!(live.details.status(), GithubPullRequestStatus::Merged);
    assert!(live.details.comments.is_none());
    assert!(live.details.checks.is_none());
    assert_eq!(live.details.labels.unwrap()[0].name, "bug");
    assert_eq!(live.details.assignees.unwrap()[0].github_user_id, "11");
    assert_eq!(requests.lock().unwrap().len(), 2);
    task.abort();
}

#[tokio::test]
async fn evidence_keeps_comment_namespaces_separate_and_reviews_complete() {
    let review = json!({
        "id": 66,
        "state": "APPROVED",
        "user": {"id": 12, "login": "reviewer"},
        "submitted_at": "2026-01-01T00:00:00Z"
    });
    let (base, _requests, task) = server(HashMap::from([
        ("/repos/macro/app/pulls/7".into(), response(core())),
        (
            "/repos/macro/app/pulls/7/reviews?per_page=100&page=1".into(),
            response(json!([review])),
        ),
        (
            "/repos/macro/app/issues/7/comments?per_page=100&page=1".into(),
            response(json!([{"id": 55}])),
        ),
        (
            "/repos/macro/app/pulls/7/comments?per_page=100&page=1".into(),
            response(json!([{"id": 77}])),
        ),
    ]))
    .await;
    let live = fetch_snapshot(&client(), &base, "secret", "macro", "app", 7, true)
        .await
        .unwrap();
    assert_eq!(live.resource_ids.issue_comment_ids, [55].into());
    assert_eq!(live.resource_ids.review_comment_ids, [77].into());
    assert_eq!(live.resource_ids.review_ids, [66].into());
    assert_eq!(
        live.details.reviews.unwrap()[0].state,
        GithubPullRequestReviewState::Approved
    );
    task.abort();
}

#[tokio::test]
async fn inconsistent_repository_and_number_are_rejected() {
    for field in ["repository", "number"] {
        let mut raw = core();
        if field == "repository" {
            raw["base"]["repo"]["full_name"] = json!("macro/renamed");
        } else {
            raw["number"] = json!(8);
        }
        let (base, _, task) = server(HashMap::from([(
            "/repos/macro/app/pulls/7".into(),
            response(raw),
        )]))
        .await;
        assert!(matches!(
            fetch_snapshot(&client(), &base, "secret", "macro", "app", 7, false).await,
            Err(GithubPullRequestFetchError::Incomplete)
        ));
        task.abort();
    }
}

#[tokio::test]
async fn rate_limit_timing_is_reported_without_consuming_error_bodies() {
    let mut headers = HeaderMap::new();
    headers.insert("retry-after", "120".parse().unwrap());
    let (base, _, task) = server(HashMap::from([(
        "/repos/macro/app/pulls/7".into(),
        (
            StatusCode::TOO_MANY_REQUESTS,
            headers,
            json!({"message":"limited"}),
        ),
    )]))
    .await;
    assert!(matches!(
        fetch_snapshot(&client(), &base, "secret", "macro", "app", 7, false).await,
        Err(GithubPullRequestFetchError::RateLimited {
            retry_after_seconds: 120
        })
    ));
    task.abort();
}

#[tokio::test]
async fn secondary_rate_limits_without_headers_are_retryable() {
    let (base, _, task) = server(HashMap::from([(
        "/repos/macro/app/pulls/7".into(),
        (
            StatusCode::FORBIDDEN,
            HeaderMap::new(),
            json!({"message":"secondary rate limit"}),
        ),
    )]))
    .await;
    assert!(matches!(
        fetch_snapshot(&client(), &base, "secret", "macro", "app", 7, false).await,
        Err(GithubPullRequestFetchError::RateLimited {
            retry_after_seconds: 60
        })
    ));
    task.abort();
}

#[tokio::test]
async fn redirects_are_not_followed() {
    let mut headers = HeaderMap::new();
    headers.insert("location", "/repos/macro/renamed/pulls/7".parse().unwrap());
    let (base, requests, task) = server(HashMap::from([(
        "/repos/macro/app/pulls/7".into(),
        (StatusCode::MOVED_PERMANENTLY, headers, Value::Null),
    )]))
    .await;
    assert!(matches!(
        fetch_snapshot(&client(), &base, "secret", "macro", "app", 7, false).await,
        Err(GithubPullRequestFetchError::Incomplete)
    ));
    assert_eq!(requests.lock().unwrap().len(), 1);
    task.abort();
}

#[tokio::test]
async fn failed_reviews_never_become_an_empty_authoritative_collection() {
    let (base, _, task) = server(HashMap::from([
        ("/repos/macro/app/pulls/7".into(), response(core())),
        (
            "/repos/macro/app/pulls/7/reviews?per_page=100&page=1".into(),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                HeaderMap::new(),
                Value::Null,
            ),
        ),
    ]))
    .await;
    assert!(matches!(
        fetch_snapshot(&client(), &base, "secret", "macro", "app", 7, false).await,
        Err(GithubPullRequestFetchError::Retryable)
    ));
    task.abort();
}

#[tokio::test]
async fn normal_enrichment_and_snapshot_share_core_decoding_and_mapping() {
    let (base, _, task) = server(HashMap::from([
        ("/repos/macro/app/pulls/7".into(), response(core())),
        (
            "/repos/macro/app/pulls/7/reviews?per_page=100&page=1".into(),
            response(json!([])),
        ),
    ]))
    .await;
    let normal = fetch_pull_request_core(
        &client(),
        &base,
        "secret",
        "macro",
        "app",
        7,
        CoreReadPolicy::Enrichment,
    )
    .await
    .unwrap()
    .into_details();
    let mut snapshot = fetch_snapshot(&client(), &base, "secret", "macro", "app", 7, false)
        .await
        .unwrap();
    // Reviews are additional snapshot data; every structural field uses the same conversion.
    assert_eq!(snapshot.details.reviews.take(), Some(Vec::new()));
    assert_eq!(normal, snapshot.details);
    task.abort();
}

#[tokio::test]
async fn normal_enrichment_preserves_its_permissive_identity_policy() {
    let mut raw = core();
    raw.as_object_mut().unwrap().remove("number");
    raw["base"]["repo"]
        .as_object_mut()
        .unwrap()
        .remove("full_name");
    let (base, _, task) = server(HashMap::from([(
        "/repos/macro/app/pulls/7".into(),
        response(raw),
    )]))
    .await;
    let normal = fetch_pull_request_core(
        &client(),
        &base,
        "secret",
        "macro",
        "app",
        7,
        CoreReadPolicy::Enrichment,
    )
    .await
    .unwrap()
    .into_details();
    assert_eq!(normal.repository_id, Some(42));
    assert!(matches!(
        fetch_snapshot(&client(), &base, "secret", "macro", "app", 7, false).await,
        Err(GithubPullRequestFetchError::Incomplete)
    ));
    task.abort();
}

#[tokio::test]
async fn snapshots_reserve_quota_without_changing_normal_enrichment() {
    let mut headers = HeaderMap::new();
    headers.insert("x-ratelimit-remaining", "100".parse().unwrap());
    let (base, _, task) = server(HashMap::from([(
        "/repos/macro/app/pulls/7".into(),
        (StatusCode::OK, headers, core()),
    )]))
    .await;
    assert!(
        fetch_pull_request_core(
            &client(),
            &base,
            "secret",
            "macro",
            "app",
            7,
            CoreReadPolicy::Enrichment,
        )
        .await
        .is_ok()
    );
    assert!(matches!(
        fetch_snapshot(&client(), &base, "secret", "macro", "app", 7, false).await,
        Err(GithubPullRequestFetchError::RateLimited { .. })
    ));
    task.abort();
}

#[tokio::test]
async fn malformed_core_responses_fail_instead_of_defaulting_required_metadata() {
    for field in ["title", "additions"] {
        let mut raw = core();
        if field == "title" {
            raw.as_object_mut().unwrap().remove(field);
        } else {
            raw[field] = json!("not a number");
        }
        let (base, _, task) = server(HashMap::from([(
            "/repos/macro/app/pulls/7".into(),
            response(raw),
        )]))
        .await;
        assert!(
            fetch_pull_request_core(
                &client(),
                &base,
                "secret",
                "macro",
                "app",
                7,
                CoreReadPolicy::Enrichment,
            )
            .await
            .is_err()
        );
        assert!(matches!(
            fetch_snapshot(&client(), &base, "secret", "macro", "app", 7, false).await,
            Err(GithubPullRequestFetchError::Retryable)
        ));
        task.abort();
    }
}

#[test]
fn shared_conversion_preserves_omitted_scalars_and_defaulted_collections() {
    let mut raw = core();
    for field in [
        "draft",
        "updated_at",
        "assignees",
        "labels",
        "requested_reviewers",
        "user",
    ] {
        raw.as_object_mut().unwrap().remove(field);
    }
    let core: GithubPullRequestResponse = serde_json::from_value(raw).unwrap();
    let details = core.into_details();
    assert_eq!(details.draft, None);
    assert_eq!(details.github_updated_at, None);
    assert_eq!(details.author_id, None);
    assert_eq!(details.author_login, None);
    assert_eq!(details.participant_github_user_ids, None);
    assert_eq!(details.requested_reviewer_github_user_ids, Some(Vec::new()));
    assert_eq!(details.assignees, Some(Vec::new()));
    assert_eq!(details.labels, Some(Vec::new()));
    assert_eq!(details.reviews, None);
    assert_eq!(details.comments, None);
    assert_eq!(details.checks, None);
}

#[tokio::test]
async fn snapshots_reject_reviews_that_exceed_the_page_bound() {
    let mut responses = HashMap::from([("/repos/macro/app/pulls/7".into(), response(core()))]);
    let reviews = vec![json!({"id": 66, "state": "PENDING"}); usize::from(METADATA_PAGE_SIZE)];
    for page in 1..=MAX_SNAPSHOT_PAGES {
        responses.insert(
            format!("/repos/macro/app/pulls/7/reviews?per_page=100&page={page}"),
            response(json!(reviews)),
        );
    }
    let (base, requests, task) = server(responses).await;
    assert!(matches!(
        fetch_snapshot(&client(), &base, "secret", "macro", "app", 7, false).await,
        Err(GithubPullRequestFetchError::Incomplete)
    ));
    assert_eq!(
        requests.lock().unwrap().len(),
        1 + MAX_SNAPSHOT_PAGES as usize
    );
    task.abort();
}
