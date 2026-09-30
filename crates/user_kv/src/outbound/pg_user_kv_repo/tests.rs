// Fixtures insert users, and the constraint test inserts rows the typed
// queries can't express (invalid slugs, arrays), so plain `sqlx::query` is
// intended here.
#![allow(clippy::disallowed_methods)]

use macro_db_migrator::MACRO_DB_MIGRATIONS;
use macro_user_id::user_id::MacroUserIdStr;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use super::PgUserKvRepo;
use crate::domain::models::{KvKey, KvNamespace, KvValue};
use crate::domain::ports::UserKvRepo;

const USER_A: &str = "macro|user-a@macro.com";
const USER_B: &str = "macro|user-b@macro.com";

fn user(id: &str) -> MacroUserIdStr<'_> {
    MacroUserIdStr::parse_from_str(id).expect("valid user id")
}

fn ns(value: &str) -> KvNamespace {
    KvNamespace::parse(value).expect("valid namespace")
}

fn key(value: &str) -> KvKey {
    KvKey::parse(value).expect("valid key")
}

fn object(value: serde_json::Value) -> KvValue {
    value.as_object().expect("object").clone()
}

async fn insert_user(pool: &PgPool, id: &str) {
    let macro_user_id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO macro_user (id, username, email, stripe_customer_id) VALUES ($1, $2, $2, $2)"#,
    )
    .bind(macro_user_id)
    .bind(id)
    .execute(pool)
    .await
    .expect("macro_user should insert");
    sqlx::query(r#"INSERT INTO "User" (id, email, macro_user_id) VALUES ($1, $1, $2)"#)
        .bind(id)
        .bind(macro_user_id)
        .execute(pool)
        .await
        .expect("user should insert");
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn upsert_creates_then_replaces_keeping_created_at(pool: PgPool) {
    insert_user(&pool, USER_A).await;
    let repo = PgUserKvRepo::new(pool);
    let target = (user(USER_A), ns("tours"), key("calendar"));

    let created = repo
        .upsert_entry(
            &target.0,
            &target.1,
            &target.2,
            &object(json!({ "step": 1 })),
        )
        .await
        .expect("insert");
    assert_eq!(created.value, object(json!({ "step": 1 })));
    assert_eq!(created.created_at, created.updated_at);

    let replaced = repo
        .upsert_entry(
            &target.0,
            &target.1,
            &target.2,
            &object(json!({ "status": "completed" })),
        )
        .await
        .expect("replace");
    assert_eq!(replaced.value, object(json!({ "status": "completed" })));
    assert_eq!(replaced.created_at, created.created_at);
    assert!(replaced.updated_at >= created.updated_at);

    let fetched = repo
        .get_entry(&target.0, &target.1, &target.2)
        .await
        .expect("get")
        .expect("entry exists");
    assert_eq!(fetched.value, replaced.value);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn entries_are_scoped_by_user_and_namespace(pool: PgPool) {
    insert_user(&pool, USER_A).await;
    insert_user(&pool, USER_B).await;
    let repo = PgUserKvRepo::new(pool);

    for k in ["mail", "calendar"] {
        repo.upsert_entry(&user(USER_A), &ns("tours"), &key(k), &KvValue::new())
            .await
            .expect("insert");
    }
    repo.upsert_entry(
        &user(USER_A),
        &ns("tips"),
        &key("calendar"),
        &KvValue::new(),
    )
    .await
    .expect("insert");
    repo.upsert_entry(&user(USER_B), &ns("tours"), &key("home"), &KvValue::new())
        .await
        .expect("insert");

    let tours = repo
        .list_entries(&user(USER_A), &ns("tours"))
        .await
        .expect("list");
    let keys: Vec<_> = tours.iter().map(|entry| entry.key.as_str()).collect();
    assert_eq!(keys, ["calendar", "mail"], "one namespace, ordered by key");

    assert_eq!(repo.count_entries(&user(USER_A)).await.expect("count"), 3);
    assert_eq!(repo.count_entries(&user(USER_B)).await.expect("count"), 1);
    assert!(
        repo.get_entry(&user(USER_B), &ns("tours"), &key("mail"))
            .await
            .expect("get")
            .is_none()
    );
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn delete_reports_whether_a_row_was_removed(pool: PgPool) {
    insert_user(&pool, USER_A).await;
    let repo = PgUserKvRepo::new(pool);
    repo.upsert_entry(
        &user(USER_A),
        &ns("tours"),
        &key("calendar"),
        &KvValue::new(),
    )
    .await
    .expect("insert");

    assert!(
        repo.delete_entry(&user(USER_A), &ns("tours"), &key("calendar"))
            .await
            .expect("delete")
    );
    assert!(
        !repo
            .delete_entry(&user(USER_A), &ns("tours"), &key("calendar"))
            .await
            .expect("delete")
    );
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn entries_are_removed_with_their_user(pool: PgPool) {
    insert_user(&pool, USER_A).await;
    let repo = PgUserKvRepo::new(pool.clone());
    repo.upsert_entry(
        &user(USER_A),
        &ns("tours"),
        &key("calendar"),
        &KvValue::new(),
    )
    .await
    .expect("insert");

    sqlx::query(r#"DELETE FROM "User" WHERE id = $1"#)
        .bind(USER_A)
        .execute(&pool)
        .await
        .expect("user delete");
    assert_eq!(repo.count_entries(&user(USER_A)).await.expect("count"), 0);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn table_checks_reject_bad_rows_the_service_would_catch(pool: PgPool) {
    insert_user(&pool, USER_A).await;
    let insert = |namespace: &'static str, key: &'static str, value: serde_json::Value| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                r#"INSERT INTO user_kv (user_id, namespace, key, value) VALUES ($1, $2, $3, $4)"#,
            )
            .bind(USER_A)
            .bind(namespace)
            .bind(key)
            .bind(value)
            .execute(&pool)
            .await
        }
    };

    assert!(
        insert("Tours", "calendar", json!({})).await.is_err(),
        "namespace slug"
    );
    assert!(
        insert("tours", "has space", json!({})).await.is_err(),
        "key slug"
    );
    assert!(
        insert("tours", "calendar", json!([1, 2])).await.is_err(),
        "object only"
    );
    assert!(
        insert("tours", "big", json!({ "blob": "x".repeat(40_000) }))
            .await
            .is_err(),
        "size backstop"
    );
    assert!(
        insert("tours", "calendar", json!({ "ok": true }))
            .await
            .is_ok()
    );
}
