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
        if !rows::lock_live_database(&mut *transaction, database_id).await? {
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
        let positions = keys_between(None, None, ids.len())?;
        let tables = sqlx::query_as!(
            TableRecord,
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
        let mut tables: Vec<Table> = tables.into_iter().map(Table::from).collect();
        tables.sort_by(|left, right| left.position.cmp(&right.position));
        Ok(TableOrderOutcome::Applied(tables))
    }
}
