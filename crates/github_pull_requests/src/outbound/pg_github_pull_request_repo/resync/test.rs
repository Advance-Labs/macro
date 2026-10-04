use super::*;
use crate::domain::ports::{GithubPullRequestIndexRepository, GithubPullRequestRepository};
use macro_db_migrator::MACRO_DB_MIGRATIONS;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn insert_source(pool: &PgPool, key: &str) {
    sqlx::query!(r#"INSERT INTO foreign_entity (id, foreign_entity_id, foreign_entity_source, metadata, stored_for_id, stored_for_auth_entity)
        VALUES ($1,$2,'github_pull_request',$3,$4,'user')"#,
        Uuid::now_v7(), key,
        json!({"githubKey":key,"owner":"macro","repo":"app","number":7,"repositoryId":42,
            "url":"https://github.com/macro/app/pull/7","displayName":"PR","name":"fresh","status":"open","reviews":[]}),
        format!("macro|{}@example.com", Uuid::now_v7()),
    ).execute(pool).await.unwrap();
}

fn row(source: &ForeignEntity) -> GithubPullRequestRow {
    let mut row = GithubPullRequestRow::from_metadata(&source.metadata).unwrap();
    row.github_key = "macro/app/pull/7".into();
    row
}

async fn typed_snapshot(pool: &PgPool) -> Vec<Value> {
    sqlx::query_scalar!(r#"SELECT to_jsonb(pr) AS "row!: serde_json::Value" FROM github_pull_request pr ORDER BY github_key"#)
        .fetch_all(pool).await.unwrap()
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn case_variants_share_one_candidate_and_dry_run_is_insert_free(pool: PgPool) {
    insert_source(&pool, "Macro/App/pull/7").await;
    insert_source(&pool, "macro/app/pull/7").await;
    let repo = PgGithubPullRequestRepo::new(pool.clone());
    assert_eq!(
        repo.resync_keys(None, 5).await.unwrap(),
        ["macro/app/pull/7"]
    );
    assert!(
        repo.resync_keys(Some("macro/app/pull/7"), 5)
            .await
            .unwrap()
            .is_empty()
    );
    let candidate = repo.resync_candidate("MACRO/APP/pull/7").await.unwrap();
    assert_eq!(candidate.sources.len(), 2);
    let row = row(&candidate.sources[0]);
    assert_eq!(
        repo.initialize_resynced_row(&row, &candidate.sources, true)
            .await
            .unwrap(),
        PullRequestResyncOutcome::Ready
    );
    assert!(typed_snapshot(&pool).await.is_empty());
    assert_eq!(
        repo.initialize_resynced_row(&row, &candidate.sources, false)
            .await
            .unwrap(),
        PullRequestResyncOutcome::Inserted
    );
    let before = typed_snapshot(&pool).await;
    assert_eq!(
        repo.initialize_resynced_row(&row, &candidate.sources, false)
            .await
            .unwrap(),
        PullRequestResyncOutcome::AlreadyPresent
    );
    assert_eq!(typed_snapshot(&pool).await, before);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn punctuation_pagination_matches_lexical_cursors_under_non_c_collation(pool: PgPool) {
    sqlx::query!(
        r#"ALTER TABLE foreign_entity ALTER COLUMN foreign_entity_id TYPE text COLLATE "en-x-icu""#
    )
    .execute(&pool)
    .await
    .unwrap();
    let expected = [
        "macro/app-a/pull/7",
        "macro/app.a/pull/7",
        "macro/app/pull/7",
        "macro/app_a/pull/7",
        "macro/appa/pull/7",
    ];
    for key in expected {
        insert_source(&pool, key).await;
    }
    insert_source(&pool, "MACRO/APP-A/pull/7").await;
    let native = sqlx::query_scalar!(
        r#"SELECT DISTINCT lower(foreign_entity_id) AS "key!" FROM foreign_entity
           ORDER BY lower(foreign_entity_id)"#
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_ne!(
        native, expected,
        "fixture must expose native locale ordering"
    );
    let repo = PgGithubPullRequestRepo::new(pool);
    let mut after: Option<String> = None;
    let mut seen = Vec::new();
    for expected_key in expected {
        let page = repo.resync_keys(after.as_deref(), 1).await.unwrap();
        assert_eq!(page, [expected_key]);
        if let Some(previous) = &after {
            assert!(page[0] > *previous);
        }
        seen.push(page[0].clone());
        after = Some(page[0].clone());
    }
    assert_eq!(seen, expected);
    assert!(
        repo.resync_keys(after.as_deref(), 1)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repo.resync_candidate("macro/app-a/pull/7")
            .await
            .unwrap()
            .sources
            .len(),
        2
    );
}
#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn changed_metadata_or_added_sibling_blocks_initialization(pool: PgPool) {
    insert_source(&pool, "macro/app/pull/7").await;
    let repo = PgGithubPullRequestRepo::new(pool.clone());
    let before = repo.resync_candidate("macro/app/pull/7").await.unwrap();
    let row = row(&before.sources[0]);
    sqlx::query!(
        "UPDATE foreign_entity SET metadata = '{}'::jsonb WHERE id = $1",
        before.sources[0].id
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        repo.initialize_resynced_row(&row, &before.sources, false)
            .await
            .unwrap(),
        PullRequestResyncOutcome::SourceChanged
    );
    let reread = repo.resync_candidate("macro/app/pull/7").await.unwrap();
    insert_source(&pool, "MACRO/app/pull/7").await;
    assert_eq!(
        repo.initialize_resynced_row(&row, &reread.sources, false)
            .await
            .unwrap(),
        PullRequestResyncOutcome::SourceChanged
    );
    assert!(typed_snapshot(&pool).await.is_empty());
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn existing_typed_identity_conflicts_never_change_rows(pool: PgPool) {
    insert_source(&pool, "macro/app/pull/7").await;
    let repo = PgGithubPullRequestRepo::new(pool.clone());
    let candidate = repo.resync_candidate("macro/app/pull/7").await.unwrap();
    let requested = row(&candidate.sources[0]);
    let mut other = requested.clone();
    other.github_key = "macro/renamed/pull/7".into();
    other.repo = "renamed".into();
    repo.initialize_indexed_row(&other).await.unwrap();
    let before = typed_snapshot(&pool).await;
    assert_eq!(
        repo.inspect_resync_row(&requested).await.unwrap(),
        Some(PullRequestIndexOutcome::IdentityConflict)
    );
    assert_eq!(
        repo.initialize_resynced_row(&requested, &candidate.sources, false)
            .await
            .unwrap(),
        PullRequestResyncOutcome::IdentityConflict
    );
    assert_eq!(typed_snapshot(&pool).await, before);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn source_writer_exclusion_has_a_bounded_timeout(pool: PgPool) {
    insert_source(&pool, "macro/app/pull/7").await;
    let repo = PgGithubPullRequestRepo::new(pool.clone());
    let candidate = repo.resync_candidate("macro/app/pull/7").await.unwrap();
    let row = row(&candidate.sources[0]);
    let mut writer = pool.begin().await.unwrap();
    sqlx::query!("LOCK TABLE foreign_entity IN ROW EXCLUSIVE MODE")
        .execute(&mut *writer)
        .await
        .unwrap();
    let failure = repo
        .initialize_resynced_row(&row, &candidate.sources, false)
        .await
        .unwrap_err();
    assert_eq!(
        failure
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("55P03")
    );
    writer.rollback().await.unwrap();
    assert!(typed_snapshot(&pool).await.is_empty());
    assert_eq!(
        repo.initialize_resynced_row(&row, &candidate.sources, false)
            .await
            .unwrap(),
        PullRequestResyncOutcome::Inserted
    );
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn a_live_row_created_after_preflight_remains_untouched(pool: PgPool) {
    insert_source(&pool, "macro/app/pull/7").await;
    let repo = PgGithubPullRequestRepo::new(pool.clone());
    let candidate = repo.resync_candidate("macro/app/pull/7").await.unwrap();
    let requested = row(&candidate.sources[0]);
    assert_eq!(repo.inspect_resync_row(&requested).await.unwrap(), None);
    let mut live = crate::domain::models::GithubPullRequestWrite::from_metadata(
        &candidate.sources[0].metadata,
    )
    .unwrap();
    live.title = Some("newer live title".into());
    repo.upsert_row(&live).await.unwrap();
    let before = typed_snapshot(&pool).await;
    assert_eq!(
        repo.initialize_resynced_row(&requested, &candidate.sources, false)
            .await
            .unwrap(),
        PullRequestResyncOutcome::AlreadyPresent
    );
    assert_eq!(typed_snapshot(&pool).await, before);
}
