//! Writes as ops: a resolved statement, with the rows its read found, becomes
//! the one [`DatabaseOp`] the driver applies, and the op's result becomes the
//! statement's [`Outcome`].

use models_databases::{
    CellValue, CellWrite, ColumnKind as OpColumnKind, DatabaseOp, EntityKind as OpEntityKind,
    EntityRef, OpResult, OptionRef, RowChange, RowChanges,
};
use uuid::Uuid;

use crate::catalog::{Catalog, Column, ColumnKind, EntityKind, Table};
use crate::fold::Cell;
use crate::resolve::{
    AlterColumnTypeQuery, Assigned, InsertQuery, SelectItem, UpdateQuery, Value, column_key,
};
use crate::run::{AlteredColumn, Outcome, RunError};

/// What was sent, so its result can be read back into an outcome.
#[derive(Debug, Clone)]
pub(crate) enum Sent {
    /// Rows inserted, updated or deleted.
    Rows,
    /// A column's type change.
    Column {
        table: Uuid,
        column: Uuid,
        to: OpColumnKind,
    },
}

/// `INSERT`: every row in one op. Labels name options; none is created.
pub(crate) fn insert(catalog: &Catalog, query: &InsertQuery) -> DatabaseOp {
    let table = table(catalog, query.table);
    DatabaseOp::InsertRows {
        table: query.table,
        rows: query
            .rows
            .iter()
            .map(|cells| {
                cells
                    .iter()
                    .map(|(definition, value)| {
                        let column = column(table, *definition);
                        CellWrite {
                            column: column.placement,
                            value: cell_value(column, Some(value)),
                        }
                    })
                    .collect()
            })
            .collect(),
        create_missing_options: false,
    }
}

/// `UPDATE` of the rows its read found: the same cells for every row when
/// every value is a literal, else each row its own. `None` when the read
/// found nothing.
pub(crate) fn update(
    catalog: &Catalog,
    query: &UpdateQuery,
    found: &Outcome,
) -> Option<DatabaseOp> {
    if found.row_ids.is_empty() {
        return None;
    }
    let table = table(catalog, query.table);
    let uniform = query
        .assignments
        .iter()
        .all(|assignment| matches!(assignment.value, Assigned::Value(_)));
    let cells = |row: &[Option<Cell>]| -> Vec<CellWrite> {
        query
            .assignments
            .iter()
            .map(|assignment| {
                let target = column(table, assignment.column);
                let value = match &assignment.value {
                    Assigned::Value(value) => cell_value(target, value.as_ref()),
                    Assigned::Column(definition) => {
                        let item = query
                            .read
                            .items
                            .iter()
                            .position(|item| {
                                *item == SelectItem::Column(column_key(0, *definition))
                            })
                            .expect("the read selects every copied column");
                        copied_value(target, column(table, *definition), row[item].as_ref())
                    }
                };
                CellWrite {
                    column: target.placement,
                    value,
                }
            })
            .collect()
    };
    let changes = if uniform {
        RowChanges::Uniform {
            rows: found.row_ids.clone(),
            cells: cells(&[]),
        }
    } else {
        RowChanges::PerRow {
            rows: found
                .row_ids
                .iter()
                .zip(&found.rows)
                .map(|(row, values)| RowChange {
                    row: *row,
                    cells: cells(values),
                })
                .collect(),
        }
    };
    Some(DatabaseOp::UpdateRows {
        table: query.table,
        changes,
        create_missing_options: false,
    })
}

/// `DELETE` of the rows its read found; `None` when it found nothing.
pub(crate) fn delete(table: Uuid, found: &Outcome) -> Option<DatabaseOp> {
    (!found.row_ids.is_empty()).then(|| DatabaseOp::DeleteRows {
        table,
        rows: found.row_ids.clone(),
    })
}

/// `ALTER COLUMN … TYPE`.
pub(crate) fn alter(catalog: &Catalog, query: &AlterColumnTypeQuery) -> DatabaseOp {
    DatabaseOp::ChangeColumnType {
        table: query.table,
        column: column(table(catalog, query.table), query.column).placement,
        to: query.to,
        clear_invalid: query.clear_invalid,
    }
}

/// The outcome the results of what was sent make.
pub(crate) fn outcome(sent: &Sent, results: &[OpResult]) -> Result<Outcome, RunError> {
    let [result] = results else {
        return Err(RunError::Results {
            message: format!("one op was sent, but {} results came back", results.len()),
        });
    };
    match (sent, result) {
        (
            Sent::Rows,
            OpResult::RowsWritten {
                inserted, affected, ..
            },
        ) => Ok(Outcome {
            inserted_row_ids: inserted.clone(),
            changes_applied: *affected,
            ..Outcome::default()
        }),
        (
            Sent::Column { table, column, to },
            OpResult::ColumnTyped {
                cleared_cells,
                trimmed_cells,
                ..
            },
        ) => Ok(Outcome {
            altered_column: Some(AlteredColumn {
                table: *table,
                column: *column,
                to: to.to_string(),
                cleared_cells: *cleared_cells as usize,
                trimmed_cells: *trimmed_cells as usize,
            }),
            ..Outcome::default()
        }),
        (Sent::Rows, OpResult::ColumnTyped { .. }) => Err(RunError::Results {
            message: "rows were written, but a column type change came back".into(),
        }),
        (Sent::Column { .. }, OpResult::RowsWritten { .. }) => Err(RunError::Results {
            message: "a column type change was sent, but rows written came back".into(),
        }),
        (_, OpResult::OptionChanged { .. }) => Err(RunError::Results {
            message: "a statement changes no option, but an option change came back".into(),
        }),
    }
}

fn table(catalog: &Catalog, id: Uuid) -> &Table {
    catalog
        .tables
        .iter()
        .find(|table| table.id == id)
        .expect("the statement was resolved against this catalog")
}

fn column(table: &Table, definition: Uuid) -> &Column {
    table
        .columns
        .iter()
        .find(|column| column.id == definition)
        .expect("the statement was resolved against this catalog")
}

/// A typed literal as the cell value it writes; `None` clears the cell.
fn cell_value(column: &Column, value: Option<&Value>) -> CellValue {
    let Some(value) = value else {
        return CellValue::Clear;
    };
    match value {
        Value::Text(text) if column.kind == ColumnKind::Link => CellValue::Link(vec![text.clone()]),
        Value::Text(text) => CellValue::Text(text.clone()),
        Value::Number(number) => CellValue::Number(*number),
        Value::Bool(checked) => CellValue::Boolean(*checked),
        Value::Date(date) => CellValue::Date(*date),
        Value::Option(id) => options(column, std::slice::from_ref(id)),
        Value::Options(ids) => options(column, ids),
        Value::Entity(id) => references(column, std::slice::from_ref(id)),
        Value::Entities(ids) => references(column, ids),
    }
}

/// Another column's cell, as the value it writes into `target`; the resolver
/// allows only columns of the same kind.
fn copied_value(target: &Column, source: &Column, cell: Option<&Cell>) -> CellValue {
    let Some(cell) = cell else {
        return CellValue::Clear;
    };
    match cell {
        Cell::Text(text) if target.kind == ColumnKind::Link => {
            CellValue::Link(text.split_whitespace().map(str::to_owned).collect())
        }
        Cell::Text(text) => CellValue::Text(text.clone()),
        Cell::Number(number) => CellValue::Number(*number),
        Cell::Bool(checked) => CellValue::Boolean(*checked),
        Cell::Date(date) => CellValue::Date(*date),
        Cell::Options(ids) => options(source, ids),
        Cell::Entities(ids) => references(target, ids),
    }
}

/// Options by label, the way a statement names them.
fn options(column: &Column, ids: &[Uuid]) -> CellValue {
    let ColumnKind::Select {
        options: labels, ..
    } = &column.kind
    else {
        unreachable!("only select columns hold options")
    };
    CellValue::Options(
        ids.iter()
            .map(|id| {
                labels
                    .iter()
                    .find(|option| option.id == *id)
                    .map_or(OptionRef::Id(*id), |option| {
                        OptionRef::Label(option.label.clone())
                    })
            })
            .collect(),
    )
}

/// Entity references, or related rows for a relation.
fn references(column: &Column, ids: &[String]) -> CellValue {
    match column.kind {
        ColumnKind::Entity {
            target: EntityKind::Row,
            ..
        } => CellValue::Rows(
            ids.iter()
                .filter_map(|id| Uuid::parse_str(id).ok())
                .collect(),
        ),
        ColumnKind::Entity { target, .. } => CellValue::Entities(
            ids.iter()
                .map(|id| EntityRef {
                    entity_type: entity_kind(target),
                    entity_id: id.clone(),
                })
                .collect(),
        ),
        _ => unreachable!("only entity columns hold references"),
    }
}

/// What a reference points at, as an op names it; a relation is written as
/// rows, never as references.
pub(crate) fn entity_kind(kind: EntityKind) -> OpEntityKind {
    match kind {
        EntityKind::User => OpEntityKind::User,
        EntityKind::Document => OpEntityKind::Document,
        EntityKind::Task => OpEntityKind::Task,
        EntityKind::Company => OpEntityKind::Company,
        EntityKind::CallRecord => OpEntityKind::CallRecord,
        EntityKind::Channel => OpEntityKind::Channel,
        EntityKind::Chat => OpEntityKind::Chat,
        EntityKind::Project => OpEntityKind::Project,
        EntityKind::Thread => OpEntityKind::Thread,
        EntityKind::CalendarEvent => OpEntityKind::CalendarEvent,
        EntityKind::Initiative => OpEntityKind::Initiative,
        EntityKind::Row => unreachable!("a relation is written as rows, not references"),
    }
}
