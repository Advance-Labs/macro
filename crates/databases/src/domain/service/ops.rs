//! Typed ops: the batched write surface. Every op is checked against the
//! receipt's database before anything is written, and the writes they become
//! commit in one transaction.

mod views;

use models_databases::{
    CellValue, CellWrite, ColumnKind, DatabaseOp, OpResult, OptionRef, RowChanges,
};
use models_properties::service::property_definition_with_options::PropertyDefinitionWithOptions;
use models_properties::service::property_value::PropertyValue;
use models_properties::shared::EntityReference;

use super::column_types::is_complete_url;
use super::*;
use crate::domain::catalog::{ColumnEntry, entity_type};
use crate::domain::models::{
    CellChanges, DatabaseView, NewOption, OpRefusal, PropertyDefinitionId, RowId, ViewId, Write,
    Writes, WritesOutcome,
};
use chrono::DateTime;

/// Most rows one request inserts, updates and deletes in total.
const MAX_WRITTEN_ROWS: usize = 10_000;

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
            if let DatabaseOp::ChangeColumnType { column, .. } = op
                && ops.len() > 1
            {
                return Err(refuse(
                    index,
                    None,
                    Some(*column),
                    "a column type change is applied on its own; send it as the only op of its \
                     request",
                ));
            }
        }
        let attribution = events::Attribution::acting(viewer.user_id.clone(), viewer.acting_bot);
        if let [
            DatabaseOp::ChangeColumnType {
                table,
                column,
                to,
                clear_invalid,
            },
        ] = ops.as_slice()
        {
            let changed = self
                .retype_column(
                    database_id,
                    Some(attribution),
                    &viewer,
                    change_column_type(&entries, *table, *column, *to, *clear_invalid),
                )
                .await
                .map_err(|error| match error {
                    DatabaseError::InvalidSchemaOperation(reason) => {
                        refuse(0, None, Some(*column), reason)
                    }
                    DatabaseError::NotFound => {
                        refuse(0, None, Some(*column), "no such column in this table")
                    }
                    other => other,
                })?;
            return Ok(vec![OpResult::ColumnTyped {
                table_version: changed
                    .table_versions
                    .get(table)
                    .copied()
                    .unwrap_or_else(|| current_version(&entries, *table)),
                cleared_cells: count(changed.cleared_cells),
                trimmed_cells: count(changed.trimmed_cells),
            }]);
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
            created_by: viewer.user_id.as_ref().to_string(),
            options: planner.options,
            writes,
            related_rows: planner
                .related
                .iter()
                .map(|related| (related.table, related.row))
                .collect(),
        };
        let (inserted, table_versions) = match self
            .cells
            .apply_writes(&row_writes)
            .await
            .map_err(repo_err)?
        {
            WritesOutcome::Applied {
                inserted,
                table_versions,
            } => (inserted, table_versions),
            WritesOutcome::TableNotFound(_) => return Err(DatabaseError::NotFound),
            WritesOutcome::MissingOption { write } => {
                return Err(refuse(
                    write,
                    None,
                    option_column(&ops[write]),
                    "the option was removed by someone else; refresh and try again",
                ));
            }
            WritesOutcome::OptionLabelTaken { write } => {
                return Err(refuse(
                    write,
                    None,
                    option_column(&ops[write]),
                    "another option took that label first; refresh and try again",
                ));
            }
            WritesOutcome::MissingView { write } => {
                return Err(refuse(
                    write,
                    None,
                    None,
                    "the view was removed by someone else; refresh and try again",
                ));
            }
            WritesOutcome::ViewNameTaken { write } => {
                return Err(refuse(
                    write,
                    None,
                    None,
                    "another view took that name first; refresh and try again",
                ));
            }
            WritesOutcome::MissingRow { write, row } => {
                return Err(refuse(
                    write,
                    row_index(&ops[write], row),
                    None,
                    format!("no row {row} in this table"),
                ));
            }
            WritesOutcome::MissingRelatedRow(row) => {
                let origin = planner
                    .related
                    .iter()
                    .find(|related| related.row == row)
                    .ok_or_else(|| {
                        DatabaseError::Repo(
                            rootcause::report!("the cell store reported a row no op named")
                                .into_dynamic(),
                        )
                    })?;
                return Err(refuse(
                    origin.op,
                    origin.row_index,
                    Some(origin.column),
                    format!("row {row} is not a row of the related table"),
                ));
            }
        };

        self.publish(
            Some(attribution),
            &table_versions
                .keys()
                .map(|table| (*table, database_id))
                .collect(),
            &table_versions,
        )
        .await;
        Ok(row_writes
            .writes
            .iter()
            .zip(inserted)
            .map(|(write, inserted)| {
                let table_version = table_versions
                    .get(&write.table_id())
                    .copied()
                    .unwrap_or_else(|| current_version(&entries, write.table_id()));
                match write {
                    Write::InsertRows { .. }
                    | Write::UpdateRows { .. }
                    | Write::DeleteRows { .. } => OpResult::RowsWritten {
                        table_version,
                        inserted,
                        affected: count(write.affected()),
                    },
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
                }
            })
            .collect())
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
                .repo
                .row_refs(*table)
                .await
                .map_err(repo_err)?
                .into_iter()
                .map(|row| row.id)
                .collect();
            let cells = self
                .cells
                .column_cells(&rows, grouping)
                .await
                .map_err(repo_err)?;
            let positions = self.repo.view_positions(*view).await.map_err(repo_err)?;
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
            .map_err(repo_err)
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

/// Why options of a property shared beyond the database cannot change.
pub(super) fn shared_options_refusal(column: &str) -> String {
    format!(
        "\"{column}\" is a property shared beyond this database, and you may not change its \
         options"
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

fn current_version(entries: &[TableEntry], table: TableId) -> TableVersion {
    entries
        .iter()
        .find(|entry| entry.table.id == table)
        .map_or(TableVersion(0), |entry| entry.table.version)
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
) -> ChangeColumnType {
    let plain = |data_type, is_multi_select| (data_type, is_multi_select, None, None);
    let (data_type, is_multi_select, specific_entity_type, relation) = match to {
        ColumnKind::Text => plain(DataType::String, false),
        ColumnKind::Number => plain(DataType::Number, false),
        ColumnKind::Boolean => plain(DataType::Boolean, false),
        ColumnKind::Date => plain(DataType::Date, false),
        ColumnKind::Link => plain(DataType::Link, false),
        ColumnKind::Select { multi } => plain(DataType::SelectString, multi),
        ColumnKind::SelectNumber { multi } => plain(DataType::SelectNumber, multi),
        ColumnKind::Tag => plain(DataType::Tag, true),
        ColumnKind::Entity { target, multi } => {
            (DataType::Entity, multi, Some(entity_type(target)), None)
        }
        ColumnKind::Relation { database, table } => {
            (DataType::Entity, true, None, Some((database, table)))
        }
    };
    ChangeColumnType {
        table_id: table,
        column_id: column,
        data_type,
        is_multi_select,
        specific_entity_type,
        relation,
        base_version: current_version(entries, table),
        clear_invalid,
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
    labels: HashMap<PropertyDefinitionId, Vec<(Uuid, String)>>,
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
            DatabaseOp::ChangeColumnType { column, .. } => Err(refuse(
                index,
                None,
                Some(*column),
                "a column type change is applied on its own",
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
                    Some(color) => Some(color.map(|color| color.hex().to_string())),
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
            return Err(place.refuse(shared_options_refusal(column.name())));
        }
        Ok(())
    }

    /// Check the column has the option, as the ops so far leave it.
    fn known_option(
        &mut self,
        place: Place,
        column: &ColumnEntry,
        option: Uuid,
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
        option: Uuid,
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
                DatabaseError::InvalidSchemaOperation(reason) => place.refuse(reason),
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

    /// One row's cells as stored values; `None` empties a cell.
    fn cells(
        &mut self,
        entry: &TableEntry,
        op: usize,
        row: Option<usize>,
        cells: &[CellWrite],
        create_missing_options: bool,
    ) -> Result<CellChanges, DatabaseError> {
        let mut stored = Vec::with_capacity(cells.len());
        for (index, cell) in cells.iter().enumerate() {
            let place = Place {
                op,
                row,
                column: cell.column,
            };
            if cells[..index]
                .iter()
                .any(|earlier| earlier.column == cell.column)
            {
                return Err(place.refuse("the column is written twice"));
            }
            let column = entry
                .columns
                .iter()
                .find(|column| column.column.id == cell.column)
                .ok_or_else(|| place.refuse("no such column in this table"))?;
            let value = self.value(place, column, &cell.value, create_missing_options)?;
            stored.push((column.definition.definition.id, value));
        }
        Ok(stored)
    }

    /// A value as the properties system stores it in `column`, checked to
    /// fit the column's type the way a property value is.
    fn value(
        &mut self,
        place: Place,
        column: &ColumnEntry,
        value: &CellValue,
        create_missing_options: bool,
    ) -> Result<Option<PropertyValue>, DatabaseError> {
        let data_type = column.definition.definition.data_type;
        let misfit = || {
            place.refuse(format!(
                "\"{}\" is a {} column; {} does not fit it",
                column.name(),
                column_kind_name(column),
                value_kind_name(value)
            ))
        };
        let single = |values: usize| {
            if values > 1 && !column.is_multi() {
                Err(place.refuse(format!(
                    "\"{}\" holds one value; {values} were given",
                    column.name()
                )))
            } else {
                Ok(())
            }
        };
        match value {
            CellValue::Clear => Ok(None),
            CellValue::Text(text) if data_type == DataType::String => {
                Ok(Some(PropertyValue::Str(text.clone())))
            }
            CellValue::Number(number) if data_type == DataType::Number => {
                if !number.is_finite() {
                    return Err(place.refuse("a number must be finite"));
                }
                Ok(Some(PropertyValue::Num(*number)))
            }
            CellValue::Boolean(checked) if data_type == DataType::Boolean => {
                Ok(Some(PropertyValue::Bool(*checked)))
            }
            CellValue::Date(date) if data_type == DataType::Date => {
                Ok(Some(PropertyValue::Date(*date)))
            }
            CellValue::Link(urls) if data_type == DataType::Link => {
                single(urls.len())?;
                if let Some(bad) = urls.iter().find(|url| !is_complete_url(url)) {
                    return Err(
                        place.refuse(format!("`{bad}` is not a complete http or https URL"))
                    );
                }
                Ok((!urls.is_empty()).then(|| PropertyValue::Link(urls.clone())))
            }
            CellValue::Options(options) if takes_options(data_type) => {
                single(options.len())?;
                let mut ids: Vec<Uuid> = Vec::with_capacity(options.len());
                for option in options {
                    let id = self.option(place, column, option, create_missing_options)?;
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
                Ok((!ids.is_empty()).then_some(PropertyValue::SelectOption(ids)))
            }
            CellValue::Entities(references)
                if data_type == DataType::Entity && !column.is_relation() =>
            {
                single(references.len())?;
                let expected = column.definition.definition.specific_entity_type;
                let references = references
                    .iter()
                    .map(|reference| {
                        let stored = entity_type(reference.entity_type);
                        if Some(stored) != expected {
                            return Err(place.refuse(format!(
                                "\"{}\" points at {}; a {stored} reference does not fit it",
                                column.name(),
                                expected
                                    .map_or_else(|| "nothing".to_string(), |kind| kind.to_string())
                            )));
                        }
                        if reference.entity_id.trim().is_empty() {
                            return Err(place.refuse("an entity id must not be empty"));
                        }
                        Ok(EntityReference {
                            entity_id: reference.entity_id.clone(),
                            entity_type: stored,
                            specific_message_id: None,
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((!references.is_empty()).then_some(PropertyValue::EntityRef(references)))
            }
            CellValue::Rows(rows) => {
                let Some(ColumnConfig::Link {
                    table_id: target, ..
                }) = column.column.config
                else {
                    return Err(misfit());
                };
                let mut references = Vec::with_capacity(rows.len());
                for row in rows {
                    self.related.push(RelatedRow {
                        table: target,
                        row: *row,
                        op: place.op,
                        row_index: place.row,
                        column: place.column,
                    });
                    references.push(EntityReference {
                        entity_id: row.to_string(),
                        entity_type: models_properties::EntityType::DatabaseRow,
                        specific_message_id: None,
                    });
                }
                Ok((!references.is_empty()).then_some(PropertyValue::EntityRef(references)))
            }
            _ => Err(misfit()),
        }
    }

    /// The id of the option a reference names: one the column has, one this
    /// batch already created, or, when the op creates missing options, a new
    /// one.
    fn option(
        &mut self,
        place: Place,
        column: &ColumnEntry,
        option: &OptionRef,
        create_missing_options: bool,
    ) -> Result<Uuid, DatabaseError> {
        let definition = &column.definition;
        let data_type = definition.definition.data_type;
        let label = match option {
            OptionRef::Id(id) => {
                return if self
                    .labels_of(definition)
                    .iter()
                    .any(|(option, _)| option == id)
                {
                    Ok(*id)
                } else {
                    Err(place.refuse(format!("no option {id} on \"{}\"", column.name())))
                };
            }
            OptionRef::Label(label) => label,
        };
        let key = label_key(data_type, label);
        if let Some((id, _)) = self
            .labels_of(definition)
            .iter()
            .find(|(_, existing)| label_key(data_type, existing) == key)
        {
            return Ok(*id);
        }
        if !create_missing_options {
            return Err(place.refuse(format!(
                "`{label}` is not an option of \"{}\"",
                column.name()
            )));
        }
        self.may_change_options(place, column)?;
        let value = validate_option_labels(data_type, std::slice::from_ref(label), &[])
            .map_err(|error| match error {
                DatabaseError::InvalidSchemaOperation(reason) => place.refuse(reason),
                other => other,
            })?
            .into_iter()
            .next()
            .ok_or_else(|| place.refuse("an option label must not be empty"))?;
        let id = macro_uuid::generate_uuid_v7();
        self.labels_of(definition)
            .push((id, catalog::option_display(&value)));
        self.options.push(NewOption {
            definition_id: definition.definition.id,
            id,
            value,
        });
        Ok(id)
    }
}

impl Planner<'_> {
    /// The options of a definition as the ops planned so far leave them.
    fn labels_of(
        &mut self,
        definition: &PropertyDefinitionWithOptions,
    ) -> &mut Vec<(Uuid, String)> {
        self.labels
            .entry(definition.definition.id)
            .or_insert_with(|| catalog::option_labels(definition))
    }
}

/// What labels match on: case-insensitive text, or for a numeric select the
/// number as its label shows it, so `2.0` names the option `2`.
fn label_key(data_type: DataType, label: &str) -> String {
    match label.trim().parse::<f64>() {
        Ok(number) if data_type == DataType::SelectNumber && number.is_finite() => {
            models_databases::cast::number_label(number)
        }
        _ => option_key(label),
    }
}

fn column_kind_name(column: &ColumnEntry) -> &'static str {
    match column.definition.definition.data_type {
        DataType::String => "text",
        DataType::Number => "number",
        DataType::Boolean => "checkbox",
        DataType::Date => "date",
        DataType::Link => "link",
        DataType::SelectString => "select",
        DataType::SelectNumber => "numeric select",
        DataType::Tag => "tag",
        DataType::Entity if column.is_relation() => "relation",
        DataType::Entity => "reference",
    }
}

fn value_kind_name(value: &CellValue) -> &'static str {
    match value {
        CellValue::Text(_) => "text",
        CellValue::Number(_) => "a number",
        CellValue::Boolean(_) => "true or false",
        CellValue::Date(_) => "a date",
        CellValue::Link(_) => "a link",
        CellValue::Options(_) => "an option",
        CellValue::Entities(_) => "a reference",
        CellValue::Rows(_) => "a row",
        CellValue::Clear => "nothing",
    }
}
