use sqlx::{PgPool, Postgres, Transaction};

use super::*;

/// Open a transaction holding `table` (live, at its version) and the other
/// tables of `ids` locked for a schema change; `None` when `table` moved.
pub(crate) async fn lock_column_tables(
    pool: &PgPool,
    table: &Table,
    ids: &[TableId],
) -> Result<Option<Transaction<'static, Postgres>>, PgDatabasesRepoError> {
    let mut transaction = pool.begin().await?;
    // Parent locks precede table locks, exactly as row writes and purge do.
    let databases = sqlx::query!(
        "SELECT t.id AS table_id, d.id AS database_id, d.trashed_at
             FROM database_tables t JOIN databases d ON d.id = t.database_id
             WHERE t.id = ANY($1) ORDER BY d.id, t.id FOR SHARE OF d",
        ids
    )
    .fetch_all(&mut *transaction)
    .await?;
    if !databases.iter().any(|row| {
        row.table_id == table.id && row.database_id == table.database_id && row.trashed_at.is_none()
    }) {
        transaction.rollback().await?;
        return Ok(None);
    }
    let versions = sqlx::query!(
        "SELECT id, version FROM database_tables WHERE id = ANY($1) ORDER BY id FOR UPDATE",
        ids
    )
    .fetch_all(&mut *transaction)
    .await?;
    if !versions
        .iter()
        .any(|row| row.id == table.id && row.version == table.version.0)
    {
        transaction.rollback().await?;
        return Ok(None);
    }
    Ok(Some(transaction))
}

/// Swap a placement onto its replacement definition and store the views the
/// change rewrote, inside `transaction`; `false` when either is gone.
pub(crate) async fn rebind_placement(
    transaction: &mut Transaction<'static, Postgres>,
    table: &Table,
    replacement: &ColumnReplacement,
    views: &[DatabaseView],
) -> Result<bool, PgDatabasesRepoError> {
    let config = replacement
        .config
        .as_ref()
        .map(serde_json::to_value)
        .transpose()?;
    let changed = sqlx::query!(
        "UPDATE database_columns SET property_definition_id = $4, config = $5, infer_type = false
             WHERE id = $1 AND table_id = $2 AND property_definition_id = $3",
        replacement.column.id,
        table.id,
        replacement.column.property_definition_id,
        replacement.definition_id,
        config
    )
    .execute(&mut **transaction)
    .await?;
    Ok(changed.rows_affected() == 1 && rewrite_views(transaction, views).await?)
}

impl PgDatabasesRepo {
    pub(super) async fn delete_column_placement(
        &self,
        table: &Table,
        column: &Column,
        views: &[DatabaseView],
    ) -> Result<Option<ColumnSchemaOutcome>, PgDatabasesRepoError> {
        let mut tables = vec![table.id];
        if let Some(ColumnConfig::Link { table_id, .. }) = column.config
            && table_id != table.id
        {
            tables.push(table_id);
        }
        let Some(mut transaction) = lock_column_tables(&self.pool, table, &tables).await? else {
            return Ok(None);
        };
        let changed = sqlx::query!(
            "DELETE FROM database_columns WHERE id = $1 AND table_id = $2 AND property_definition_id = $3",
            column.id,
            table.id,
            column.property_definition_id
        )
        .execute(&mut *transaction)
        .await?;
        if changed.rows_affected() != 1 || !rewrite_views(&mut transaction, views).await? {
            transaction.rollback().await?;
            return Ok(None);
        }
        let versions = sqlx::query!(
            "UPDATE database_tables SET version = version + 1 WHERE id = ANY($1) RETURNING id, version",
            &tables
        )
        .fetch_all(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(Some(ColumnSchemaOutcome {
            table_versions: versions
                .into_iter()
                .map(|row| (row.id, TableVersion(row.version)))
                .collect(),
        }))
    }

    pub(super) async fn reorder_column_placements(
        &self,
        table: &Table,
        ids: &[ColumnId],
    ) -> Result<Option<TableVersion>, PgDatabasesRepoError> {
        let Some(mut transaction) = lock_column_tables(&self.pool, table, &[table.id]).await?
        else {
            return Ok(None);
        };
        for (id, position) in ids.iter().zip(keys_between(None, None, ids.len())?) {
            let result = sqlx::query!(
                "UPDATE database_columns SET position = $3 WHERE id = $1 AND table_id = $2",
                id,
                table.id,
                position
            )
            .execute(&mut *transaction)
            .await?;
            if result.rows_affected() != 1 {
                transaction.rollback().await?;
                return Ok(None);
            }
        }
        let version = rows::bump_table_version(&mut *transaction, table.id).await?;
        transaction.commit().await?;
        Ok(Some(version))
    }
}

/// Store views rewritten by a schema change; `false` when one is gone.
async fn rewrite_views(
    transaction: &mut Transaction<'_, Postgres>,
    views: &[DatabaseView],
) -> Result<bool, PgDatabasesRepoError> {
    for view in views {
        if !views::update_view(&mut **transaction, view).await? {
            return Ok(false);
        }
    }
    Ok(true)
}
