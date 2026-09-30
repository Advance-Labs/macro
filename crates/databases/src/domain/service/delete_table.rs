use super::*;
use crate::domain::models::TableDeletion;

fn last_table() -> DatabaseError {
    DatabaseError::InvalidSchemaOperation(
        "This is the database's only table, and a database keeps at least one. Add another \
         table first, or delete the whole database instead."
            .into(),
    )
}

impl<Repo, Defs, Cells, Events, Access, Broker>
    DatabasesServiceImpl<Repo, Defs, Cells, Events, Access, Broker>
where
    Repo: DatabasesRepo,
    Defs: ColumnDefinitionStore,
    Cells: CellStore,
    Events: TableEventPublisher,
    Access: AccessDirectory,
    Broker: MacroEventBroker,
{
    pub(super) async fn remove_table(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        table_id: TableId,
    ) -> Result<(), DatabaseError> {
        let (database, tables) = self.database_for_edit(&receipt).await?;
        let table = tables
            .iter()
            .find(|table| table.id == table_id)
            .ok_or(DatabaseError::NotFound)?;
        if tables.len() == 1 {
            return Err(last_table());
        }
        // A relation into the table would be left holding ids of rows that
        // no longer exist.
        let other_tables: Vec<TableId> = tables
            .iter()
            .filter(|other| other.id != table_id)
            .map(|other| other.id)
            .collect();
        let columns = self
            .repo
            .columns_for_tables(&other_tables)
            .await
            .map_err(repo_err)?;
        if let Some(relation) = columns.iter().find(|column| {
            matches!(column.config, Some(ColumnConfig::Link { table_id: target, .. }) if target == table_id)
        }) {
            let definition_name = self
                .definitions
                .definitions(&[relation.property_definition_id])
                .await
                .map_err(repo_err)?
                .into_iter()
                .next()
                .map(|definition| definition.definition.display_name);
            let column_name = relation
                .display_name
                .clone()
                .or(definition_name)
                .unwrap_or_default();
            let source_table = tables
                .iter()
                .find(|other| other.id == relation.table_id)
                .map(|other| other.name.as_str())
                .unwrap_or_default();
            return Err(DatabaseError::InvalidSchemaOperation(format!(
                "Column `{column_name}` of table `{source_table}` relates to rows of `{}`. Delete \
                 that column first.",
                table.name
            )));
        }

        let row_ids = match self.repo.delete_table(table).await.map_err(repo_err)? {
            TableDeletion::Deleted { row_ids } => row_ids,
            TableDeletion::NotFound => return Err(DatabaseError::NotFound),
            TableDeletion::LastTable => return Err(last_table()),
        };
        // The table is gone and nothing can reach these rows any more, so a
        // cell left behind is unreachable rather than wrong; failing the
        // request here would only invite a retry of a committed delete.
        for row_id in row_ids {
            if let Err(error) = self.cells.clear(row_id).await {
                tracing::error!(error = ?error, %row_id, %table_id, "failed to clear a deleted table's row cells");
            }
        }
        self.publish(
            receipt_attribution(&receipt),
            &HashMap::from([(table_id, database.id)]),
            &HashMap::from([(table_id, table.version)]),
        )
        .await;
        Ok(())
    }
}
