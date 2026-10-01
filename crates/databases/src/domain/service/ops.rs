//! Typed ops: every op is checked against the receipt's database, as the ops
//! before it leave the schema, before anything is written, and their writes
//! commit in one transaction.

mod cells;
mod columns;
mod tables;
mod views;

use std::sync::Arc;

use cells::column_kind_name;

use models_databases::{
    CellWrite, ColumnKind, DatabaseOp, NewColumn, OpResult, PropertyId, RowChanges, TakenId,
    VersionedTable,
};
use models_properties::api::is_valid_hex_color;
use models_properties::service::property_definition_with_options::PropertyDefinitionWithOptions;

use super::*;
use crate::domain::catalog::ColumnEntry;
use crate::domain::models::{
    DatabaseView, NewOption, OpBatch, OpRefusal, PropertyDefinitionId, RowId, ViewId, Write,
    Writes, WritesOutcome,
};
use chrono::DateTime;

/// Most rows one request inserts, updates and deletes in total.
const MAX_WRITTEN_ROWS: usize = 10_000;

impl<Repository, Definitions, Cells, Events, Access, Broker>
    DatabasesServiceImpl<Repository, Definitions, Cells, Events, Access, Broker>
where
    Repository: DatabasesRepo,
    Definitions: ColumnDefinitionStore,
    Cells: CellStore,
    Events: TableEventPublisher,
    Access: AccessDirectory,
    Broker: MacroEventBroker,
{
    pub(super) async fn apply_database_ops(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        viewer: Viewer,
        batch: OpBatch,
    ) -> Result<Vec<OpResult>, DatabaseError> {
        let OpBatch { ops, base_versions } = batch;
        let database_id = receipt_database_id(&receipt)?;
        let grant = receipt_grant(&receipt, AccessLevel::Edit);
        let entries = self
            .entries_for(&HashMap::from([(database_id, grant)]))
            .await?;
        let Some(database) = entries.first().map(|entry| entry.database.clone()) else {
            return Err(DatabaseError::NotFound);
        };
        refuse_foreign_tables(&entries, &ops)?;
        for (table, version) in &base_versions {
            let entry = entries
                .iter()
                .find(|entry| entry.table.id == *table)
                .ok_or(DatabaseError::NotFound)?;
            if entry.table.version != *version {
                return Err(DatabaseError::VersionConflict);
            }
        }
        let attribution = receipt_attribution(&receipt);

        let found = self.found_for(&entries, &viewer, &ops).await?;
        let editable = self
            .editable_shared_definitions(&entries, &found, database_id, &viewer, &ops)
            .await?;
        let boards = self.boards_moved_by(&entries, &ops).await?;
        let mut planner = Planner {
            entries: entries.iter().cloned().map(Arc::new).collect(),
            database,
            grant,
            editable,
            found,
            options: Vec::new(),
            labels: HashMap::new(),
            views: HashMap::new(),
            boards,
            now: models_databases::views::written_at(),
            related: Vec::new(),
            written_rows: 0,
            written_tables: HashSet::new(),
            created_tables: HashSet::new(),
            changed_options: HashSet::new(),
            retyped: HashMap::new(),
        };
        let writes = ops
            .iter()
            .enumerate()
            .map(|(index, op)| planner.write(index, op))
            .collect::<Result<Vec<_>, _>>()?;
        let writes = Writes {
            database_id,
            created_by: viewer.user_id.clone(),
            options: std::mem::take(&mut planner.options),
            writes,
            related_rows: planner
                .related
                .iter()
                .map(|related| (related.table, related.row))
                .collect(),
            expected_versions: base_versions.into_iter().collect(),
        };
        let outcome = self
            .cells
            .apply_writes(&writes)
            .await
            .map_err(repository_error)?;
        let committed = applied(outcome, &ops, &planner.related)?;

        let mut changes: Vec<(DatabaseId, TableId, TableVersion)> = committed
            .table_versions
            .iter()
            .map(|(table, version)| {
                let database = related_database(&writes, *table).unwrap_or(database_id);
                (database, *table, *version)
            })
            .collect();
        changes.extend(writes.writes.iter().filter_map(|write| match write {
            Write::DeleteTable { table_id, version } => Some((database_id, *table_id, *version)),
            _ => None,
        }));
        self.publish(attribution, &changes).await;
        op_results(&entries, &ops, &writes, &planner, committed)
    }

    /// What the batch's ops need read before they are planned: the
    /// properties its new columns bind, the relation targets outside the
    /// database, and the cells of every column it retypes.
    async fn found_for(
        &self,
        entries: &[TableEntry],
        viewer: &Viewer,
        ops: &[DatabaseOp],
    ) -> Result<Found, DatabaseError> {
        let database_id = entries
            .first()
            .map(|entry| entry.database.id)
            .ok_or(DatabaseError::NotFound)?;
        let mut found = Found::default();
        for op in ops {
            let target_database = match op {
                DatabaseOp::CreateColumn {
                    definition:
                        NewColumn::New {
                            kind: ColumnKind::Relation { database, .. },
                            ..
                        },
                    ..
                }
                | DatabaseOp::ChangeColumnType {
                    to: ColumnKind::Relation { database, .. },
                    ..
                } => Some(*database),
                _ => None,
            };
            if let Some(database) = target_database
                && database != database_id
                && !found.relation_targets.contains_key(&database)
            {
                let tables = match self
                    .live_database_grant(viewer, database)
                    .await
                    .map_err(DatabaseError::Repo)?
                {
                    Some(_) => Some(
                        self.repository
                            .tables_for_databases(&[database])
                            .await
                            .map_err(repository_error)?
                            .into_iter()
                            .map(|table| table.id)
                            .collect(),
                    ),
                    None => None,
                };
                found.relation_targets.insert(database, tables);
            }
            if let DatabaseOp::CreateColumn {
                definition: NewColumn::Existing { property },
                ..
            } = op
                && !found.properties.contains_key(property)
            {
                let bindable = self
                    .definitions
                    .bindable_definition(database_id, viewer, property.into_uuid())
                    .await
                    .map_err(repository_error)?;
                let definition = match bindable {
                    Some(id) => self
                        .definitions
                        .definitions(&[id])
                        .await
                        .map_err(repository_error)?
                        .into_iter()
                        .next(),
                    None => None,
                };
                found.properties.insert(*property, definition);
            }
            if let DatabaseOp::ChangeColumnType { table, column, .. } = op
                && !found.cells.contains_key(column)
                && let Some(definition) = entries
                    .iter()
                    .find(|entry| entry.table.id == *table)
                    .and_then(|entry| {
                        entry
                            .columns
                            .iter()
                            .find(|entry| entry.column.id == *column)
                    })
                    .map(|entry| entry.definition.definition.id)
            {
                let rows: Vec<RowId> = self
                    .repository
                    .row_refs(*table)
                    .await
                    .map_err(repository_error)?
                    .into_iter()
                    .map(|row| row.id)
                    .collect();
                let cells = if rows.len() > MAX_CONVERTED_ROWS {
                    HashMap::new()
                } else {
                    self.cells
                        .column_cells(&rows, definition)
                        .await
                        .map_err(repository_error)?
                };
                found.cells.insert(*column, ColumnCells { rows, cells });
            }
        }
        Ok(found)
    }

    /// Where the cards of every board a `MoveCard` of the batch names are
    /// now, so the planner can place each move among them.
    async fn boards_moved_by(
        &self,
        entries: &[TableEntry],
        ops: &[DatabaseOp],
    ) -> Result<HashMap<ViewId, views::Board>, DatabaseError> {
        let mut boards = HashMap::new();
        for op in ops {
            let DatabaseOp::MoveCard { table, view, .. } = op else {
                continue;
            };
            if boards.contains_key(view) {
                continue;
            }
            let Some(entry) = entries.iter().find(|entry| entry.table.id == *table) else {
                continue;
            };
            let Some(group_by) = entry
                .views
                .iter()
                .find(|stored| stored.id == *view)
                .and_then(|stored| stored.layout.group_by())
            else {
                continue;
            };
            let Some(grouping) = entry
                .columns
                .iter()
                .find(|column| column.column.id == group_by)
                .map(|column| column.definition.definition.id)
            else {
                continue;
            };
            let rows: Vec<RowId> = self
                .repository
                .row_refs(*table)
                .await
                .map_err(repository_error)?
                .into_iter()
                .map(|row| row.id)
                .collect();
            let cells = self
                .cells
                .column_cells(&rows, grouping)
                .await
                .map_err(repository_error)?;
            let positions = self
                .repository
                .view_positions(*view)
                .await
                .map_err(repository_error)?;
            boards.insert(
                *view,
                views::Board::new(grouping, &rows, &cells, &positions),
            );
        }
        Ok(boards)
    }

    /// The definitions shared beyond the database whose options the batch
    /// may change, and which of them the viewer may change: a shared
    /// property changes wherever it is used, so the database's edit grant is
    /// not enough on its own.
    async fn editable_shared_definitions(
        &self,
        entries: &[TableEntry],
        found: &Found,
        database_id: DatabaseId,
        viewer: &Viewer,
        ops: &[DatabaseOp],
    ) -> Result<Vec<PropertyDefinitionId>, DatabaseError> {
        let mut shared: Vec<PropertyDefinitionId> = ops
            .iter()
            .filter_map(option_columns)
            .flat_map(|(table, columns)| {
                columns.into_iter().filter_map(move |column| {
                    entries
                        .iter()
                        .find(|entry| entry.table.id == table)?
                        .columns
                        .iter()
                        .find(|entry| entry.column.id == column)
                        .filter(|entry| entry.shared_outside(database_id))
                        .map(|entry| entry.definition.definition.id)
                })
            })
            .collect();
        // A property bound by this batch may have its options changed by a
        // later op of it.
        shared.extend(
            found
                .properties
                .values()
                .flatten()
                .filter(|definition| shared_beyond(definition, database_id))
                .map(|definition| definition.definition.id),
        );
        shared.sort();
        shared.dedup();
        if shared.is_empty() {
            return Ok(Vec::new());
        }
        self.definitions
            .editable_definitions(viewer, &shared)
            .await
            .map_err(repository_error)
    }
}

/// Whether a definition belongs to something beyond `database_id`.
fn shared_beyond(definition: &PropertyDefinitionWithOptions, database_id: DatabaseId) -> bool {
    !matches!(
        definition.definition.owner,
        models_properties::shared::PropertyOwner::Database { database_id: owner }
            if DatabaseId::from_uuid(owner) == database_id
    )
}

/// The database a write's related table belongs to, when it is outside the
/// batch's own.
fn related_database(writes: &Writes, table: TableId) -> Option<DatabaseId> {
    writes.writes.iter().find_map(|write| match write {
        Write::DeleteColumn {
            related: Some((database, related)),
            ..
        } if *related == table => Some(*database),
        _ => None,
    })
}

/// The table an op names and the columns whose options it may change: the
/// option ops', and every column whose missing options a row op may create.
fn option_columns(op: &DatabaseOp) -> Option<(TableId, Vec<ColumnId>)> {
    let cells = |cells: &[CellWrite]| cells.iter().map(|cell| cell.column).collect::<Vec<_>>();
    match op {
        DatabaseOp::UpdateOption { table, column, .. }
        | DatabaseOp::DeleteOption { table, column, .. }
        | DatabaseOp::AddOptions { table, column, .. } => Some((*table, vec![*column])),
        DatabaseOp::InsertRows {
            table,
            rows,
            create_missing_options: true,
        } => Some((*table, rows.iter().flat_map(|row| cells(row)).collect())),
        DatabaseOp::UpdateRows {
            table,
            changes,
            create_missing_options: true,
        } => Some((
            *table,
            match changes {
                RowChanges::Uniform { cells: written, .. } => cells(written),
                RowChanges::PerRow { rows } => {
                    rows.iter().flat_map(|row| cells(&row.cells)).collect()
                }
            },
        )),
        _ => None,
    }
}

/// Refuse a batch naming a table that is neither in the receipt's database
/// nor created by an earlier op of the batch.
fn refuse_foreign_tables(entries: &[TableEntry], ops: &[DatabaseOp]) -> Result<(), DatabaseError> {
    let mut created: Vec<TableId> = Vec::new();
    for (index, op) in ops.iter().enumerate() {
        if let DatabaseOp::CreateTable { id, .. } = op {
            created.push(*id);
            continue;
        }
        let named: Vec<TableId> = match op {
            DatabaseOp::ReorderTables { order } => order.clone(),
            op => op.table().into_iter().collect(),
        };
        for table in named {
            if !entries.iter().any(|entry| entry.table.id == table) && !created.contains(&table) {
                return Err(refuse(
                    index,
                    None,
                    None,
                    format!("table {table} is not in this database"),
                ));
            }
        }
    }
    Ok(())
}

/// What a batch's writes committed.
struct Committed {
    /// Per write, the rows it inserted.
    inserted: Vec<Vec<RowId>>,
    /// The new version of every table a write changed.
    table_versions: HashMap<TableId, TableVersion>,
}

/// What the cell store committed, or the op its refusal points at.
fn applied(
    outcome: WritesOutcome,
    ops: &[DatabaseOp],
    related: &[RelatedRow],
) -> Result<Committed, DatabaseError> {
    let column_of = |write: usize| op_column(&ops[write]);
    match outcome {
        WritesOutcome::Applied {
            inserted,
            table_versions,
        } => Ok(Committed {
            inserted,
            table_versions,
        }),
        WritesOutcome::TableNotFound(_) => Err(DatabaseError::NotFound),
        WritesOutcome::VersionConflict(_) | WritesOutcome::TablesChanged { .. } => {
            Err(DatabaseError::VersionConflict)
        }
        WritesOutcome::IdTaken { write, id } => Err(refuse_taken(write, id)),
        WritesOutcome::TableNameTaken { write } => Err(refuse(
            write,
            None,
            None,
            "another table took that name first; refresh and try again",
        )),
        WritesOutcome::TableRenamedElsewhere { write } => Err(refuse(
            write,
            None,
            None,
            SchemaError::TableRenameConflict.to_string(),
        )),
        WritesOutcome::LastTable { write } => Err(refuse(
            write,
            None,
            None,
            SchemaError::LastTable.to_string(),
        )),
        WritesOutcome::MissingColumn { write } => Err(refuse(
            write,
            None,
            column_of(write),
            "the column was removed or changed by someone else; refresh and try again",
        )),
        WritesOutcome::ColumnRenamedElsewhere { write } => Err(refuse(
            write,
            None,
            column_of(write),
            SchemaError::ColumnRenamedElsewhere.to_string(),
        )),
        WritesOutcome::MissingOption { write } => Err(refuse(
            write,
            None,
            column_of(write),
            "the option was removed by someone else; refresh and try again",
        )),
        WritesOutcome::OptionLabelTaken { write } => Err(refuse(
            write,
            None,
            column_of(write),
            "another option took that label first; refresh and try again",
        )),
        WritesOutcome::MissingView { write } => Err(refuse(
            write,
            None,
            None,
            "the view was removed by someone else; refresh and try again",
        )),
        WritesOutcome::ViewNameTaken { write } => Err(refuse(
            write,
            None,
            None,
            "another view took that name first; refresh and try again",
        )),
        WritesOutcome::MissingRow { write, row } => Err(refuse(
            write,
            row_index(&ops[write], row),
            None,
            format!("no row {row} in this table"),
        )),
        WritesOutcome::MissingRelatedRow(row) => {
            let origin = related
                .iter()
                .find(|related| related.row == row)
                .ok_or_else(|| {
                    DatabaseError::Repo(
                        rootcause::report!("the cell store reported a row no op named")
                            .into_dynamic(),
                    )
                })?;
            Err(refuse(
                origin.op,
                origin.row_index,
                Some(origin.column),
                format!("row {row} is not a row of the related table"),
            ))
        }
    }
}

/// One result per op, in order, from what its write committed.
fn op_results(
    entries: &[TableEntry],
    ops: &[DatabaseOp],
    writes: &Writes,
    planner: &Planner,
    committed: Committed,
) -> Result<Vec<OpResult>, DatabaseError> {
    let table_versions = committed.table_versions;
    // A write that changed nothing leaves its table where it was.
    let version_of = |table: TableId| -> Result<TableVersion, DatabaseError> {
        table_versions
            .get(&table)
            .copied()
            .or_else(|| {
                entries
                    .iter()
                    .find(|entry| entry.table.id == table)
                    .map(|entry| entry.table.version)
            })
            .ok_or_else(|| {
                DatabaseError::Repo(
                    rootcause::report!("an op named a table the batch did not version")
                        .into_dynamic(),
                )
            })
    };
    ops.iter()
        .zip(&writes.writes)
        .zip(committed.inserted)
        .enumerate()
        .map(|(index, ((op, write), inserted))| {
            let table_version = || version_of(op.table().ok_or_else(mismatched_write)?);
            Ok(match op {
                DatabaseOp::CreateTable { id, .. } => OpResult::TableCreated {
                    table: *id,
                    table_version: version_of(*id)?,
                },
                DatabaseOp::RenameTable { .. } => OpResult::TableRenamed {
                    table_version: table_version()?,
                },
                DatabaseOp::DeleteTable { table } => OpResult::TableDeleted { table: *table },
                DatabaseOp::ReorderTables { order } => OpResult::TablesReordered {
                    tables: order
                        .iter()
                        .map(|table| {
                            Ok(VersionedTable {
                                table: *table,
                                version: version_of(*table)?,
                            })
                        })
                        .collect::<Result<_, DatabaseError>>()?,
                },
                DatabaseOp::CreateColumn { id, .. } => OpResult::ColumnCreated {
                    column: *id,
                    table_version: table_version()?,
                },
                DatabaseOp::RenameColumn { .. } => OpResult::ColumnRenamed {
                    table_version: table_version()?,
                },
                DatabaseOp::DeleteColumn { .. } => OpResult::ColumnDeleted {
                    table_version: table_version()?,
                },
                DatabaseOp::ReorderColumns { .. } => OpResult::ColumnsReordered {
                    table_version: table_version()?,
                },
                DatabaseOp::AddOptions { .. } => OpResult::OptionsAdded {
                    table_version: table_version()?,
                    added: match write {
                        Write::AddOptions { options, .. } => {
                            options.iter().map(|(id, _)| *id).collect()
                        }
                        _ => Vec::new(),
                    },
                },
                DatabaseOp::ChangeColumnType { .. } => {
                    let (cleared, trimmed) =
                        planner.retyped.get(&index).copied().unwrap_or_default();
                    OpResult::ColumnTyped {
                        table_version: table_version()?,
                        cleared_cells: count(cleared),
                        trimmed_cells: count(trimmed),
                    }
                }
                DatabaseOp::InsertRows { .. }
                | DatabaseOp::UpdateRows { .. }
                | DatabaseOp::DeleteRows { .. } => OpResult::RowsWritten {
                    table_version: table_version()?,
                    inserted,
                    affected: count(write.affected()),
                },
                DatabaseOp::UpdateOption { .. } | DatabaseOp::DeleteOption { .. } => {
                    OpResult::OptionChanged {
                        table_version: table_version()?,
                    }
                }
                DatabaseOp::CreateView { .. } | DatabaseOp::UpdateView { .. } => match write {
                    Write::CreateView { view } | Write::UpdateView { view, .. } => {
                        OpResult::ViewWritten {
                            table_version: table_version()?,
                            view: Box::new(view.clone()),
                        }
                    }
                    _ => return Err(mismatched_write()),
                },
                DatabaseOp::DeleteView { .. } => OpResult::ViewDeleted {
                    table_version: table_version()?,
                },
                DatabaseOp::ReorderViews { .. } => match write {
                    Write::OrderViews { positions, .. } => OpResult::ViewsReordered {
                        table_version: table_version()?,
                        positions: positions.clone(),
                    },
                    _ => return Err(mismatched_write()),
                },
                DatabaseOp::MoveCard { .. } => match write {
                    Write::MoveCard { positions, .. } => OpResult::CardMoved {
                        table_version: table_version()?,
                        positions: positions.clone(),
                    },
                    _ => return Err(mismatched_write()),
                },
            })
        })
        .collect()
}

fn mismatched_write() -> DatabaseError {
    DatabaseError::Repo(
        rootcause::report!("an op was planned as another op's write").into_dynamic(),
    )
}

fn refuse(
    op: usize,
    row: Option<usize>,
    column: Option<ColumnId>,
    reason: impl Into<String>,
) -> DatabaseError {
    DatabaseError::InvalidOp(OpRefusal {
        op,
        row,
        column,
        taken: None,
        reason: reason.into(),
    })
}

/// Refuse an op minting an id that already names something.
fn refuse_taken(op: usize, id: TakenId) -> DatabaseError {
    let (named, what, column, uuid) = match id {
        TakenId::Table(id) => ("a table", "table", None, id.into_uuid()),
        TakenId::Column(id) => ("a column", "column", Some(id), id.into_uuid()),
        TakenId::Option(id) => ("an option", "option", None, id.into_uuid()),
    };
    DatabaseError::InvalidOp(OpRefusal {
        op,
        row: None,
        column,
        taken: Some(id),
        reason: format!(
            "{uuid} already names {named}; mint a new id for each {what} a request creates"
        ),
    })
}

/// The column an op names, for a refusal to point at.
fn op_column(op: &DatabaseOp) -> Option<ColumnId> {
    match op {
        DatabaseOp::UpdateOption { column, .. }
        | DatabaseOp::DeleteOption { column, .. }
        | DatabaseOp::AddOptions { column, .. }
        | DatabaseOp::RenameColumn { column, .. }
        | DatabaseOp::DeleteColumn { column, .. }
        | DatabaseOp::ChangeColumnType { column, .. } => Some(*column),
        DatabaseOp::CreateColumn { id, .. } => Some(*id),
        _ => None,
    }
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Where a row an op names sits in it; `None` for an insert, whose rows are
/// new, and for every op that names no rows.
fn row_index(op: &DatabaseOp, row: RowId) -> Option<usize> {
    match op {
        DatabaseOp::UpdateRows {
            changes: RowChanges::Uniform { rows, .. },
            ..
        }
        | DatabaseOp::DeleteRows { rows, .. } => rows.iter().position(|named| *named == row),
        DatabaseOp::UpdateRows {
            changes: RowChanges::PerRow { rows },
            ..
        } => rows.iter().position(|change| change.row == row),
        _ => None,
    }
}

/// A row a relation cell points at, and where the op named it.
struct RelatedRow {
    table: TableId,
    row: RowId,
    op: usize,
    row_index: Option<usize>,
    column: ColumnId,
}

/// One column's stored cells, read before the batch is planned.
#[derive(Debug, Clone, Default)]
struct ColumnCells {
    /// Every row of the column's table, in position order.
    rows: Vec<RowId>,
    /// The column's value on each row holding one.
    cells: HashMap<RowId, PropertyValue>,
}

/// What the batch's ops needed read before planning.
#[derive(Debug, Default)]
struct Found {
    /// Each property a new column binds, as the properties system lets the
    /// viewer bind it; `None` when it may not.
    properties: HashMap<PropertyId, Option<PropertyDefinitionWithOptions>>,
    /// Each database outside the batch's that a relation points into, with
    /// its tables; `None` when the viewer cannot reach it.
    relation_targets: HashMap<DatabaseId, Option<Vec<TableId>>>,
    /// The stored cells of each column the batch retypes.
    cells: HashMap<ColumnId, ColumnCells>,
}

/// Turns ops into writes against one database's catalog, as the ops before
/// each leave it, collecting the options they create and the rows their
/// relation cells point at.
struct Planner {
    /// The database's tables as the ops so far leave them, in tab order.
    entries: Vec<Arc<TableEntry>>,
    database: Database,
    grant: AccessLevel,
    /// The shared definitions whose options the viewer may change.
    editable: Vec<PropertyDefinitionId>,
    found: Found,
    options: Vec<NewOption>,
    /// The options of each definition an op has looked at, with their
    /// labels, as the ops planned so far leave them.
    labels: HashMap<PropertyDefinitionId, Vec<(OptionId, String)>>,
    /// The views of each table an op has looked at, in their order, as the
    /// ops planned so far leave them.
    views: HashMap<TableId, Vec<DatabaseView>>,
    /// Where the cards of the boards the batch moves cards on are.
    boards: HashMap<ViewId, views::Board>,
    /// When the batch is applied, for the views it writes.
    now: DateTime<Utc>,
    related: Vec<RelatedRow>,
    written_rows: usize,
    /// The tables whose rows an op so far wrote.
    written_tables: HashSet<TableId>,
    /// The tables an op so far created.
    created_tables: HashSet<TableId>,
    /// The definitions whose options an op so far changed.
    changed_options: HashSet<PropertyDefinitionId>,
    /// Per type change op, the cells it emptied and the cells it trimmed.
    retyped: HashMap<usize, (usize, usize)>,
}

/// Where in the batch a cell is: its op, the row's index within the op
/// (none for an update's shared cells), and its column.
#[derive(Clone, Copy)]
struct Place {
    op: usize,
    row: Option<usize>,
    column: ColumnId,
}

impl Place {
    fn refuse(self, reason: impl Into<String>) -> DatabaseError {
        refuse(self.op, self.row, Some(self.column), reason)
    }
}

impl Planner {
    fn write(&mut self, index: usize, op: &DatabaseOp) -> Result<Write, DatabaseError> {
        match op {
            DatabaseOp::CreateTable { id, name } => return self.create_table(index, *id, name),
            DatabaseOp::ReorderTables { order } => return self.order_tables(index, order),
            _ => {}
        }
        let table = op
            .table()
            .ok_or_else(|| refuse(index, None, None, "the op names no table"))?;
        let entry = self.entry(index, table)?;
        let rows = match op {
            DatabaseOp::InsertRows { rows, .. } => rows.len(),
            DatabaseOp::UpdateRows {
                changes: RowChanges::Uniform { rows, .. },
                ..
            }
            | DatabaseOp::DeleteRows { rows, .. } => rows.len(),
            DatabaseOp::UpdateRows {
                changes: RowChanges::PerRow { rows },
                ..
            } => rows.len(),
            _ => 0,
        };
        self.written_rows += rows;
        if self.written_rows > MAX_WRITTEN_ROWS {
            return Err(refuse(
                index,
                None,
                None,
                format!("a request writes at most {MAX_WRITTEN_ROWS} rows"),
            ));
        }
        match op {
            DatabaseOp::CreateTable { .. } | DatabaseOp::ReorderTables { .. } => {
                Err(refuse(index, None, None, "the op names no table"))
            }
            DatabaseOp::RenameTable {
                name,
                previous_name,
                ..
            } => self.rename_table(index, &entry, name, previous_name.as_deref()),
            DatabaseOp::DeleteTable { .. } => self.delete_table(index, &entry),
            DatabaseOp::CreateColumn {
                id,
                definition,
                after,
                ..
            } => self.create_column(index, &entry, *id, definition, *after),
            DatabaseOp::RenameColumn {
                column,
                name,
                previous_name,
                ..
            } => self.rename_column(index, &entry, *column, name, previous_name.as_deref()),
            DatabaseOp::DeleteColumn { column, .. } => self.delete_column(index, &entry, *column),
            DatabaseOp::ReorderColumns { order, .. } => self.order_columns(index, &entry, order),
            DatabaseOp::AddOptions {
                column, options, ..
            } => self.add_options(index, &entry, *column, options),
            DatabaseOp::ChangeColumnType {
                column,
                to,
                clear_invalid,
                ..
            } => self.change_type(index, &entry, *column, *to, *clear_invalid),
            DatabaseOp::InsertRows {
                rows,
                create_missing_options,
                ..
            } => {
                self.written_tables.insert(table);
                let rows = rows
                    .iter()
                    .enumerate()
                    .map(|(row, cells)| {
                        let cells =
                            self.cells(&entry, index, Some(row), cells, *create_missing_options)?;
                        Ok(cells
                            .into_iter()
                            .filter_map(|(definition, value)| {
                                value.map(|value| (definition, value))
                            })
                            .collect())
                    })
                    .collect::<Result<_, DatabaseError>>()?;
                Ok(Write::InsertRows {
                    table_id: table,
                    rows,
                })
            }
            DatabaseOp::UpdateRows {
                changes: RowChanges::Uniform { rows, cells },
                create_missing_options,
                ..
            } => {
                self.written_tables.insert(table);
                let cells = self.cells(&entry, index, None, cells, *create_missing_options)?;
                Ok(Write::UpdateRows {
                    table_id: table,
                    rows: rows.iter().map(|row| (*row, cells.clone())).collect(),
                })
            }
            DatabaseOp::UpdateRows {
                changes: RowChanges::PerRow { rows },
                create_missing_options,
                ..
            } => {
                self.written_tables.insert(table);
                Ok(Write::UpdateRows {
                    table_id: table,
                    rows: rows
                        .iter()
                        .enumerate()
                        .map(|(row, change)| {
                            let cells = self.cells(
                                &entry,
                                index,
                                Some(row),
                                &change.cells,
                                *create_missing_options,
                            )?;
                            Ok((change.row, cells))
                        })
                        .collect::<Result<_, DatabaseError>>()?,
                })
            }
            DatabaseOp::DeleteRows { rows, .. } => {
                self.written_tables.insert(table);
                for (row, id) in rows.iter().enumerate() {
                    if rows[..row].contains(id) {
                        return Err(refuse(
                            index,
                            Some(row),
                            None,
                            format!("row {id} is named twice"),
                        ));
                    }
                }
                Ok(Write::DeleteRows {
                    table_id: table,
                    rows: rows.clone(),
                })
            }
            DatabaseOp::UpdateOption {
                column,
                option,
                label,
                color,
                ..
            } => {
                let place = Place {
                    op: index,
                    row: None,
                    column: *column,
                };
                let column = self.option_column(&entry, place)?;
                self.known_option(place, column, *option)?;
                let value = label
                    .as_deref()
                    .map(|label| self.relabel(place, column, *option, label))
                    .transpose()?;
                let color = match color {
                    Some(None) if column.definition.definition.data_type == DataType::Tag => {
                        return Err(
                            place.refuse("a tag option always has a colour; pick another instead")
                        );
                    }
                    Some(Some(color)) if !is_valid_hex_color(color) => {
                        return Err(place.refuse(format!(
                            "{color} is not a colour; give a hex string like #RRGGBB"
                        )));
                    }
                    Some(color) => Some(color.clone()),
                    None => None,
                };
                let definition_id = column.definition.definition.id;
                self.changed_options.insert(definition_id);
                Ok(Write::UpdateOption {
                    table_id: table,
                    tables: self.tables_binding(definition_id),
                    definition_id,
                    option_id: *option,
                    value,
                    color,
                })
            }
            DatabaseOp::DeleteOption { column, option, .. } => {
                let place = Place {
                    op: index,
                    row: None,
                    column: *column,
                };
                let column = self.option_column(&entry, place)?;
                self.known_option(place, column, *option)?;
                self.labels_of(&column.definition)
                    .retain(|(id, _)| id != option);
                let definition_id = column.definition.definition.id;
                self.changed_options.insert(definition_id);
                let tables = self.tables_binding(definition_id);
                let views = self.views_without_option(&tables, definition_id, *option);
                Ok(Write::DeleteOption {
                    table_id: table,
                    tables,
                    definition_id,
                    option_id: *option,
                    views,
                })
            }
            DatabaseOp::CreateView { .. }
            | DatabaseOp::UpdateView { .. }
            | DatabaseOp::DeleteView { .. }
            | DatabaseOp::ReorderViews { .. }
            | DatabaseOp::MoveCard { .. } => self.view_write(index, &entry, op),
        }
    }

    /// The table an op names, as the ops so far leave it.
    fn entry(&self, index: usize, table: TableId) -> Result<Arc<TableEntry>, DatabaseError> {
        self.entries
            .iter()
            .find(|entry| entry.table.id == table)
            .cloned()
            .ok_or_else(|| refuse(index, None, None, "table is not in this database"))
    }

    /// The table an op changes, to change it in place.
    fn entry_mut(&mut self, table: TableId) -> Option<&mut TableEntry> {
        self.entries
            .iter_mut()
            .find(|entry| entry.table.id == table)
            .map(Arc::make_mut)
    }

    /// The column an option op names, which must hold options the viewer
    /// may change.
    fn option_column<'entry>(
        &self,
        entry: &'entry TableEntry,
        place: Place,
    ) -> Result<&'entry ColumnEntry, DatabaseError> {
        let column = entry
            .columns
            .iter()
            .find(|column| column.column.id == place.column)
            .ok_or_else(|| place.refuse("no such column in this table"))?;
        let definition = &column.definition.definition;
        if !takes_options(definition.data_type) {
            return Err(place.refuse(format!(
                "\"{}\" is a {} column; only select and tag columns have options",
                column.name(),
                column_kind_name(column)
            )));
        }
        self.may_change_options(place, column)?;
        Ok(column)
    }

    /// Refuse a change to the options of a property shared beyond the
    /// database unless the viewer may edit that property.
    fn may_change_options(&self, place: Place, column: &ColumnEntry) -> Result<(), DatabaseError> {
        if column.shared_outside(self.database.id)
            && !self.editable.contains(&column.definition.definition.id)
        {
            return Err(place.refuse(
                SchemaError::SharedOptions {
                    column: column.name().to_owned(),
                }
                .to_string(),
            ));
        }
        Ok(())
    }

    /// Check the column has the option, as the ops so far leave it.
    fn known_option(
        &mut self,
        place: Place,
        column: &ColumnEntry,
        option: OptionId,
    ) -> Result<(), DatabaseError> {
        if self
            .labels_of(&column.definition)
            .iter()
            .any(|(id, _)| *id == option)
        {
            Ok(())
        } else {
            Err(place.refuse(format!("no option {option} on \"{}\"", column.name())))
        }
    }

    /// The value an option's new label stores, checked as a new option's
    /// would be against the column's other options.
    fn relabel(
        &mut self,
        place: Place,
        column: &ColumnEntry,
        option: OptionId,
        label: &str,
    ) -> Result<PropertyOptionValue, DatabaseError> {
        let data_type = column.definition.definition.data_type;
        let labels = self.labels_of(&column.definition);
        let others: Vec<String> = labels
            .iter()
            .filter(|(id, _)| *id != option)
            .map(|(_, label)| label.clone())
            .collect();
        let value = validate_option_labels(data_type, &[label.to_string()], &others)
            .map_err(|error| schema_refusal(place, error))?
            .into_iter()
            .next()
            .ok_or_else(|| {
                place.refuse(format!(
                    "`{}` is already an option of \"{}\"",
                    label.trim(),
                    column.name()
                ))
            })?;
        if let Some((_, current)) = labels.iter_mut().find(|(id, _)| *id == option) {
            *current = catalog::option_display(&value);
        }
        Ok(value)
    }

    /// Every table of the database whose columns bind the definition.
    fn tables_binding(&self, definition: PropertyDefinitionId) -> Vec<TableId> {
        self.entries
            .iter()
            .filter(|entry| entry.column_for(definition).is_some())
            .map(|entry| entry.table.id)
            .collect()
    }
}

/// A schema refusal as the refusal of the op at `place`.
fn schema_refusal(place: Place, error: DatabaseError) -> DatabaseError {
    match error {
        DatabaseError::InvalidSchemaOperation(reason) => place.refuse(reason.to_string()),
        other => other,
    }
}
