//! PostgreSQL implementation of the [`UserKvRepo`] port.

#[cfg(test)]
mod tests;

use chrono::{DateTime, Utc};
use macro_user_id::user_id::MacroUserIdStr;
use sqlx::PgPool;
use sqlx::types::Json;

use crate::domain::models::{KvKey, KvNamespace, KvValue, UserKvEntry};
use crate::domain::ports::UserKvRepo;

/// Postgres-backed key-value repository over the `user_kv` table.
#[derive(Debug, Clone)]
pub struct PgUserKvRepo {
    pool: PgPool,
}

impl PgUserKvRepo {
    /// Create a repository backed by the provided pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Errors produced by the Postgres key-value repository.
#[derive(Debug, thiserror::Error)]
pub enum UserKvRepoErr {
    /// Underlying database error.
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    /// A stored namespace or key isn't a valid slug. The table's checks make
    /// this unreachable unless they are dropped.
    #[error("invalid namespace or key stored in user_kv")]
    InvalidRow,
}

struct UserKvRow {
    namespace: String,
    key: String,
    value: Json<KvValue>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<UserKvRow> for UserKvEntry {
    type Error = UserKvRepoErr;

    fn try_from(row: UserKvRow) -> Result<Self, Self::Error> {
        Ok(Self {
            namespace: KvNamespace::parse(row.namespace).map_err(|_| UserKvRepoErr::InvalidRow)?,
            key: KvKey::parse(row.key).map_err(|_| UserKvRepoErr::InvalidRow)?,
            value: row.value.0,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

impl UserKvRepo for PgUserKvRepo {
    type Err = UserKvRepoErr;

    #[tracing::instrument(err, skip_all)]
    async fn list_entries(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
    ) -> Result<Vec<UserKvEntry>, Self::Err> {
        sqlx::query_as!(
            UserKvRow,
            r#"
            SELECT namespace, key, value AS "value: Json<KvValue>", created_at, updated_at
            FROM user_kv
            WHERE user_id = $1 AND namespace = $2
            ORDER BY key
            "#,
            user_id.as_ref(),
            namespace.as_str(),
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(UserKvEntry::try_from)
        .collect()
    }

    #[tracing::instrument(err, skip_all)]
    async fn get_entry(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
        key: &KvKey,
    ) -> Result<Option<UserKvEntry>, Self::Err> {
        sqlx::query_as!(
            UserKvRow,
            r#"
            SELECT namespace, key, value AS "value: Json<KvValue>", created_at, updated_at
            FROM user_kv
            WHERE user_id = $1 AND namespace = $2 AND key = $3
            "#,
            user_id.as_ref(),
            namespace.as_str(),
            key.as_str(),
        )
        .fetch_optional(&self.pool)
        .await?
        .map(UserKvEntry::try_from)
        .transpose()
    }

    #[tracing::instrument(err, skip_all)]
    async fn upsert_entry(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
        key: &KvKey,
        value: &KvValue,
    ) -> Result<UserKvEntry, Self::Err> {
        sqlx::query_as!(
            UserKvRow,
            r#"
            INSERT INTO user_kv (user_id, namespace, key, value)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id, namespace, key)
            DO UPDATE SET value = EXCLUDED.value, updated_at = now()
            RETURNING namespace, key, value AS "value: Json<KvValue>", created_at, updated_at
            "#,
            user_id.as_ref(),
            namespace.as_str(),
            key.as_str(),
            Json(value) as _,
        )
        .fetch_one(&self.pool)
        .await?
        .try_into()
    }

    #[tracing::instrument(err, skip_all)]
    async fn delete_entry(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
        key: &KvKey,
    ) -> Result<bool, Self::Err> {
        let result = sqlx::query!(
            r#"DELETE FROM user_kv WHERE user_id = $1 AND namespace = $2 AND key = $3"#,
            user_id.as_ref(),
            namespace.as_str(),
            key.as_str(),
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(err, skip_all)]
    async fn count_entries(&self, user_id: &MacroUserIdStr<'_>) -> Result<i64, Self::Err> {
        Ok(sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!" FROM user_kv WHERE user_id = $1"#,
            user_id.as_ref(),
        )
        .fetch_one(&self.pool)
        .await?)
    }
}
