use super::*;
use crate::domain::models::{QueryDefinition, QueryId, SavedQuery};

/// A `database_queries` row, its definition still JSON.
struct SavedQueryRecord {
    id: QueryId,
    database_id: Option<DatabaseId>,
    definition: serde_json::Value,
    created_by: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<SavedQueryRecord> for SavedQuery {
    type Error = serde_json::Error;

    fn try_from(record: SavedQueryRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: record.id,
            definition: serde_json::from_value(record.definition)?,
            database_id: record.database_id,
            created_by: record.created_by,
            created_at: record.created_at,
        })
    }
}

impl PgDatabasesRepo {
    pub(super) async fn insert_query(
        &self,
        database_id: Option<DatabaseId>,
        definition: &QueryDefinition,
        created_by: &str,
    ) -> Result<SavedQuery, PgDatabasesRepoError> {
        let saved = sqlx::query_as!(
            SavedQueryRecord,
            r#"
            INSERT INTO database_queries (id, database_id, definition, created_by)
            VALUES ($1, $2, $3, $4)
            RETURNING id, database_id, definition, created_by, created_at
            "#,
            macro_uuid::generate_uuid_v7(),
            database_id,
            serde_json::to_value(definition)?,
            created_by,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(saved.try_into()?)
    }

    pub(super) async fn query_by_id(
        &self,
        id: QueryId,
    ) -> Result<Option<SavedQuery>, PgDatabasesRepoError> {
        let saved = sqlx::query_as!(
            SavedQueryRecord,
            r#"
            SELECT id, database_id, definition, created_by, created_at
            FROM database_queries
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(saved.map(SavedQuery::try_from).transpose()?)
    }
}
