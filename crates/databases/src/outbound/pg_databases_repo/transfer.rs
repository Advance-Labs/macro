use super::*;
use crate::domain::models::Viewer;
use crate::domain::transfer::{DatabaseTransferRepo, ImportOutcome, ImportTable};

impl DatabaseTransferRepo for PgDatabasesRepo {
    type Err = PgDatabasesRepoError;

    async fn imported_table(
        &self,
        database_id: DatabaseId,
        request_id: Uuid,
    ) -> Result<Option<(Table, String)>, Self::Err> {
        let row = sqlx::query!(
            "SELECT id, database_id, name, position, version, import_fingerprint FROM database_tables WHERE database_id = $1 AND import_key = $2",
            database_id, request_id,
        ).fetch_optional(&self.pool).await?;
        Ok(row.map(|row| {
            (
                Table {
                    id: row.id,
                    database_id: row.database_id,
                    name: row.name,
                    position: row.position,
                    version: TableVersion(row.version),
                },
                row.import_fingerprint.unwrap_or_default(),
            )
        }))
    }

    async fn import_table(
        &self,
        database_id: DatabaseId,
        viewer: &Viewer,
        request: &ImportTable,
        fingerprint: &str,
        definitions: &[PropertyDefinitionId],
    ) -> Result<ImportOutcome, Self::Err> {
        let mut transaction = self.pool.begin().await?;
        if sqlx::query_scalar!(
            "SELECT id FROM databases WHERE id = $1 AND trashed_at IS NULL FOR UPDATE",
            database_id
        )
        .fetch_optional(&mut *transaction)
        .await?
        .is_none()
        {
            return Ok(ImportOutcome::NotFound);
        }
        if let Some(row) = sqlx::query!(
            "SELECT id, database_id, name, position, version, import_fingerprint FROM database_tables WHERE database_id = $1 AND import_key = $2",
            database_id, request.request_id,
        ).fetch_optional(&mut *transaction).await? {
            return Ok(if row.import_fingerprint.as_deref() == Some(fingerprint) {
                ImportOutcome::Replayed(Table { id: row.id, database_id: row.database_id,
                    name: row.name, position: row.position, version: TableVersion(row.version) })
            } else { ImportOutcome::KeyConflict });
        }
        let max_position = sqlx::query_scalar!(
            "SELECT MAX(position) FROM database_tables WHERE database_id = $1",
            database_id
        )
        .fetch_one(&mut *transaction)
        .await?;
        let id = macro_uuid::generate_uuid_v7();
        let position = position_after(max_position.as_deref())?;
        let table = sqlx::query!(
            r#"INSERT INTO database_tables (id, database_id, name, position, version, import_key, import_fingerprint)
               SELECT $1, $2, $3, $4, 1, $5, $6 WHERE NOT EXISTS (
                   SELECT 1 FROM database_tables WHERE database_id = $2 AND lower(name) = lower($3))
               RETURNING id, database_id, name, position, version"#,
            id, database_id, request.name, position, request.request_id, fingerprint,
        ).fetch_optional(&mut *transaction).await?;
        let Some(table) = table else {
            return Ok(ImportOutcome::NameConflict);
        };
        let column_positions = keys_between(None, None, definitions.len())?;
        for (definition, position) in definitions.iter().zip(column_positions) {
            let column_id = macro_uuid::generate_uuid_v7();
            sqlx::query!(
                "INSERT INTO database_columns (id, table_id, property_definition_id, position, infer_type) VALUES ($1, $2, $3, $4, false)",
                column_id, id, definition, position,
            ).execute(&mut *transaction).await?;
        }
        // Rows are minted in a bounded INSERT using Postgres arrays; their
        // cells follow through the properties system once this commits.
        let mut rows = Vec::with_capacity(request.rows.len());
        let mut row_positions = keys_between(None, None, request.rows.len())?.into_iter();
        for batch in request.rows.chunks(500) {
            let row_ids: Vec<Uuid> = batch
                .iter()
                .map(|_| macro_uuid::generate_uuid_v7())
                .collect();
            let positions: Vec<String> = row_positions.by_ref().take(batch.len()).collect();
            sqlx::query!(
                r#"INSERT INTO database_rows (id, table_id, position, created_by)
                   SELECT row_id, $1, position, $2 FROM UNNEST($3::uuid[], $4::text[]) AS data(row_id, position)"#,
                id, viewer.user_id.as_ref(), &row_ids, &positions,
            ).execute(&mut *transaction).await?;
            rows.extend(row_ids);
        }
        transaction.commit().await?;
        Ok(ImportOutcome::Created {
            table: Table {
                id: table.id,
                database_id: table.database_id,
                name: table.name,
                position: table.position,
                version: TableVersion(table.version),
            },
            rows,
        })
    }
}
