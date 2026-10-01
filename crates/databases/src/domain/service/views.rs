//! Views outside the ops: a board's card positions, and what removing or
//! retyping a column does to the views that refer to it.

use chrono::{DateTime, Utc};

use super::*;
use crate::domain::models::{DatabaseView, ViewId};

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
    pub(super) async fn board_positions(
        &self,
        receipt: EntityAccessReceipt<ViewAccessLevel>,
        view_id: ViewId,
    ) -> Result<Vec<CardPosition>, DatabaseError> {
        let database_id = receipt_database_id(&receipt)?;
        let (database, tables) = self
            .repo
            .get_database(database_id)
            .await
            .map_err(repo_err)?
            .ok_or(DatabaseError::NotFound)?;
        if database.trashed_at.is_some() {
            return Err(DatabaseError::NotFound);
        }
        let table_ids: Vec<TableId> = tables.iter().map(|table| table.id).collect();
        let views = self
            .repo
            .views_for_tables(&table_ids)
            .await
            .map_err(repo_err)?;
        if !views.iter().any(|view| view.id == view_id) {
            return Err(DatabaseError::NotFound);
        }
        self.repo.view_positions(view_id).await.map_err(repo_err)
    }
}

/// The views of a table that refer to `column`, rewritten without it, for
/// the column's removal. A board grouped by it has nothing else to group
/// by, so the removal waits for the board to go or regroup.
pub(super) fn views_without_column(
    views: &[DatabaseView],
    column: ColumnId,
    now: DateTime<Utc>,
) -> Result<Vec<DatabaseView>, SchemaError> {
    views
        .iter()
        .filter_map(|view| {
            let Some(layout) = view.layout.without_column(column) else {
                return Some(Err(SchemaError::BoardGroupsByRemovedColumn {
                    board: view.name.clone(),
                }));
            };
            let query = view.query.without_column(column);
            (query != view.query || layout != view.layout).then(|| {
                Ok(DatabaseView {
                    query,
                    layout,
                    updated_at: now,
                    ..view.clone()
                })
            })
        })
        .collect()
}

/// The views of a table whose filters test `column`, rewritten without
/// those tests, for a change of the column's type: they tested values the
/// column no longer holds. A board grouped by it must go or regroup first.
pub(super) fn views_without_tests_of(
    views: &[DatabaseView],
    column: ColumnId,
    now: DateTime<Utc>,
) -> Result<Vec<DatabaseView>, SchemaError> {
    views
        .iter()
        .filter_map(|view| {
            if view.layout.group_by() == Some(column) {
                return Some(Err(SchemaError::BoardGroupsByRetypedColumn {
                    board: view.name.clone(),
                }));
            }
            let query = view.query.without_tests_of(column);
            (query != view.query).then(|| {
                Ok(DatabaseView {
                    query,
                    updated_at: now,
                    ..view.clone()
                })
            })
        })
        .collect()
}
