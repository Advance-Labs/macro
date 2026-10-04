//! Conditional metadata-only updates for foreign entity records.

#[cfg(test)]
mod test;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::PgForeignEntityRepo;
use crate::domain::models::ForeignEntity;
use crate::domain::ports::ForeignEntityMetadataRepository;

impl ForeignEntityMetadataRepository for PgForeignEntityRepo {
    type Err = sqlx::Error;

    #[tracing::instrument(err, skip(self, expected, metadata))]
    async fn replace_metadata_if_unchanged(
        &self,
        expected: &ForeignEntity,
        metadata: serde_json::Value,
    ) -> Result<Option<ForeignEntity>, Self::Err> {
        sqlx::query_as!(
            ForeignEntity,
            r#"
            UPDATE foreign_entity
            SET metadata = $2,
                updated_at = CASE WHEN foreign_entity.metadata = $2 THEN updated_at ELSE NOW() END
            WHERE id = $1
              AND foreign_entity_id = $3
              AND foreign_entity_source = $4
              AND stored_for_id = $5
              AND stored_for_auth_entity = $6
              AND foreign_entity.metadata = $7
              AND created_at = $8
              AND updated_at = $9
            RETURNING
                id as "id!: Uuid",
                foreign_entity_id as "foreign_entity_id!: String",
                foreign_entity_source as "foreign_entity_source!: String",
                metadata as "metadata!: serde_json::Value",
                stored_for_id as "stored_for_id!: String",
                stored_for_auth_entity as "stored_for_auth_entity!: String",
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>"
            "#,
            expected.id,
            metadata,
            expected.foreign_entity_id,
            expected.foreign_entity_source,
            expected.stored_for_id,
            expected.stored_for_auth_entity,
            expected.metadata,
            expected.created_at,
            expected.updated_at,
        )
        .fetch_optional(&self.pool)
        .await
    }
}
