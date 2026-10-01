//! Postgres repository for databases, tables, columns and row identities;
//! cells are entity properties, written through the properties adapter.

/// Schema-change statements, shared with the cell store's column rebind.
pub(crate) mod columns;
mod delete_table;
mod reorder_tables;
/// Row identity statements, shared with the cell store's batches.
pub(crate) mod rows;
mod saved_queries;
mod sharing;
#[cfg(test)]
mod test;
/// View and card-place statements, shared with the cell store's batches.
pub(crate) mod views;

use std::collections::HashMap;

use models_databases::position::{PositionError, key_between, keys_between};

use macro_user_id::user_id::MacroUserIdStr;
use models_properties::DataType;
use properties::domain::database_definition_writer::{
    DatabaseDefinitionWriter, NewDatabaseDefinition,
};
use sqlx::{PgPool, Postgres, Transaction};

use entity_access_db_utils::{AccessLevel, EntityAccessSourceType};
use model_entity::EntityType;

use crate::domain::models::{
    CardPosition, ColumnReplacement, ColumnSchemaOutcome, DatabaseView, ViewId,
};
use crate::domain::models::{
    Column, ColumnConfig, ColumnId, CreateColumn, CreateDatabase, CreateTable, Database,
    DatabaseId, FirstTable, PropertyDefinitionId, RenameColumnOutcome, RowRef, Table, TableId,
    TableMutationOutcome, TableVersion,
};
use crate::domain::models::{
    QueryDefinition, QueryId, SavedQuery, TableDeletion, TableOrderOutcome,
};
use crate::domain::ports::DatabasesRepo;

/// Errors from the Postgres repository.
#[derive(Debug, thiserror::Error)]
pub enum PgDatabasesRepoError {
    /// Underlying database failure.
    #[error("database error")]
    Sqlx(#[from] sqlx::Error),
    /// A domain value could not be encoded as JSON for storage, or a stored
    /// one decoded.
    #[error("failed to encode or decode stored json")]
    Json(#[from] serde_json::Error),
    /// A stored position is not a fractional key.
    #[error("stored position")]
    Position(#[from] PositionError),
    /// A stored card lane is neither empty nor an option id.
    #[error("stored card lane `{0}` is not an option id")]
    CorruptLane(String),
    /// The properties domain refused or failed a write.
    #[error("properties write failed: {0}")]
    Properties(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// A `database_tables` row, read by `query_as!` and mapped onto [`Table`].
pub(crate) struct TableRecord {
    pub(crate) id: TableId,
    pub(crate) database_id: DatabaseId,
    pub(crate) name: String,
    pub(crate) position: String,
    pub(crate) version: i64,
}

impl From<TableRecord> for Table {
    fn from(record: TableRecord) -> Self {
        Self {
            id: record.id,
            database_id: record.database_id,
            name: record.name,
            position: record.position,
            version: TableVersion(record.version),
        }
    }
}

/// The position that appends after `last`, the largest one a list has, or
/// starts an empty list. Positions are fractional keys compared as bytes
/// (the columns are `COLLATE "C"`), so the largest is the last.
fn position_after(last: Option<&str>) -> Result<String, PositionError> {
    key_between(last, None)
}

/// Insert a database, its first table and its owner's grant inside
/// `transaction`, so no database can exist that nobody can open.
pub(crate) async fn insert_owned_database(
    transaction: &mut Transaction<'static, Postgres>,
    database_id: DatabaseId,
    name: &str,
    owner_id: &str,
    table_id: TableId,
    table_name: &str,
) -> Result<Database, PgDatabasesRepoError> {
    let database = sqlx::query_as!(
        Database,
        r#"
            INSERT INTO databases (id, name, owner_id)
            VALUES ($1, $2, $3)
            RETURNING id, name, owner_id, created_at, trashed_at
            "#,
        database_id,
        name,
        owner_id,
    )
    .fetch_one(&mut **transaction)
    .await?;
    sqlx::query!(
        r#"
            INSERT INTO database_tables (id, database_id, name, position)
            VALUES ($1, $2, $3, $4)
            "#,
        table_id,
        database_id,
        table_name,
        position_after(None)?,
    )
    .execute(&mut **transaction)
    .await?;
    entity_access_db_utils::insert_entity_access_row(
        transaction,
        &database_id,
        EntityType::Database,
        owner_id,
        EntityAccessSourceType::User,
        AccessLevel::Owner,
    )
    .await?;
    Ok(database)
}

/// Insert a column placement bound to `definition_id` inside `transaction`.
pub(crate) async fn insert_column(
    transaction: &mut Transaction<'static, Postgres>,
    column_id: ColumnId,
    table_id: TableId,
    definition_id: PropertyDefinitionId,
    position: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO database_columns (id, table_id, property_definition_id, position, infer_type) VALUES ($1, $2, $3, $4, false)",
        column_id,
        table_id,
        definition_id,
        position,
    )
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

/// [`DatabasesRepo`] backed by MacroDB; a new database's title definition is
/// written through the properties domain, in the same transaction.
#[derive(Debug, Clone)]
pub struct PgDatabasesRepo<Properties> {
    pool: PgPool,
    properties: Properties,
}

impl<Properties> PgDatabasesRepo<Properties> {
    /// Create a repository over the pool and the properties writer.
    pub fn new(pool: PgPool, properties: Properties) -> Self {
        Self { pool, properties }
    }
}

impl<Properties> DatabasesRepo for PgDatabasesRepo<Properties>
where
    Properties: DatabaseDefinitionWriter<Transaction = Transaction<'static, Postgres>>
        + Send
        + Sync
        + 'static,
{
    type Error = PgDatabasesRepoError;

    #[tracing::instrument(err, skip(self, table, column, views))]
    async fn delete_column(
        &self,
        table: &Table,
        column: &Column,
        views: &[DatabaseView],
    ) -> Result<Option<ColumnSchemaOutcome>, Self::Error> {
        self.delete_column_placement(table, column, views).await
    }

    #[tracing::instrument(err, skip(self))]
    async fn views_for_tables(
        &self,
        table_ids: &[TableId],
    ) -> Result<Vec<DatabaseView>, Self::Error> {
        views::views_for_tables(&self.pool, table_ids).await
    }

    #[tracing::instrument(err, skip(self))]
    async fn view_positions(&self, view_id: ViewId) -> Result<Vec<CardPosition>, Self::Error> {
        views::view_positions(&self.pool, view_id).await
    }

    #[tracing::instrument(err, skip(self, table))]
    async fn reorder_columns(
        &self,
        table: &Table,
        column_ids: &[ColumnId],
    ) -> Result<Option<TableVersion>, Self::Error> {
        self.reorder_column_placements(table, column_ids).await
    }

    #[tracing::instrument(err, skip(self, table))]
    async fn delete_table(&self, table: &Table) -> Result<TableDeletion, Self::Error> {
        self.delete_table_and_rows(table).await
    }

    #[tracing::instrument(err, skip(self, definition))]
    async fn save_query(
        &self,
        database_id: Option<DatabaseId>,
        definition: &QueryDefinition,
        created_by: &MacroUserIdStr<'_>,
    ) -> Result<SavedQuery, Self::Error> {
        self.insert_query(database_id, definition, created_by.as_ref())
            .await
    }

    #[tracing::instrument(err, skip(self))]
    async fn get_query(&self, id: QueryId) -> Result<Option<SavedQuery>, Self::Error> {
        self.query_by_id(id).await
    }

    #[tracing::instrument(err, skip(self, command))]
    async fn create_database(
        &self,
        command: &CreateDatabase,
        first_table: FirstTable,
    ) -> Result<Database, Self::Error> {
        let mut transaction = self.pool.begin().await?;
        let table_id = macro_uuid::generate_uuid_v7();
        let database = insert_owned_database(
            &mut transaction,
            macro_uuid::generate_uuid_v7(),
            &command.name,
            command.owner_id.as_ref(),
            table_id,
            first_table.name,
        )
        .await?;
        let title = self
            .properties
            .create_database_definition_in(
                &mut transaction,
                NewDatabaseDefinition {
                    database_id: database.id,
                    name: first_table.title_column,
                    data_type: DataType::String,
                    is_multi_select: false,
                    specific_entity_type: None,
                    options: &[],
                },
            )
            .await
            .map_err(|error| PgDatabasesRepoError::Properties(Box::new(error)))?;
        insert_column(
            &mut transaction,
            macro_uuid::generate_uuid_v7(),
            table_id,
            title.definition.id,
            &position_after(None)?,
        )
        .await?;
        transaction.commit().await?;
        Ok(database)
    }

    /// Returns the row whether or not it is trashed; the domain decides what a
    /// trashed database means.
    #[tracing::instrument(err, skip(self))]
    async fn get_database(
        &self,
        id: DatabaseId,
    ) -> Result<Option<(Database, Vec<Table>)>, Self::Error> {
        let Some(database) = sqlx::query_as!(
            Database,
            r#"SELECT id, name, owner_id, created_at, trashed_at FROM databases WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        let tables = sqlx::query_as!(
            TableRecord,
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
        .map(Table::from)
        .collect();

        Ok(Some((database, tables)))
    }

    #[tracing::instrument(err, skip(self))]
    async fn rename_database(&self, id: DatabaseId, name: &str) -> Result<bool, Self::Error> {
        let renamed = sqlx::query!(
            r#"UPDATE databases SET name = $2, updated_at = now() WHERE id = $1"#,
            id,
            name,
        )
        .execute(&self.pool)
        .await?;
        Ok(renamed.rows_affected() == 1)
    }

    #[tracing::instrument(err, skip(self))]
    async fn trash_database(
        &self,
        id: DatabaseId,
        trashed_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool, Self::Error> {
        let trashed = sqlx::query!(
            r#"UPDATE databases SET trashed_at = $2, updated_at = now() WHERE id = $1"#,
            id,
            trashed_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(trashed.rows_affected() == 1)
    }

    #[tracing::instrument(err, skip(self))]
    async fn restore_database(&self, id: DatabaseId) -> Result<bool, Self::Error> {
        let restored = sqlx::query!(
            r#"UPDATE databases SET trashed_at = NULL, updated_at = now() WHERE id = $1"#,
            id,
        )
        .execute(&self.pool)
        .await?;
        Ok(restored.rows_affected() == 1)
    }

    /// Tables, columns, rows, views and database-owned property definitions
    /// go with the database through `ON DELETE CASCADE`, and the rows' cells
    /// by trigger; `entity_access` rows are a generic side table with no
    /// foreign key to `databases`, so they are purged explicitly in the same
    /// transaction.
    #[tracing::instrument(err, skip(self))]
    async fn delete_database(&self, id: DatabaseId) -> Result<(), Self::Error> {
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

    #[tracing::instrument(err, skip(self, command))]
    async fn create_table(
        &self,
        command: &CreateTable,
    ) -> Result<TableMutationOutcome, Self::Error> {
        let mut transaction = self.pool.begin().await?;
        if !rows::lock_live_database(&mut *transaction, command.database_id).await? {
            return Ok(TableMutationOutcome::NotFound);
        }

        let max_position = sqlx::query_scalar!(
            r#"SELECT MAX(position) FROM database_tables WHERE database_id = $1"#,
            command.database_id
        )
        .fetch_one(&mut *transaction)
        .await?;
        let position = position_after(max_position.as_deref())?;
        let id = macro_uuid::generate_uuid_v7();

        let table = sqlx::query_as!(
            TableRecord,
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
            command.database_id,
            command.name,
            position,
        )
        .fetch_optional(&mut *transaction)
        .await?;

        transaction.commit().await?;
        Ok(table.map_or(TableMutationOutcome::Conflict, |table| {
            TableMutationOutcome::Applied(table.into())
        }))
    }

    #[tracing::instrument(err, skip(self, table))]
    async fn rename_table(
        &self,
        table: &Table,
        name: &str,
        previous_name: &str,
    ) -> Result<TableMutationOutcome, Self::Error> {
        let mut transaction = self.pool.begin().await?;
        // Serialize table naming and position allocation within a database.
        if !rows::lock_live_database(&mut *transaction, table.database_id).await? {
            return Ok(TableMutationOutcome::NotFound);
        }
        let renamed = sqlx::query_as!(
            TableRecord,
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
        Ok(renamed.map_or(TableMutationOutcome::Conflict, |table| {
            TableMutationOutcome::Applied(table.into())
        }))
    }

    #[tracing::instrument(err, skip(self))]
    async fn reorder_tables(
        &self,
        database_id: DatabaseId,
        table_ids: &[TableId],
    ) -> Result<TableOrderOutcome, Self::Error> {
        self.rewrite_table_positions(database_id, table_ids).await
    }

    #[tracing::instrument(err, skip(self, command))]
    async fn create_column(
        &self,
        table_id: TableId,
        property_definition_id: PropertyDefinitionId,
        command: &CreateColumn,
    ) -> Result<(ColumnId, TableVersion), Self::Error> {
        let config = command
            .config
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?;

        let mut transaction = self.pool.begin().await?;

        let max_position = sqlx::query_scalar!(
            r#"SELECT MAX(position) FROM database_columns WHERE table_id = $1"#,
            table_id
        )
        .fetch_one(&mut *transaction)
        .await?;
        let position = position_after(max_position.as_deref())?;
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
            command.infer_type,
        )
        .execute(&mut *transaction)
        .await?;

        // A new column changes the table's shape.
        let version = rows::bump_table_version(&mut *transaction, table_id).await?;
        transaction.commit().await?;
        Ok((id, version))
    }

    #[tracing::instrument(err, skip(self, table, column))]
    async fn rename_column(
        &self,
        table: &Table,
        column: &Column,
        name: &str,
    ) -> Result<Option<RenameColumnOutcome>, Self::Error> {
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
    ) -> Result<Option<TableVersion>, Self::Error> {
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
        let version = rows::bump_table_version(&mut *transaction, table.id).await?;
        transaction.commit().await?;
        Ok(Some(version))
    }

    #[tracing::instrument(err, skip(self))]
    async fn row_refs(&self, table_id: TableId) -> Result<Vec<RowRef>, Self::Error> {
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

    #[tracing::instrument(skip(self), err)]
    async fn databases_by_ids(&self, ids: &[DatabaseId]) -> Result<Vec<Database>, Self::Error> {
        Ok(sqlx::query_as!(
            Database,
            r#"
            SELECT id, name, owner_id, created_at, trashed_at
            FROM databases
            WHERE id = ANY($1)
            ORDER BY created_at
            "#,
            ids,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    #[tracing::instrument(skip(self), err)]
    async fn tables_for_databases(
        &self,
        database_ids: &[DatabaseId],
    ) -> Result<Vec<Table>, Self::Error> {
        let tables = sqlx::query_as!(
            TableRecord,
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
        Ok(tables.into_iter().map(Table::from).collect())
    }

    #[tracing::instrument(skip(self), err)]
    async fn columns_for_tables(&self, table_ids: &[TableId]) -> Result<Vec<Column>, Self::Error> {
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
            .map(|row| {
                Ok(Column {
                    id: row.id,
                    table_id: row.table_id,
                    property_definition_id: row.property_definition_id,
                    position: row.position,
                    config: row.config.map(serde_json::from_value).transpose()?,
                    display_name: row.display_name,
                    infer_type: row.infer_type,
                })
            })
            .collect()
    }

    #[tracing::instrument(err, skip(self))]
    async fn table_versions(
        &self,
        table_ids: &[TableId],
    ) -> Result<HashMap<TableId, TableVersion>, Self::Error> {
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
