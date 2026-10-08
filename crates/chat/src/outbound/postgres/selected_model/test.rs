use super::*;
use macro_db_migrator::MACRO_DB_MIGRATIONS;
use sqlx::PgPool;

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../fixtures", scripts("users"))
)]
async fn remembers_and_replaces_the_selected_model(pool: PgPool) {
    let repo = PgSelectedModelRepo::new(pool);
    let user_id = "macro|test@example.com";

    assert_eq!(repo.get(user_id).await.unwrap(), None);

    repo.set(user_id, "anthropic/claude-opus-5-5")
        .await
        .unwrap();
    assert_eq!(
        repo.get(user_id).await.unwrap().as_deref(),
        Some("anthropic/claude-opus-5-5")
    );

    repo.set(user_id, "google/gemini-3.8-flash").await.unwrap();
    assert_eq!(
        repo.get(user_id).await.unwrap().as_deref(),
        Some("google/gemini-3.8-flash")
    );
}
