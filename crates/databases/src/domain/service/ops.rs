//! Typed ops: every op is checked against the receipt's database before
//! anything is written, and their writes commit in one transaction.

mod cells;
mod views;

use cells::column_kind_name;

use models_databases::{CellWrite, ColumnKind, DatabaseOp, OpResult, RowChanges};
use models_properties::api::is_valid_hex_color;

use super::*;
use crate::domain::catalog::{ColumnEntry, PropertyType};
use crate::domain::models::{
    DatabaseView, NewOption, OpRefusal, PropertyDefinitionId, RowId, ViewId, Write, Writes,
    WritesOutcome,
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
        ops: Vec<DatabaseOp>,
    ) -> Result<Vec<OpResult>, DatabaseError> {
        let database_id = receipt_database_id(&receipt)?;
        let grant = receipt_grant(&receipt, AccessLevel::Edit);
        let entries = self
            .entries_for(&HashMap::from([(database_id, grant)]))
            .await?;
        if entries.is_empty() {
            return Err(DatabaseError::NotFound);
        }
        refuse_foreign_tables(&entries, &ops)?;
        let attribution = receipt_attribution(&receipt);
        if let [
            DatabaseOp::ChangeColumnType {
                table,
                column,
                to,
                clear_invalid,
            },
        ] = ops.as_slice()
        {
            let change = change_column_type(&entries, *table, *column, *to, *clear_invalid)?;
            return self
                .apply_type_change_op(database_id, attribution, &viewer, change)
                .await
                .map(|result| vec![result]);
        }

        let editable = self
            .editable_shared_definitions(&entries, database_id, &viewer, &ops)
            .await?;
        let boards = self.boards_moved_by(&entries, &ops).await?;
        let mut planner = Planner {
            entries: &entries,
            database_id,
            editable: &editable,
            options: Vec::new(),
            labels: HashMap::new(),
            views: HashMap::new(),
            boards,
            now: models_databases::views::written_at(),
            related: Vec::new(),
            written_rows: 0,
        };
        let writes = ops
            .iter()
            .enumerate()
            .map(|(index, op)| planner.write(index, op))
            .collect::<Result<Vec<_>, _>>()?;
        let row_writes = Writes {
            created_by: viewer.user_id.clone(),
            options: planner.options,
            writes,
            related_rows: planner
                .related
                .iter()
                .map(|related| (related.table, related.row))
                .collect(),
        };
        let outcome = self
            .cells
            .apply_writes(&row_writes)
            .await
            .map_err(repository_error)?;
        let committed = applied(outcome, &ops, &planner.related)?;

        self.publish(
            attribution,
            &committed
                .table_versions
                .iter()
                .map(|(table, version)| (database_id, *table, *version))
                .collect::<Vec<_>>(),
        )
        .await;
        op_results(&entries, &row_writes, committed)
    }

    /// A batch's only op, a column type change, applied as
    /// `change_column_type` applies it.
    async fn apply_type_change_op(
        &self,
        database_id: DatabaseId,
        attribution: Option<events::Attribution>,
        viewer: &Viewer,
        change: ChangeColumnType,
    ) -> Result<OpResult, DatabaseError> {
        let (table, column) = (change.table_id, change.column_id);
        let changed = self
            .retype_column(database_id, attribution, viewer, change)
            .await
            .map_err(|error| match error {
                DatabaseError::InvalidSchemaOperation(reason) => {
                    refuse(0, None, Some(column), reason.to_string())
                }
                DatabaseError::NotFound => {
                    refuse(0, None, Some(column), "no such column in this table")
                }
                other => other,
            })?;
        let table_version = changed.table_versions.get(&table).copied().ok_or_else(|| {
            DatabaseError::Repo(
                rootcause::report!("a type change did not answer its table's version")
                    .into_dynamic(),
            )
        })?;
        Ok(OpResult::ColumnTyped {
            table_version,
            cleared_cells: count(changed.cleared_cells),
            trimmed_cells: count(changed.trimmed_cells),
        })
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
    /// changes, and which of them the viewer may change: a shared property
    /// changes wherever it is used, so the database's edit grant is not
    /// enough on its own.
    async fn editable_shared_definitions(
        &self,
        entries: &[TableEntry],
        database_id: DatabaseId,
        viewer: &Viewer,
        ops: &[DatabaseOp],
    ) -> Result<Vec<PropertyDefinitionId>, DatabaseError> {
        let shared: Vec<PropertyDefinitionId> = ops
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
        if shared.is_empty() {
            return Ok(Vec::new());
        }
        self.definitions
            .editable_definitions(viewer, &shared)
            .await
            .map_err(repository_error)
    }
}

/// The table an op names and the columns whose options it may change: the
/// option ops', and every column whose missing options a row op may create.
fn option_columns(op: &DatabaseOp) -> Option<(TableId, Vec<ColumnId>)> {
    let cells = |cells: &[CellWrite]| cells.iter().map(|cell| cell.column).collect::<Vec<_>>();
    match op {
        DatabaseOp::UpdateOption { table, column, .. }
        | DatabaseOp::DeleteOption { table, column, .. } => Some((*table, vec![*column])),
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

/// Refuse a batch naming a table outside the receipt's database.
fn refuse_foreign_tables(entries: &[TableEntry], ops: &[DatabaseOp]) -> Result<(), DatabaseError> {
    for (index, op) in ops.iter().enumerate() {
        let table = op.table();
        if !entries.iter().any(|entry| entry.table.id == table) {
            return Err(refuse(
                index,
                None,
                None,
                format!("table {table} is not in this database"),
            ));
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
    match outcome {
        WritesOutcome::Applied {
            inserted,
            table_versions,
        } => Ok(Committed {
            inserted,
            table_versions,
        }),
        WritesOutcome::TableNotFound(_) => Err(DatabaseError::NotFound),
        WritesOutcome::MissingOption { write } => Err(refuse(
            write,
            None,
            option_column(&ops[write]),
            "the option was removed by someone else; refresh and try again",
        )),
        WritesOutcome::OptionLabelTaken { write } => Err(refuse(
            write,
            None,
            option_column(&ops[write]),
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
    row_writes: &Writes,
    committed: Committed,
) -> Result<Vec<OpResult>, DatabaseError> {
    let table_versions = committed.table_versions;
    row_writes
        .writes
        .iter()
        .zip(committed.inserted)
        .map(|(write, inserted)| {
            // A write that changed nothing leaves its table where it was.
            let table_version = match table_versions.get(&write.table_id()) {
                Some(version) => *version,
                None => current_version(entries, write.table_id())?,
            };
            Ok(match write {
                Write::InsertRows { .. } | Write::UpdateRows { .. } | Write::DeleteRows { .. } => {
                    OpResult::RowsWritten {
                        table_version,
                        inserted,
                        affected: count(write.affected()),
                    }
                }
                Write::UpdateOption { .. } | Write::DeleteOption { .. } => {
                    OpResult::OptionChanged { table_version }
                }
                Write::CreateView { view } | Write::UpdateView { view, .. } => {
                    OpResult::ViewWritten {
                        table_version,
                        view: Box::new(view.clone()),
                    }
                }
                Write::DeleteView { .. } => OpResult::ViewDeleted { table_version },
                Write::OrderViews { positions, .. } => OpResult::ViewsReordered {
                    table_version,
                    positions: positions.clone(),
                },
                Write::MoveCard { positions, .. } => OpResult::CardMoved {
                    table_version,
                    positions: positions.clone(),
                },
            })
        })
        .collect()
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
        reason: reason.into(),
    })
}

/// The column an option op names.
fn option_column(op: &DatabaseOp) -> Option<ColumnId> {
    match op {
        DatabaseOp::UpdateOption { column, .. } | DatabaseOp::DeleteOption { column, .. } => {
            Some(*column)
        }
        _ => None,
    }
}

/// The version a table had when the batch read the catalog; every table an
/// op names was checked to be there.
fn current_version(entries: &[TableEntry], table: TableId) -> Result<TableVersion, DatabaseError> {
    entries
        .iter()
        .find(|entry| entry.table.id == table)
        .map(|entry| entry.table.version)
        .ok_or_else(|| {
            DatabaseError::Repo(
                rootcause::report!("an op named a table the batch did not check").into_dynamic(),
            )
        })
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Where a row an op names sits in it; `None` for an insert, whose rows are
/// new.
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
        DatabaseOp::InsertRows { .. }
        | DatabaseOp::ChangeColumnType { .. }
        | DatabaseOp::UpdateOption { .. }
        | DatabaseOp::DeleteOption { .. }
        | DatabaseOp::CreateView { .. }
        | DatabaseOp::UpdateView { .. }
        | DatabaseOp::DeleteView { .. }
        | DatabaseOp::ReorderViews { .. }
        | DatabaseOp::MoveCard { .. } => None,
    }
}

/// The type change an op asks for, against the table's current version:
/// ops are last-write-wins.
fn change_column_type(
    entries: &[TableEntry],
    table: TableId,
    column: ColumnId,
    to: ColumnKind,
    clear_invalid: bool,
) -> Result<ChangeColumnType, DatabaseError> {
    let target = PropertyType::from_column_kind(to);
    Ok(ChangeColumnType {
        table_id: table,
        column_id: column,
        data_type: target.data_type,
        is_multi_select: target.is_multi_select,
        specific_entity_type: target.specific_entity_type,
        relation: match to {
            ColumnKind::Relation { database, table } => Some((database, table)),
            _ => None,
        },
        base_version: current_version(entries, table)?,
        clear_invalid,
    })
}

/// A row a relation cell points at, and where the op named it.
struct RelatedRow {
    table: TableId,
    row: RowId,
    op: usize,
    row_index: Option<usize>,
    column: ColumnId,
}

/// Turns ops into writes against one database's catalog, collecting the
/// options they create and the rows their relation cells point at.
struct Planner<'a> {
    entries: &'a [TableEntry],
    database_id: DatabaseId,
    /// The shared definitions whose options the viewer may change.
    editable: &'a [PropertyDefinitionId],
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

impl Planner<'_> {
    fn write(&mut self, index: usize, op: &DatabaseOp) -> Result<Write, DatabaseError> {
        let table = op.table();
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.table.id == table)
            .ok_or_else(|| refuse(index, None, None, "table is not in this database"))?;
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
            DatabaseOp::ChangeColumnType { .. }
            | DatabaseOp::UpdateOption { .. }
            | DatabaseOp::DeleteOption { .. }
            | DatabaseOp::CreateView { .. }
            | DatabaseOp::UpdateView { .. }
            | DatabaseOp::DeleteView { .. }
            | DatabaseOp::ReorderViews { .. }
            | DatabaseOp::MoveCard { .. } => 0,
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
            DatabaseOp::InsertRows {
                rows,
                create_missing_options,
                ..
            } => {
                let rows = rows
                    .iter()
                    .enumerate()
                    .map(|(row, cells)| {
                        let cells =
                            self.cells(entry, index, Some(row), cells, *create_missing_options)?;
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
                let cells = self.cells(entry, index, None, cells, *create_missing_options)?;
                Ok(Write::UpdateRows {
                    table_id: table,
                    rows: rows.iter().map(|row| (*row, cells.clone())).collect(),
                })
            }
            DatabaseOp::UpdateRows {
                changes: RowChanges::PerRow { rows },
                create_missing_options,
                ..
            } => Ok(Write::UpdateRows {
                table_id: table,
                rows: rows
                    .iter()
                    .enumerate()
                    .map(|(row, change)| {
                        let cells = self.cells(
                            entry,
                            index,
                            Some(row),
                            &change.cells,
                            *create_missing_options,
                        )?;
                        Ok((change.row, cells))
                    })
                    .collect::<Result<_, DatabaseError>>()?,
            }),
            DatabaseOp::DeleteRows { rows, .. } => {
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
            // A batch of one type change never reaches the planner.
            DatabaseOp::ChangeColumnType { column, .. } => Err(refuse(
                index,
                None,
                Some(*column),
                "a column type change is applied on its own; send it as the only op of its \
                 request",
            )),
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
                let column = self.option_column(entry, place)?;
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
                Ok(Write::UpdateOption {
                    table_id: table,
                    tables: self.tables_binding(column),
                    definition_id: column.definition.definition.id,
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
                let column = self.option_column(entry, place)?;
                self.known_option(place, column, *option)?;
                self.labels_of(&column.definition)
                    .retain(|(id, _)| id != option);
                let definition_id = column.definition.definition.id;
                let tables = self.tables_binding(column);
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
            | DatabaseOp::MoveCard { .. } => self.view_write(index, entry, op),
        }
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
        if column.shared_outside(self.database_id)
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
            .map_err(|error| match error {
                DatabaseError::InvalidSchemaOperation(reason) => place.refuse(reason.to_string()),
                other => other,
            })?
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

    /// Every table of the database whose columns bind the column's
    /// definition.
    fn tables_binding(&self, column: &ColumnEntry) -> Vec<TableId> {
        let definition = column.definition.definition.id;
        self.entries
            .iter()
            .filter(|entry| entry.column_for(definition).is_some())
            .map(|entry| entry.table.id)
            .collect()
    }
}
