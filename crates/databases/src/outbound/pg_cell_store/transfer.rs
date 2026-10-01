//! Imports: the table, its placements, its rows and their cells, committed
//! together so a retry never finds half an import.

use models_databases::position::{key_between, keys_between};
use uuid::Uuid;

use super::*;
use crate::domain::models::{DatabaseId, Table, TableVersion, Viewer};
use crate::domain::transfer::{
    DatabaseTransferRepo, ImportFingerprint, ImportOutcome, ImportTable,
};

/// Rows minted per `INSERT … UNNEST` statement.
const ROWS_PER_INSERT: usize = 500;

impl<Properties> DatabaseTransferRepo for PgCellStore<Properties>
where
    Properties: PropertiesRepo<Err = anyhow::Error>
        + DatabaseCellWriter<Transaction = Transaction<'static, Postgres>>
        + Send
        + Sync
        + 'static,
{
    type Err = PgCellStoreError;

    async fn imported_table(
        &self,
        database_id: DatabaseId,
        request_id: Uuid,
    ) -> Result<Option<(Table, ImportFingerprint)>, Self::Err> {
        let row = sqlx::query!(
            "SELECT id, database_id, name, position, version, import_fingerprint FROM database_tables WHERE database_id = $1 AND import_key = $2",
            database_id, request_id,
        ).fetch_optional(&self.pool).await?;
        row.map(|row| {
            let fingerprint = row
                .import_fingerprint
                .ok_or(PgCellStoreError::MissingImportFingerprint(row.id))?;
            Ok((
                Table {
                    id: row.id,
                    database_id: row.database_id,
                    name: row.name,
                    position: row.position,
                    version: TableVersion(row.version),
                },
                ImportFingerprint(fingerprint),
            ))
        })
        .transpose()
    }

    async fn import_table(
        &self,
        database_id: DatabaseId,
        viewer: &Viewer,
        request: &ImportTable,
        fingerprint: &ImportFingerprint,
        definitions: &[PropertyDefinitionId],
        cells: &[Vec<(PropertyDefinitionId, PropertyValue)>],
    ) -> Result<ImportOutcome, Self::Err> {
        let fingerprint = fingerprint.0.as_str();
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
        let position =
            key_between(max_position.as_deref(), None).map_err(PgDatabasesRepoError::from)?;
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
        let column_positions =
            keys_between(None, None, definitions.len()).map_err(PgDatabasesRepoError::from)?;
        for (definition, position) in definitions.iter().zip(column_positions) {
            let column_id = macro_uuid::generate_uuid_v7();
            sqlx::query!(
                "INSERT INTO database_columns (id, table_id, property_definition_id, position, infer_type) VALUES ($1, $2, $3, $4, false)",
                column_id, id, definition, position,
            ).execute(&mut *transaction).await?;
        }
        let mut rows = Vec::with_capacity(request.rows.len());
        let mut row_positions = keys_between(None, None, request.rows.len())
            .map_err(PgDatabasesRepoError::from)?
            .into_iter();
        for batch in request.rows.chunks(ROWS_PER_INSERT) {
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
        for (row, row_cells) in rows.iter().zip(cells) {
            for (definition, value) in row_cells {
                self.properties
                    .upsert_entity_property_in(
                        &mut transaction,
                        &row_entity(*row),
                        *definition,
                        Some(value.clone()),
                    )
                    .await
                    .map_err(cells_error)?;
            }
        }
        transaction.commit().await?;
        Ok(ImportOutcome::Created(Table {
            id: table.id,
            database_id: table.database_id,
            name: table.name,
            position: table.position,
            version: TableVersion(table.version),
        }))
    }
}
