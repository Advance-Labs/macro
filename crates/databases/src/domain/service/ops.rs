//! Typed ops: the batched write surface. Every op is checked against the
//! receipt's database before anything is written, and the row writes they
//! become commit in one transaction.

use models_databases::{
    CellValue, CellWrite, ColumnKind, DatabaseOp, EntityKind, OpResult, OptionRef, RowChanges,
};
use models_properties::service::property_value::PropertyValue;
use models_properties::shared::EntityReference;

use super::column_types::is_complete_url;
use super::*;
use crate::domain::catalog::ColumnEntry;
use crate::domain::models::{
    CellChanges, NewOption, OpRefusal, PropertyDefinitionId, RowId, RowWrite, RowWrites,
    RowWritesOutcome,
};

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
        self.apply_ops_with_grant(database_id, grant, viewer, ops)
            .await
    }

    /// Apply ops as a viewer already known to hold `grant` on the database:
    /// the receipt's, or the one a SQL statement's catalog was built with.
    pub(super) async fn apply_ops_with_grant(
        &self,
        database_id: DatabaseId,
        grant: AccessLevel,
        viewer: Viewer,
        ops: Vec<DatabaseOp>,
    ) -> Result<Vec<OpResult>, DatabaseError> {
        let entries = self
            .entries_for(&HashMap::from([(database_id, grant)]))
            .await
            .map_err(|error| DatabaseError::Repo(rootcause::Report::new(error).into_dynamic()))?;
        if entries.is_empty() {
            return Err(DatabaseError::NotFound);
        }
        for (index, op) in ops.iter().enumerate() {
            let table = op_table(op);
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

        let mut planner = Planner {
            entries: &entries,
            options: Vec::new(),
            created: HashMap::new(),
            related: Vec::new(),
            written_rows: 0,
        };
        let writes = ops
            .iter()
            .enumerate()
            .map(|(index, op)| planner.write(index, op))
            .collect::<Result<Vec<_>, _>>()?;
        let row_writes = RowWrites {
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
            .apply_row_writes(&row_writes)
            .await
            .map_err(repo_err)?
        {
            RowWritesOutcome::Applied {
                inserted,
                table_versions,
            } => (inserted, table_versions),
            RowWritesOutcome::TableNotFound(_) => return Err(DatabaseError::NotFound),
            RowWritesOutcome::MissingRow { write, row } => {
                return Err(refuse(
                    write,
                    row_index(&ops[write], row),
                    None,
                    format!("no row {row} in this table"),
                ));
            }
            RowWritesOutcome::MissingRelatedRow(row) => {
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
            .map(|(write, inserted)| OpResult::RowsWritten {
                table_version: table_versions
                    .get(&write.table_id())
                    .copied()
                    .unwrap_or_else(|| current_version(&entries, write.table_id())),
                inserted,
                affected: count(write.affected()),
            })
            .collect())
    }
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

fn op_table(op: &DatabaseOp) -> TableId {
    match op {
        DatabaseOp::InsertRows { table, .. }
        | DatabaseOp::UpdateRows { table, .. }
        | DatabaseOp::DeleteRows { table, .. }
        | DatabaseOp::ChangeColumnType { table, .. } => *table,
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
        DatabaseOp::InsertRows { .. } | DatabaseOp::ChangeColumnType { .. } => None,
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

/// The properties system's name for what a reference points at.
fn entity_type(kind: EntityKind) -> models_properties::EntityType {
    use models_properties::EntityType as Stored;
    match kind {
        EntityKind::User => Stored::User,
        EntityKind::Document => Stored::Document,
        EntityKind::Task => Stored::Task,
        EntityKind::Company => Stored::Company,
        EntityKind::CallRecord => Stored::CallRecord,
        EntityKind::Channel => Stored::Channel,
        EntityKind::Chat => Stored::Chat,
        EntityKind::Project => Stored::Project,
        EntityKind::Thread => Stored::Thread,
        EntityKind::CalendarEvent => Stored::CalendarEvent,
        EntityKind::Initiative => Stored::Initiative,
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

/// Turns ops into row writes against one database's catalog, collecting the
/// options they create and the rows their relation cells point at.
struct Planner<'a> {
    entries: &'a [TableEntry],
    options: Vec<NewOption>,
    /// Options created so far, per definition, by the key labels match on.
    created: HashMap<PropertyDefinitionId, Vec<(String, Uuid)>>,
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
    fn write(&mut self, index: usize, op: &DatabaseOp) -> Result<RowWrite, DatabaseError> {
        let table = op_table(op);
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
            DatabaseOp::ChangeColumnType { .. } => 0,
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
                Ok(RowWrite::Insert {
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
                Ok(RowWrite::Update {
                    table_id: table,
                    rows: rows.iter().map(|row| (*row, cells.clone())).collect(),
                })
            }
            DatabaseOp::UpdateRows {
                changes: RowChanges::PerRow { rows },
                create_missing_options,
                ..
            } => Ok(RowWrite::Update {
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
                Ok(RowWrite::Delete {
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
        }
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
                return if definition
                    .property_options
                    .iter()
                    .any(|option| option.id == *id)
                {
                    Ok(*id)
                } else {
                    Err(place.refuse(format!("no option {id} on \"{}\"", column.name())))
                };
            }
            OptionRef::Label(label) => label,
        };
        let key = label_key(data_type, label);
        let existing = catalog::option_labels(definition);
        if let Some((id, _)) = existing
            .iter()
            .find(|(_, existing)| label_key(data_type, existing) == key)
        {
            return Ok(*id);
        }
        let created = self.created.entry(definition.definition.id).or_default();
        if let Some((_, id)) = created.iter().find(|(created, _)| *created == key) {
            return Ok(*id);
        }
        if !create_missing_options {
            return Err(place.refuse(format!(
                "`{label}` is not an option of \"{}\"",
                column.name()
            )));
        }
        let value = validate_option_labels(data_type, std::slice::from_ref(label), &[])
            .map_err(|error| match error {
                DatabaseError::InvalidSchemaOperation(reason) => place.refuse(reason),
                other => other,
            })?
            .into_iter()
            .next()
            .ok_or_else(|| place.refuse("an option label must not be empty"))?;
        let id = macro_uuid::generate_uuid_v7();
        created.push((key, id));
        self.options.push(NewOption {
            definition_id: definition.definition.id,
            id,
            value,
        });
        Ok(id)
    }
}

/// What labels match on: case-insensitive text, or for a numeric select the
/// number as its label shows it, so `2.0` names the option `2`.
fn label_key(data_type: DataType, label: &str) -> String {
    match label.trim().parse::<f64>() {
        Ok(number) if data_type == DataType::SelectNumber && number.is_finite() => {
            catalog::format_number(number)
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
