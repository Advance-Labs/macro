//! What a statement did, shaped for agents: a result set whose values are
//! the scalars a model reads (labels for options, ids for references), the
//! versions a follow-up write can guard on, and what a write changed.

use std::collections::HashMap;

use database_sql::catalog::{ColumnKind, EntityKind};
use database_sql::fold::Cell;
use database_sql::run::{Outcome, OutcomeKind};
use databases::domain::models::{ColumnId, RowId, TableId, TableVersion};
use model_entity::EntityType;
use serde_json::Value;

use crate::catalog::ViewerCatalog;

/// The answer to one statement.
#[derive(Debug, Clone, PartialEq)]
pub struct SqlOutcome {
    /// The rows a `SELECT` returned; `None` for a write.
    pub result: Option<ResultSet>,
    /// How many rows a write changed.
    pub changes_applied: usize,
    /// The rows an `INSERT` created, in statement order.
    pub inserted_row_ids: Vec<RowId>,
    /// The new version of every table written.
    pub new_versions: HashMap<TableId, TableVersion>,
    /// The version of every table read, for a follow-up write to guard on.
    pub read_versions: HashMap<TableId, TableVersion>,
    /// Tables whose read hit the engine's row cap, so aggregates over them
    /// are partial.
    pub truncated_tables: Vec<String>,
    /// The column an `ALTER COLUMN … TYPE` changed.
    pub altered_column: Option<AlteredColumn>,
}

/// A `SELECT`'s rows. A row-shaped result leads with `row_id`.
#[derive(Debug, Clone, PartialEq)]
pub struct ResultSet {
    /// The columns, in select-list order.
    pub columns: Vec<ResultColumn>,
    /// One JSON scalar per column per row.
    pub rows: Vec<Vec<Value>>,
}

/// One result column.
#[derive(Debug, Clone, PartialEq)]
pub struct ResultColumn {
    /// The name or alias the statement gave it.
    pub name: String,
    /// What its ids refer to, when it holds entity ids.
    pub entity_type: Option<EntityType>,
}

/// A column an `ALTER COLUMN … TYPE` changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlteredColumn {
    /// The table.
    pub table_id: TableId,
    /// The column placement; its id survives the change.
    pub column_id: ColumnId,
    /// The column's name.
    pub name: String,
    /// The type it became, as SQL spells it.
    pub to: String,
    /// Cells `USING NULL` emptied because their value did not fit.
    pub cleared_cells: usize,
    /// Cells that held several values and kept only their first.
    pub trimmed_cells: usize,
}

/// The engine's outcome in the viewer's terms.
pub(crate) fn shape(
    catalog: &ViewerCatalog,
    outcome: &Outcome,
    new_versions: HashMap<TableId, TableVersion>,
) -> SqlOutcome {
    let read_versions = outcome
        .read_tables
        .iter()
        .filter_map(|table| {
            catalog
                .table(*table)
                .map(|(_, detail)| (*table, detail.table.version))
        })
        .collect();
    let truncated_tables = if outcome.truncated {
        outcome
            .read_tables
            .iter()
            .filter_map(|table| catalog.table(*table))
            .map(|(_, detail)| detail.table.name.clone())
            .collect()
    } else {
        Vec::new()
    };
    SqlOutcome {
        result: result_set(catalog, outcome),
        changes_applied: outcome.changes_applied as usize,
        inserted_row_ids: outcome.inserted_row_ids.clone(),
        new_versions,
        read_versions,
        truncated_tables,
        altered_column: outcome.altered_column.as_ref().and_then(|altered| {
            let (_, table) = catalog.table(altered.table)?;
            let column = table
                .columns
                .iter()
                .find(|column| column.definition.definition.id == altered.column)?;
            Some(AlteredColumn {
                table_id: altered.table,
                column_id: column.column.id,
                name: column
                    .column
                    .display_name
                    .clone()
                    .unwrap_or_else(|| column.definition.definition.display_name.clone()),
                to: altered.to.clone(),
                cleared_cells: altered.cleared_cells,
                trimmed_cells: altered.trimmed_cells,
            })
        }),
    }
}

fn result_set(catalog: &ViewerCatalog, outcome: &Outcome) -> Option<ResultSet> {
    if outcome.columns.is_empty() {
        return None;
    }
    let row_shaped = outcome.row_ids.len() == outcome.rows.len() && !outcome.rows.is_empty()
        || (outcome.rows.is_empty() && outcome.columns.iter().all(|c| c.column.is_some()));
    let kinds: Vec<Option<&ColumnKind>> = outcome
        .columns
        .iter()
        .map(|column| {
            column
                .column
                .and_then(|definition| catalog.column(definition))
                .map(|column| &column.kind)
        })
        .collect();

    let mut columns = Vec::with_capacity(outcome.columns.len() + 1);
    if row_shaped {
        columns.push(ResultColumn {
            name: "row_id".into(),
            entity_type: None,
        });
    }
    columns.extend(
        outcome
            .columns
            .iter()
            .zip(&kinds)
            .map(|(column, kind)| ResultColumn {
                name: column.name.clone(),
                entity_type: match (column.kind, kind) {
                    (OutcomeKind::Entity, Some(ColumnKind::Entity { target, .. })) => {
                        Some(entity_type(*target))
                    }
                    _ => None,
                },
            }),
    );

    let rows = outcome
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let mut values = Vec::with_capacity(row.len() + 1);
            if row_shaped {
                values.push(Value::String(outcome.row_ids[index].to_string()));
            }
            values.extend(
                row.iter()
                    .zip(&kinds)
                    .map(|(cell, kind)| value(cell.as_ref(), *kind)),
            );
            values
        })
        .collect();
    Some(ResultSet { columns, rows })
}

/// A result cell as the scalar a model reads: labels for options, ids for
/// references, a JSON array (as text) for a multi-valued cell.
fn value(cell: Option<&Cell>, kind: Option<&ColumnKind>) -> Value {
    let Some(cell) = cell else {
        return Value::Null;
    };
    let multi = kind.is_some_and(ColumnKind::is_multi);
    match cell {
        Cell::Text(text) => Value::String(text.clone()),
        Cell::Number(number) => serde_json::Number::from_f64(*number)
            .map(Value::Number)
            // A non-finite number has no JSON spelling; its text form is
            // better than a null that reads as an empty cell.
            .unwrap_or_else(|| Value::String(number.to_string())),
        Cell::Bool(checked) => Value::from(i64::from(*checked)),
        Cell::Date(date) => Value::String(date.to_rfc3339()),
        Cell::Options(ids) => {
            let options = match kind {
                Some(ColumnKind::Select { options, .. }) => options.as_slice(),
                _ => &[],
            };
            let labels = ids
                .iter()
                .map(|id| {
                    options
                        .iter()
                        .find(|option| option.id == *id)
                        .map_or_else(|| id.to_string(), |option| option.label.clone())
                })
                .collect();
            scalar_or_array(labels, multi)
        }
        Cell::Entities(ids) => scalar_or_array(ids.clone(), multi),
    }
}

fn scalar_or_array(values: Vec<String>, multi: bool) -> Value {
    if multi {
        Value::String(Value::Array(values.into_iter().map(Value::String).collect()).to_string())
    } else {
        values.into_iter().next().map_or(Value::Null, Value::String)
    }
}

/// The platform entity type an entity column's ids refer to.
fn entity_type(target: EntityKind) -> EntityType {
    match target {
        EntityKind::User => EntityType::User,
        EntityKind::Document | EntityKind::Task => EntityType::Document,
        EntityKind::Company => EntityType::CrmCompany,
        EntityKind::CallRecord => EntityType::Call,
        EntityKind::Channel => EntityType::Channel,
        EntityKind::Chat => EntityType::Chat,
        EntityKind::Project => EntityType::Project,
        EntityKind::Thread => EntityType::EmailThread,
        EntityKind::CalendarEvent => EntityType::CalendarEvent,
        EntityKind::Initiative => EntityType::Initiative,
        EntityKind::Row => EntityType::DatabaseRow,
    }
}
