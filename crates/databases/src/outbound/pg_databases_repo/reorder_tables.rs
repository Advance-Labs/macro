use super::*;

impl PgDatabasesRepo {
    pub(super) async fn rewrite_table_positions(
        &self,
        database_id: DatabaseId,
        ids: &[TableId],
    ) -> Result<TableOrderOutcome, PgDatabasesRepoError> {
        let mut transaction = self.pool.begin().await?;
        // The same lock that serializes table creation, renames and deletes,
        // so the set checked below cannot change before the commit.
        if sqlx::query!(
            "SELECT id FROM databases WHERE id = $1 AND trashed_at IS NULL FOR UPDATE",
            database_id
        )
        .fetch_optional(&mut *transaction)
        .await?
        .is_none()
        {
            return Ok(TableOrderOutcome::NotFound);
        }
        let mut current = sqlx::query_scalar!(
            "SELECT id FROM database_tables WHERE database_id = $1",
            database_id
        )
        .fetch_all(&mut *transaction)
        .await?;
        let mut requested = ids.to_vec();
        current.sort_unstable();
        requested.sort_unstable();
        if current != requested {
            return Ok(TableOrderOutcome::Conflict);
        }
        let positions: Vec<String> = (1..=ids.len())
            .map(|index| format!("{index:0POSITION_WIDTH$}"))
            .collect();
        let tables = sqlx::query!(
            r#"
            UPDATE database_tables t
            SET position = ordered.position, version = t.version + 1
            FROM UNNEST($2::uuid[], $3::text[]) AS ordered(id, position)
            WHERE t.id = ordered.id AND t.database_id = $1
            RETURNING t.id, t.database_id, t.name, t.position, t.version
            "#,
            database_id,
            ids,
            &positions,
        )
        .fetch_all(&mut *transaction)
        .await?;
        transaction.commit().await?;
        let mut tables: Vec<Table> = tables
            .into_iter()
            .map(|row| Table {
                id: row.id,
                database_id: row.database_id,
                name: row.name,
                position: row.position,
                version: TableVersion(row.version),
            })
            .collect();
        tables.sort_by(|a, b| a.position.cmp(&b.position));
        Ok(TableOrderOutcome::Applied(tables))
    }
}
