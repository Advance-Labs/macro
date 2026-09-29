//! Postgres repository for databases, tables, columns and row identities.
//!
//! Mechanics only: sqlx queries and transactions. Policy lives in the domain
//! service. Tables: `databases`, `database_tables`, `database_columns`,
//! `database_rows`. Cells are not here: they are entity properties, written
//! through the properties adapter.

mod columns;
mod sharing;
#[cfg(test)]
mod test;
mod transfer;

use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

use entity_access_db_utils::{AccessLevel, EntityAccessSourceType};
use model_entity::EntityType;

use crate::domain::models::{
    Column, ColumnConfig, ColumnId, CreateColumn, CreateDatabase, CreateTable, Database,
    DatabaseId, PropertyDefinitionId, RenameColumnOutcome, RowId, RowRef, Table, TableId,
    TableMutationOutcome, TableVersion,
};
use crate::domain::models::{ColumnReplacement, ColumnSchemaOutcome};
use crate::domain::ports::DatabasesRepo;

/// Errors from the Postgres repository.
#[derive(Debug, thiserror::Error)]
pub enum PgDatabasesRepoError {
    /// Underlying database failure.
    #[error("database error")]
    Sqlx(#[from] sqlx::Error),
    /// A domain value could not be encoded as JSON for storage.
    #[error("failed to encode json for storage")]
    Json(#[from] serde_json::Error),
}

/// Width of the zero-padded decimal ordering keys stored in `position`.
///
/// There is no fractional-index helper in the workspace yet, so ordering keys
/// are plain zero-padded counters: a new item's position is
/// `max(position) + 1`, rendered to a fixed width so lexicographic ordering
/// (what the `TEXT` column and every `ORDER BY position` give us) matches
/// numeric ordering. Inserting *between* two neighbours is therefore not
/// expressible yet; when it is needed, swap [`next_position`] for a real
/// fractional index — the column is already `TEXT` and every read orders by it,
/// so nothing else has to change.
const POSITION_WIDTH: usize = 12;

/// The ordering key that appends after `max`, the largest existing position.
fn next_position(max: Option<&str>) -> String {
    let next = max.and_then(|p| p.parse::<u64>().ok()).unwrap_or(0) + 1;
    format!("{next:0POSITION_WIDTH$}")
}

/// [`DatabasesRepo`] backed by MacroDB.
#[derive(Debug, Clone)]
pub struct PgDatabasesRepo {
    pool: PgPool,
}

impl PgDatabasesRepo {
    /// Create a repository over the given pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl DatabasesRepo for PgDatabasesRepo {
    type Err = PgDatabasesRepoError;

    async fn replace_column(
        &self,
        table: &Table,
        replacement: &ColumnReplacement,
    ) -> Result<Option<TableVersion>, Self::Err> {
        self.replace_column_placement(table, replacement).await
    }

    async fn delete_column(
        &self,
        table: &Table,
        column: &Column,
    ) -> Result<Option<ColumnSchemaOutcome>, Self::Err> {
        self.delete_column_placement(table, column).await
    }

    async fn reorder_columns(
        &self,
        table: &Table,
        column_ids: &[ColumnId],
    ) -> Result<Option<TableVersion>, Self::Err> {
        self.reorder_column_placements(table, column_ids).await
    }

    #[tracing::instrument(err, skip(self, cmd))]
    async fn create_database(
        &self,
        cmd: &CreateDatabase,
        starter_table_name: &str,
    ) -> Result<Database, Self::Err> {
        // Time-ordered v7 so ids sort by creation and are known before insert.
        let id = macro_uuid::generate_uuid_v7();
        let mut transaction = self.pool.begin().await?;

        let row = sqlx::query!(
            r#"
            INSERT INTO databases (id, name, owner_id)
            VALUES ($1, $2, $3)
            RETURNING id, name, owner_id, created_at, trashed_at
            "#,
            id,
            cmd.name,
            cmd.owner_id.as_ref(),
        )
        .fetch_one(&mut *transaction)
        .await?;

        sqlx::query!(
            r#"
            INSERT INTO database_tables (id, database_id, name, position)
            VALUES ($1, $2, $3, $4)
            "#,
            macro_uuid::generate_uuid_v7(),
            id,
            starter_table_name,
            next_position(None),
        )
        .execute(&mut *transaction)
        .await?;

        // The creator's owner grant lives in the same transaction, so a
        // database can never exist that nobody can open.
        entity_access_db_utils::insert_entity_access_row(
            &mut transaction,
            &id,
            EntityType::Database,
            cmd.owner_id.as_ref(),
            EntityAccessSourceType::User,
            AccessLevel::Owner,
        )
        .await?;

        transaction.commit().await?;

        Ok(Database {
            id: row.id,
            name: row.name,
            owner_id: row.owner_id,
            created_at: row.created_at,
            trashed_at: row.trashed_at,
        })
    }

    /// Returns the row whether or not it is trashed; the domain decides what a
    /// trashed database means.
    #[tracing::instrument(err, skip(self))]
    async fn get_database(
        &self,
        id: DatabaseId,
    ) -> Result<Option<(Database, Vec<Table>)>, Self::Err> {
        let Some(row) = sqlx::query!(
            r#"SELECT id, name, owner_id, created_at, trashed_at FROM databases WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        let database = Database {
            id: row.id,
            name: row.name,
            owner_id: row.owner_id,
            created_at: row.created_at,
            trashed_at: row.trashed_at,
        };

        let tables = sqlx::query!(
            r#"
            SELECT id, database_id, name, position, version
            FROM database_tables
            WHERE database_id = $1
            ORDER BY position
            "#,
            id
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| Table {
            id: row.id,
            database_id: row.database_id,
            name: row.name,
            position: row.position,
            version: TableVersion(row.version),
        })
        .collect();

        Ok(Some((database, tables)))
    }

    #[tracing::instrument(err, skip(self))]
    async fn rename_database(&self, id: DatabaseId, name: &str) -> Result<(), Self::Err> {
        sqlx::query!(
            r#"UPDATE databases SET name = $2, updated_at = now() WHERE id = $1"#,
            id,
            name,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(err, skip(self))]
    async fn trash_database(
        &self,
        id: DatabaseId,
        trashed_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), Self::Err> {
        sqlx::query!(
            r#"UPDATE databases SET trashed_at = $2, updated_at = now() WHERE id = $1"#,
            id,
            trashed_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(err, skip(self))]
    async fn restore_database(&self, id: DatabaseId) -> Result<(), Self::Err> {
        sqlx::query!(
            r#"UPDATE databases SET trashed_at = NULL, updated_at = now() WHERE id = $1"#,
            id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Tables, columns, rows, link edges, and database-owned property
    /// definitions go with the database through the `ON DELETE CASCADE` chain
    /// declared in the migrations that added them; `entity_access`
    /// rows are a generic side table with no foreign key to `databases`, so
    /// they are purged explicitly in the same transaction.
    #[tracing::instrument(err, skip(self))]
    async fn delete_database(&self, id: DatabaseId) -> Result<(), Self::Err> {
        let mut transaction = self.pool.begin().await?;

        entity_access_db_utils::delete_entity_access_rows(
            &mut transaction,
            &id,
            EntityType::Database,
        )
        .await?;

        sqlx::query!(r#"DELETE FROM databases WHERE id = $1"#, id)
            .execute(&mut *transaction)
            .await?;

        transaction.commit().await?;
        Ok(())
    }

    #[tracing::instrument(err, skip(self, cmd))]
    async fn create_table(&self, cmd: &CreateTable) -> Result<TableMutationOutcome, Self::Err> {
        let mut transaction = self.pool.begin().await?;

        if sqlx::query!(
            "SELECT id FROM databases WHERE id = $1 AND trashed_at IS NULL FOR UPDATE",
            cmd.database_id
        )
        .fetch_optional(&mut *transaction)
        .await?
        .is_none()
        {
            return Ok(TableMutationOutcome::NotFound);
        }

        let max_position = sqlx::query_scalar!(
            r#"SELECT MAX(position) FROM database_tables WHERE database_id = $1"#,
            cmd.database_id
        )
        .fetch_one(&mut *transaction)
        .await?;
        let position = next_position(max_position.as_deref());
        let id = macro_uuid::generate_uuid_v7();

        let row = sqlx::query!(
            r#"
            INSERT INTO database_tables (id, database_id, name, position)
            SELECT $1, $2, $3, $4
            WHERE NOT EXISTS (
                SELECT 1 FROM database_tables
                WHERE database_id = $2 AND lower(name) = lower($3)
            )
            RETURNING id, database_id, name, position, version
            "#,
            id,
            cmd.database_id,
            cmd.name,
            position,
        )
        .fetch_optional(&mut *transaction)
        .await?;

        transaction.commit().await?;

        Ok(row
            .map(|row| {
                TableMutationOutcome::Applied(Table {
                    id: row.id,
                    database_id: row.database_id,
                    name: row.name,
                    position: row.position,
                    version: TableVersion(row.version),
                })
            })
            .unwrap_or(TableMutationOutcome::Conflict))
    }

    #[tracing::instrument(err, skip(self, table))]
    async fn rename_table(
        &self,
        table: &Table,
        name: &str,
        previous_name: &str,
    ) -> Result<TableMutationOutcome, Self::Err> {
        let mut transaction = self.pool.begin().await?;
        // Serialize table naming and position allocation within a database.
        if sqlx::query!(
            "SELECT id FROM databases WHERE id = $1 AND trashed_at IS NULL FOR UPDATE",
            table.database_id
        )
        .fetch_optional(&mut *transaction)
        .await?
        .is_none()
        {
            return Ok(TableMutationOutcome::NotFound);
        }
        let row = sqlx::query!(
            r#"
            UPDATE database_tables
            SET name = $3, version = version + 1
            WHERE id = $1 AND database_id = $2 AND name = $4
              AND NOT EXISTS (
                SELECT 1 FROM database_tables other
                WHERE other.database_id = $2 AND other.id <> $1
                  AND lower(other.name) = lower($3)
              )
            RETURNING id, database_id, name, position, version
            "#,
            table.id,
            table.database_id,
            name,
            previous_name,
        )
        .fetch_optional(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(row
            .map(|row| {
                TableMutationOutcome::Applied(Table {
                    id: row.id,
                    database_id: row.database_id,
                    name: row.name,
                    position: row.position,
                    version: TableVersion(row.version),
                })
            })
            .unwrap_or(TableMutationOutcome::Conflict))
    }

    #[tracing::instrument(err, skip(self, cmd))]
    async fn create_column(
        &self,
        table_id: TableId,
        property_definition_id: PropertyDefinitionId,
        cmd: &CreateColumn,
    ) -> Result<ColumnId, Self::Err> {
        let config = cmd.config.as_ref().map(serde_json::to_value).transpose()?;

        let mut transaction = self.pool.begin().await?;

        let max_position = sqlx::query_scalar!(
            r#"SELECT MAX(position) FROM database_columns WHERE table_id = $1"#,
            table_id
        )
        .fetch_one(&mut *transaction)
        .await?;
        let position = next_position(max_position.as_deref());
        let id = macro_uuid::generate_uuid_v7();

        sqlx::query!(
            r#"
            INSERT INTO database_columns (id, table_id, property_definition_id, position, config, infer_type)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            id,
            table_id,
            property_definition_id,
            position,
            config,
            cmd.infer_type,
        )
        .execute(&mut *transaction)
        .await?;

        // A new column changes the table's shape, so materializations keyed on
        // the version have to be rebuilt.
        sqlx::query!(
            r#"UPDATE database_tables SET version = version + 1 WHERE id = $1"#,
            table_id
        )
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;

        Ok(id)
    }

    #[tracing::instrument(err, skip(self, table, column))]
    async fn rename_column(
        &self,
        table: &Table,
        column: &Column,
        name: &str,
    ) -> Result<Option<RenameColumnOutcome>, Self::Err> {
        let mut transaction = self.pool.begin().await?;
        let version = sqlx::query_scalar!(
            r#"UPDATE database_tables SET version = version + 1
            WHERE id = $1 AND database_id = $2 AND version = $3
              AND EXISTS (SELECT 1 FROM database_columns WHERE id = $4 AND table_id = $1
                          AND display_name IS NOT DISTINCT FROM $5)
              AND EXISTS (SELECT 1 FROM databases WHERE id = $2 AND trashed_at IS NULL)
            RETURNING version"#,
            table.id,
            table.database_id,
            table.version.0,
            column.id,
            column.display_name,
        )
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(version) = version else {
            return Ok(None);
        };
        sqlx::query!(
            "UPDATE database_columns SET display_name = $2 WHERE id = $1 AND table_id = $3",
            column.id,
            name,
            table.id
        )
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(Some(RenameColumnOutcome {
            column: Column {
                display_name: Some(name.to_string()),
                ..column.clone()
            },
            table_version: TableVersion(version),
        }))
    }

    #[tracing::instrument(err, skip(self, table, column))]
    async fn infer_column_type(
        &self,
        table: &Table,
        column: &Column,
        definition_id: PropertyDefinitionId,
    ) -> Result<Option<TableVersion>, Self::Err> {
        let mut transaction = self.pool.begin().await?;
        // Row writers take this same lock before checking versions and cells.
        let current = sqlx::query_scalar!(
            "SELECT version FROM database_tables WHERE id = $1 AND database_id = $2 FOR UPDATE",
            table.id,
            table.database_id,
        )
        .fetch_optional(&mut *transaction)
        .await?;
        if current != Some(table.version.0) {
            return Ok(None);
        }
        let updated = sqlx::query_scalar!(
            r#"UPDATE database_columns SET property_definition_id = $4, infer_type = FALSE
            WHERE id = $1 AND table_id = $2 AND property_definition_id = $3 AND infer_type
              AND NOT EXISTS (
                  SELECT 1 FROM entity_properties p
                  JOIN database_rows r ON r.id::text = p.entity_id
                  WHERE r.table_id = $2 AND p.property_definition_id = $3
                    AND p.entity_type = 'DATABASE_ROW')
              AND EXISTS (SELECT 1 FROM databases WHERE id = $5 AND trashed_at IS NULL)
            RETURNING id"#,
            column.id,
            table.id,
            column.property_definition_id,
            definition_id,
            table.database_id,
        )
        .fetch_optional(&mut *transaction)
        .await?;
        if updated.is_none() {
            return Ok(None);
        }
        let version = sqlx::query_scalar!(
            "UPDATE database_tables SET version = version + 1 WHERE id = $1 RETURNING version",
            table.id,
        )
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(Some(TableVersion(version)))
    }

    #[tracing::instrument(err, skip(self))]
    async fn row_refs(&self, table_id: TableId) -> Result<Vec<RowRef>, Self::Err> {
        let rows = sqlx::query!(
            "SELECT id, position FROM database_rows WHERE table_id = $1 ORDER BY position, id",
            table_id,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| RowRef {
                id: row.id,
                position: row.position,
            })
            .collect())
    }

    #[tracing::instrument(err, skip(self))]
    async fn insert_rows(
        &self,
        table_id: TableId,
        created_by: &str,
        count: usize,
    ) -> Result<Option<Vec<RowRef>>, Self::Err> {
        let mut transaction = self.pool.begin().await?;
        // Row writers and schema writers serialize on the table's version
        // row, so positions are minted under the same lock.
        let live = sqlx::query_scalar!(
            r#"SELECT t.id FROM database_tables t
               JOIN databases d ON d.id = t.database_id
               WHERE t.id = $1 AND d.trashed_at IS NULL FOR UPDATE OF t"#,
            table_id,
        )
        .fetch_optional(&mut *transaction)
        .await?;
        if live.is_none() {
            return Ok(None);
        }
        let max_position = sqlx::query_scalar!(
            "SELECT MAX(position) FROM database_rows WHERE table_id = $1",
            table_id
        )
        .fetch_one(&mut *transaction)
        .await?;
        let mut last = max_position;
        let mut rows = Vec::with_capacity(count);
        for _ in 0..count {
            let position = next_position(last.as_deref());
            let id = macro_uuid::generate_uuid_v7();
            sqlx::query!(
                "INSERT INTO database_rows (id, table_id, position, created_by) VALUES ($1, $2, $3, $4)",
                id,
                table_id,
                position,
                created_by,
            )
            .execute(&mut *transaction)
            .await?;
            rows.push(RowRef {
                id,
                position: position.clone(),
            });
            last = Some(position);
        }
        transaction.commit().await?;
        Ok(Some(rows))
    }

    #[tracing::instrument(err, skip(self))]
    async fn delete_row(&self, table_id: TableId, row_id: RowId) -> Result<bool, Self::Err> {
        let deleted = sqlx::query!(
            "DELETE FROM database_rows WHERE id = $1 AND table_id = $2",
            row_id,
            table_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(deleted.rows_affected() == 1)
    }

    #[tracing::instrument(err, skip(self))]
    async fn row_table(&self, row_id: RowId) -> Result<Option<TableId>, Self::Err> {
        Ok(sqlx::query_scalar!(
            "SELECT table_id FROM database_rows WHERE id = $1",
            row_id
        )
        .fetch_optional(&self.pool)
        .await?)
    }

    #[tracing::instrument(err, skip(self, definitions))]
    async fn settle_inference(
        &self,
        table_id: TableId,
        definitions: &[PropertyDefinitionId],
    ) -> Result<(), Self::Err> {
        if definitions.is_empty() {
            return Ok(());
        }
        sqlx::query!(
            "UPDATE database_columns SET infer_type = FALSE
             WHERE infer_type AND table_id = $1 AND property_definition_id = ANY($2)",
            table_id,
            definitions,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip(self), err)]
    async fn databases_by_ids(&self, ids: &[DatabaseId]) -> Result<Vec<Database>, Self::Err> {
        let rows = sqlx::query!(
            r#"
            SELECT id, name, owner_id, created_at, trashed_at
            FROM databases
            WHERE id = ANY($1)
            ORDER BY created_at
            "#,
            ids,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| Database {
                id: r.id,
                name: r.name,
                owner_id: r.owner_id,
                created_at: r.created_at,
                trashed_at: r.trashed_at,
            })
            .collect())
    }

    #[tracing::instrument(skip(self), err)]
    async fn tables_for_databases(
        &self,
        database_ids: &[DatabaseId],
    ) -> Result<Vec<Table>, Self::Err> {
        let rows = sqlx::query!(
            r#"
            SELECT id, database_id, name, position, version
            FROM database_tables
            WHERE database_id = ANY($1)
            ORDER BY database_id, position
            "#,
            database_ids,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| Table {
                id: r.id,
                database_id: r.database_id,
                name: r.name,
                position: r.position,
                version: TableVersion(r.version),
            })
            .collect())
    }

    #[tracing::instrument(skip(self), err)]
    async fn columns_for_tables(&self, table_ids: &[TableId]) -> Result<Vec<Column>, Self::Err> {
        let rows = sqlx::query!(
            r#"
            SELECT id, table_id, property_definition_id, position, config, display_name, infer_type
            FROM database_columns
            WHERE table_id = ANY($1)
            ORDER BY table_id, position
            "#,
            table_ids,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(Column {
                    id: r.id,
                    table_id: r.table_id,
                    property_definition_id: r.property_definition_id,
                    position: r.position,
                    config: r.config.map(serde_json::from_value).transpose()?,
                    display_name: r.display_name,
                    infer_type: r.infer_type,
                })
            })
            .collect()
    }

    #[tracing::instrument(err, skip(self))]
    async fn bump_table_version(&self, table_id: TableId) -> Result<TableVersion, Self::Err> {
        let version = sqlx::query_scalar!(
            r#"UPDATE database_tables SET version = version + 1 WHERE id = $1 RETURNING version"#,
            table_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(TableVersion(version))
    }

    async fn table_versions(
        &self,
        table_ids: &[TableId],
    ) -> Result<HashMap<TableId, TableVersion>, Self::Err> {
        let versions = sqlx::query!(
            r#"SELECT id, version FROM database_tables WHERE id = ANY($1)"#,
            table_ids
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| (row.id, TableVersion(row.version)))
        .collect();

        Ok(versions)
    }
}
