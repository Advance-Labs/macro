//! Bounded source snapshots and read-side fencing for insert-only repair.

use foreign_entity::domain::models::ForeignEntity;

use super::{
    PgGithubPullRequestRepo,
    index::{existing_index_outcome, initialize_row},
};
use crate::domain::{
    models::{
        GITHUB_PULL_REQUEST_FOREIGN_ENTITY_SOURCE, GithubPullRequestRow, PullRequestIndexOutcome,
        PullRequestResyncCandidate, PullRequestResyncOutcome, RESYNC_SOURCE_LIMIT,
    },
    ports::GithubPullRequestResyncRepository,
};

#[cfg(test)]
mod test;

impl GithubPullRequestResyncRepository for PgGithubPullRequestRepo {
    type Err = sqlx::Error;

    #[tracing::instrument(skip(self), err)]
    async fn resync_keys(&self, after: Option<&str>, limit: u32) -> Result<Vec<String>, Self::Err> {
        sqlx::query_scalar!(
            r#"SELECT lower(foreign_entity_id) COLLATE "C" AS "key!"
               FROM foreign_entity
               WHERE foreign_entity_source = $1
                 AND ($2::text IS NULL OR lower(foreign_entity_id) COLLATE "C" > $2)
               GROUP BY lower(foreign_entity_id) COLLATE "C"
               ORDER BY lower(foreign_entity_id) COLLATE "C"
               LIMIT $3"#,
            GITHUB_PULL_REQUEST_FOREIGN_ENTITY_SOURCE,
            after,
            i64::from(limit),
        )
        .fetch_all(&self.pool)
        .await
    }

    #[tracing::instrument(skip(self), err)]
    async fn resync_candidate(&self, key: &str) -> Result<PullRequestResyncCandidate, Self::Err> {
        let sources = sqlx::query_as!(
            ForeignEntity,
            r#"SELECT id, foreign_entity_id, foreign_entity_source, metadata,
                      stored_for_id, stored_for_auth_entity, created_at, updated_at
               FROM foreign_entity
               WHERE foreign_entity_source = $1 AND lower(foreign_entity_id) COLLATE "C" = lower($2)
               ORDER BY id LIMIT $3"#,
            GITHUB_PULL_REQUEST_FOREIGN_ENTITY_SOURCE,
            key,
            (RESYNC_SOURCE_LIMIT + 1) as i64,
        )
        .fetch_all(&self.pool)
        .await?;
        let already_present = sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM github_pull_request WHERE lower(github_key) = lower($1)) AS \"exists!\"",
            key,
        ).fetch_one(&self.pool).await?;
        Ok(PullRequestResyncCandidate {
            github_key: key.to_string(),
            sources,
            already_present,
        })
    }

    #[tracing::instrument(skip_all, err)]
    async fn inspect_resync_row(
        &self,
        row: &GithubPullRequestRow,
    ) -> Result<Option<PullRequestIndexOutcome>, Self::Err> {
        let mut tx = self.pool.begin().await?;
        sqlx::query!("SET LOCAL lock_timeout = '1s'")
            .execute(&mut *tx)
            .await?;
        sqlx::query!("SET LOCAL statement_timeout = '3s'")
            .execute(&mut *tx)
            .await?;
        let outcome =
            existing_index_outcome(&mut tx, row, row.repository_id.unwrap_or_default()).await?;
        tx.commit().await?;
        Ok(outcome)
    }

    #[tracing::instrument(skip_all, err)]
    async fn initialize_resynced_row(
        &self,
        row: &GithubPullRequestRow,
        sources: &[ForeignEntity],
        dry_run: bool,
    ) -> Result<PullRequestResyncOutcome, Self::Err> {
        let mut tx = self.pool.begin().await?;
        sqlx::query!("SET LOCAL lock_timeout = '1s'")
            .execute(&mut *tx)
            .await?;
        sqlx::query!("SET LOCAL statement_timeout = '3s'")
            .execute(&mut *tx)
            .await?;
        // Read-side ownership only: block source DML while validating membership and inserting
        // the typed row. No GitHub calls or foreign-entity mutations occur inside this fence.
        sqlx::query!("LOCK TABLE foreign_entity IN SHARE MODE")
            .execute(&mut *tx)
            .await?;
        let current = sqlx::query_as!(
            ForeignEntity,
            r#"SELECT id, foreign_entity_id, foreign_entity_source, metadata,
                      stored_for_id, stored_for_auth_entity, created_at, updated_at
               FROM foreign_entity
               WHERE foreign_entity_source = $1 AND lower(foreign_entity_id) COLLATE "C" = lower($2)
               ORDER BY id LIMIT $3"#,
            GITHUB_PULL_REQUEST_FOREIGN_ENTITY_SOURCE,
            row.github_key,
            (RESYNC_SOURCE_LIMIT + 1) as i64,
        )
        .fetch_all(&mut *tx)
        .await?;
        let mut expected = sources.iter().collect::<Vec<_>>();
        expected.sort_by_key(|source| source.id);
        if expected.is_empty()
            || expected.len() > RESYNC_SOURCE_LIMIT
            || current.iter().collect::<Vec<_>>() != expected
        {
            tx.rollback().await?;
            return Ok(PullRequestResyncOutcome::SourceChanged);
        }
        let outcome = if dry_run {
            match existing_index_outcome(&mut tx, row, row.repository_id.unwrap_or_default())
                .await?
            {
                None => PullRequestResyncOutcome::Ready,
                Some(PullRequestIndexOutcome::AlreadyPresent) => {
                    PullRequestResyncOutcome::AlreadyPresent
                }
                Some(_) => PullRequestResyncOutcome::IdentityConflict,
            }
        } else {
            match initialize_row(&mut tx, row).await? {
                PullRequestIndexOutcome::Inserted => PullRequestResyncOutcome::Inserted,
                PullRequestIndexOutcome::AlreadyPresent => PullRequestResyncOutcome::AlreadyPresent,
                PullRequestIndexOutcome::IdentityConflict => {
                    PullRequestResyncOutcome::IdentityConflict
                }
            }
        };
        tx.commit().await?;
        Ok(outcome)
    }
}
