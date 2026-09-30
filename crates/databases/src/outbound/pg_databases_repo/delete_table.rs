use super::*;
use crate::domain::models::TableDeletion;

impl PgDatabasesRepo {
    pub(super) async fn delete_table_and_rows(
        &self,
        table: &Table,
    ) -> Result<TableDeletion, PgDatabasesRepoError> {
        let mut transaction = self.pool.begin().await?;
        // The database lock serializes table creation, renames and deletes,
        // so two concurrent deletes cannot both see a second table.
        if sqlx::query!(
            "SELECT id FROM databases WHERE id = $1 AND trashed_at IS NULL FOR UPDATE",
            table.database_id
        )
        .fetch_optional(&mut *transaction)
        .await?
        .is_none()
        {
            return Ok(TableDeletion::NotFound);
        }
        let tables = sqlx::query_scalar!(
            "SELECT id FROM database_tables WHERE database_id = $1",
            table.database_id
        )
        .fetch_all(&mut *transaction)
        .await?;
        if !tables.contains(&table.id) {
            return Ok(TableDeletion::NotFound);
        }
        if tables.len() <= 1 {
            return Ok(TableDeletion::LastTable);
        }
        let row_ids = sqlx::query_scalar!(
            "DELETE FROM database_rows WHERE table_id = $1 RETURNING id",
            table.id
        )
        .fetch_all(&mut *transaction)
        .await?;
        // Column placements go with the table through their foreign key.
        let deleted = sqlx::query!(
            "DELETE FROM database_tables WHERE id = $1 AND database_id = $2",
            table.id,
            table.database_id
        )
        .execute(&mut *transaction)
        .await?;
        if deleted.rows_affected() != 1 {
            transaction.rollback().await?;
            return Ok(TableDeletion::NotFound);
        }
        transaction.commit().await?;
        Ok(TableDeletion::Deleted { row_ids })
    }
}
