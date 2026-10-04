use macro_db_migrator::MACRO_DB_MIGRATIONS;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use super::*;
use crate::domain::models::CreateForeignEntity;
use crate::domain::ports::{ForeignEntityMetadataService, ForeignEntityRepository};
use crate::domain::service::ForeignEntityServiceImpl;

async fn insert(repo: &PgForeignEntityRepo) -> ForeignEntity {
    repo.create_foreign_entity(
        Uuid::now_v7(),
        CreateForeignEntity {
            foreign_entity_id: "pr-1".to_owned(),
            foreign_entity_source: "github".to_owned(),
            metadata: json!({"revision": 1}),
            stored_for_id: "document-1".to_owned(),
            stored_for_auth_entity: "document".to_owned(),
        },
    )
    .await
    .unwrap()
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn replacement_only_changes_metadata_and_update_time(pool: PgPool) {
    let repo = PgForeignEntityRepo::new(pool);
    let original = insert(&repo).await;
    let replacement = ForeignEntityServiceImpl::new(repo)
        .replace_metadata_if_unchanged(&original, json!({"revision": 2}))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(replacement.metadata, json!({"revision": 2}));
    assert!(replacement.updated_at >= original.updated_at);
    assert_eq!(replacement.id, original.id);
    assert_eq!(replacement.foreign_entity_id, original.foreign_entity_id);
    assert_eq!(
        replacement.foreign_entity_source,
        original.foreign_entity_source
    );
    assert_eq!(replacement.stored_for_id, original.stored_for_id);
    assert_eq!(
        replacement.stored_for_auth_entity,
        original.stored_for_auth_entity
    );
    assert_eq!(replacement.created_at, original.created_at);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn unchanged_retries_preserve_update_time(pool: PgPool) {
    let repo = PgForeignEntityRepo::new(pool);
    let original = insert(&repo).await;
    let first = repo
        .replace_metadata_if_unchanged(&original, original.metadata.clone())
        .await
        .unwrap()
        .unwrap();
    let second = repo
        .replace_metadata_if_unchanged(&original, original.metadata.clone())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(first, original);
    assert_eq!(second, original);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn concurrent_changes_only_allow_one_writer(pool: PgPool) {
    let repo = PgForeignEntityRepo::new(pool.clone());
    let original = insert(&repo).await;
    let (first, second) = tokio::join!(
        repo.replace_metadata_if_unchanged(&original, json!({"writer": 1})),
        repo.replace_metadata_if_unchanged(&original, json!({"writer": 2})),
    );
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(
        usize::from(first.is_some()) + usize::from(second.is_some()),
        1
    );

    let persisted = repo
        .get_foreign_entity_by_id(original.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted, first.or(second).unwrap());
    assert_eq!(
        repo.replace_metadata_if_unchanged(&original, json!({"writer": 3}))
            .await
            .unwrap(),
        None
    );
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn changed_associations_reject_stale_snapshot(pool: PgPool) {
    let repo = PgForeignEntityRepo::new(pool);
    let original = insert(&repo).await;
    for patch in [
        crate::domain::models::PatchForeignEntity {
            stored_for_id: Some("document-2".to_owned()),
            ..Default::default()
        },
        crate::domain::models::PatchForeignEntity {
            stored_for_auth_entity: Some("team".to_owned()),
            ..Default::default()
        },
    ] {
        let current = repo
            .patch_foreign_entity(original.id, patch)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            repo.replace_metadata_if_unchanged(&original, json!({"revision": 2}))
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            repo.get_foreign_entity_by_id(original.id).await.unwrap(),
            Some(current)
        );
    }
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn missing_row_returns_none(pool: PgPool) {
    let repo = PgForeignEntityRepo::new(pool);
    let original = insert(&repo).await;
    repo.delete_foreign_entity(original.id).await.unwrap();
    assert_eq!(
        repo.replace_metadata_if_unchanged(&original, json!({"revision": 2}))
            .await
            .unwrap(),
        None
    );
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn every_snapshot_field_must_match(pool: PgPool) {
    let repo = PgForeignEntityRepo::new(pool);
    let original = insert(&repo).await;
    let mut mismatches = Vec::new();

    let mut changed = original.clone();
    changed.id = Uuid::now_v7();
    mismatches.push(changed);
    let mut changed = original.clone();
    changed.foreign_entity_id = "PR-1".to_owned();
    mismatches.push(changed);
    let mut changed = original.clone();
    changed.foreign_entity_source = "GitHub".to_owned();
    mismatches.push(changed);
    let mut changed = original.clone();
    changed.metadata = json!({"revision": 2});
    mismatches.push(changed);
    let mut changed = original.clone();
    changed.stored_for_id = "document-2".to_owned();
    mismatches.push(changed);
    let mut changed = original.clone();
    changed.stored_for_auth_entity = "team".to_owned();
    mismatches.push(changed);
    let mut changed = original.clone();
    changed.created_at += chrono::Duration::seconds(1);
    mismatches.push(changed);
    let mut changed = original.clone();
    changed.updated_at += chrono::Duration::seconds(1);
    mismatches.push(changed);

    for expected in mismatches {
        assert_eq!(
            repo.replace_metadata_if_unchanged(&expected, json!({"revision": 3}))
                .await
                .unwrap(),
            None
        );
    }
    assert_eq!(
        repo.get_foreign_entity_by_id(original.id).await.unwrap(),
        Some(original)
    );
}
