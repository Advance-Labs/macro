use super::*;
use crate::domain::models::{QueryDefinition, QueryId, SavedQuery};

impl PgDatabasesRepo {
    pub(super) async fn insert_query(
        &self,
        database_id: Option<DatabaseId>,
        definition: &QueryDefinition,
        created_by: &str,
    ) -> Result<SavedQuery, PgDatabasesRepoError> {
        let row = sqlx::query!(
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
        Ok(SavedQuery {
            id: row.id,
            definition: serde_json::from_value(row.definition)?,
            database_id: row.database_id,
            created_by: row.created_by,
            created_at: row.created_at,
        })
    }

    pub(super) async fn query_by_id(
        &self,
        id: QueryId,
    ) -> Result<Option<SavedQuery>, PgDatabasesRepoError> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT id, database_id, definition, created_by, created_at
            FROM database_queries
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };
        Ok(Some(SavedQuery {
            id: row.id,
            definition: serde_json::from_value(row.definition)?,
            database_id: row.database_id,
            created_by: row.created_by,
            created_at: row.created_at,
        }))
    }
}
