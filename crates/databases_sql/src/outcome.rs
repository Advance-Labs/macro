//! What a statement did, shaped for agents and the chat that shows them:
//! typed cells with what renders them, versions read and written, and changes.

use std::collections::HashMap;

use database_sql::catalog::{ColumnKind, EntityKind, SelectOption};
use database_sql::fold::Cell;
use database_sql::run::{Outcome, OutcomeKind};
use databases::domain::models::{ColumnId, RowId, TableId, TableVersion};
use serde::Serialize;

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
    /// Every table the statement read, when any read hit the engine's row
    /// cap: the engine reports the cap per statement, so aggregates over any
    /// of them may be partial.
    pub truncated_tables: Vec<String>,
    /// The column an `ALTER COLUMN … TYPE` changed.
    pub altered_column: Option<AlteredColumn>,
}

/// A `SELECT`'s rows, as the engine's typed cells.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ai_tools", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ResultSet {
    /// The columns, in select-list order.
    pub columns: Vec<ResultColumn>,
    /// One cell per column per row; `null` is an empty cell.
    pub rows: Vec<Vec<Option<Cell>>>,
    /// For a row-shaped result, the id of the row behind each result row;
    /// empty for an aggregate.
    pub row_ids: Vec<RowId>,
}

/// One result column, with what its cells mean.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ai_tools", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ResultColumn {
    /// The name or alias the statement gave it.
    pub name: String,
    /// What its cells hold.
    pub kind: OutcomeKind,
    /// For a select column, its options: its cells hold their ids.
    // `default` is what marks the field optional in the tool's JSON Schema.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<SelectOption>,
    /// For an entity column, what its ids point at; `DATABASE_ROW` for a
    /// relation, whose ids are rows of another table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<EntityKind>,
    /// For a relation, the table its rows belong to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related_table: Option<TableId>,
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
                name: column.name().to_string(),
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
    let row_shaped = outcome.row_ids.len() == outcome.rows.len();
    Some(ResultSet {
        columns: outcome
            .columns
            .iter()
            .map(|column| {
                let kind = column
                    .column
                    .and_then(|definition| catalog.column(definition))
                    .map(|column| &column.kind);
                ResultColumn {
                    name: column.name.clone(),
                    kind: column.kind,
                    options: match (column.kind, kind) {
                        (OutcomeKind::Select, Some(ColumnKind::Select { options, .. })) => {
                            options.clone()
                        }
                        _ => Vec::new(),
                    },
                    target: match (column.kind, kind) {
                        (OutcomeKind::Entity, Some(ColumnKind::Entity { target, .. })) => {
                            Some(*target)
                        }
                        _ => None,
                    },
                    related_table: match (column.kind, kind, column.column) {
                        (
                            OutcomeKind::Entity,
                            Some(ColumnKind::Entity {
                                target: EntityKind::Row,
                                ..
                            }),
                            Some(definition),
                        ) => catalog.related_table(definition),
                        _ => None,
                    },
                }
            })
            .collect(),
        rows: outcome.rows.clone(),
        row_ids: if row_shaped {
            outcome.row_ids.clone()
        } else {
            Vec::new()
        },
    })
}
