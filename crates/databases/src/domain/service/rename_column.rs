use super::*;

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
    pub(super) async fn rename_column_label(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        table_id: TableId,
        column_id: ColumnId,
        name: String,
        previous_name: String,
    ) -> Result<RenameColumnOutcome, DatabaseError> {
        let (database, tables) = self.database_for_edit(&receipt).await?;
        let table = tables
            .iter()
            .find(|table| table.id == table_id)
            .ok_or(DatabaseError::NotFound)?;
        let columns = self
            .repo
            .columns_for_tables(&[table_id])
            .await
            .map_err(repo_err)?;
        let column = columns
            .iter()
            .find(|column| column.id == column_id)
            .ok_or(DatabaseError::NotFound)?;
        let definition_ids: Vec<_> = columns
            .iter()
            .map(|column| column.property_definition_id)
            .collect();
        let definitions: HashMap<_, _> = self
            .definitions
            .definitions(&definition_ids)
            .await
            .map_err(repo_err)?
            .into_iter()
            .map(|definition| (definition.definition.id, definition.definition.display_name))
            .collect();
        let label = |column: &crate::domain::models::Column| {
            column
                .display_name
                .clone()
                .or_else(|| definitions.get(&column.property_definition_id).cloned())
        };
        let current = label(column).ok_or(DatabaseError::NotFound)?;
        let name = validate_name(&name)?;
        // A lost successful response may be retried without applying twice.
        if current == name {
            return Ok(RenameColumnOutcome {
                column: column.clone(),
                table_version: table.version,
            });
        }
        if current != previous_name {
            return Err(DatabaseError::from(SchemaError::ColumnRenamedElsewhere));
        }
        if columns.iter().any(|other| {
            other.id != column_id && label(other).is_some_and(|label| same_name(&label, &name))
        }) {
            return Err(DatabaseError::from(SchemaError::ColumnLabelTaken));
        }
        let outcome = self
            .repo
            .rename_column(table, column, &name)
            .await
            .map_err(repo_err)?
            .ok_or_else(|| DatabaseError::from(SchemaError::TableChangedWhileRenaming))?;
        self.publish(
            receipt_attribution(&receipt),
            &[(database.id, table_id, outcome.table_version)],
        )
        .await;
        Ok(outcome)
    }
}
