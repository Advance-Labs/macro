use super::*;
use crate::domain::models::TableOrderOutcome;

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
    pub(super) async fn order_tables(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        ids: Vec<TableId>,
    ) -> Result<Vec<Table>, DatabaseError> {
        let (database, tables) = self.database_for_edit(&receipt).await?;
        let expected: HashSet<_> = tables.iter().map(|table| table.id).collect();
        if ids.len() != expected.len() || ids.iter().copied().collect::<HashSet<_>>() != expected {
            return Err(DatabaseError::InvalidSchemaOperation(
                "The table order must include every table of this database exactly once.".into(),
            ));
        }
        let reordered = match self
            .repo
            .reorder_tables(database.id, &ids)
            .await
            .map_err(repo_err)?
        {
            TableOrderOutcome::Applied(tables) => tables,
            TableOrderOutcome::NotFound => return Err(DatabaseError::NotFound),
            TableOrderOutcome::Conflict => return Err(DatabaseError::VersionConflict),
        };
        self.publish(
            receipt_attribution(&receipt),
            &reordered
                .iter()
                .map(|table| (table.id, table.database_id))
                .collect(),
            &reordered
                .iter()
                .map(|table| (table.id, table.version))
                .collect(),
        )
        .await;
        Ok(reordered)
    }
}
