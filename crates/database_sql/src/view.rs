//! Typed views on the engine: a view compiles straight to the
//! [`SelectQuery`](crate::resolve::SelectQuery) its SQL would resolve to,
//! reads back as that SQL, and lays its rows out as a board.

mod board;
mod compile;
mod sql;

use models_databases::views::{DatabaseView, SchemaColumn, ValueKind, ViewProblem, check};

use crate::catalog::{Catalog, Column, ColumnKind, Table};

pub use board::{Board, BoardLane, board};
pub use compile::compile_view;
pub use sql::view_as_sql;

/// The view's table, once the view checks out against it.
fn checked_table<'catalog>(
    view: &DatabaseView,
    catalog: &'catalog Catalog,
) -> Result<&'catalog Table, ViewProblem> {
    let table = catalog
        .tables
        .iter()
        .find(|table| table.id == view.table_id)
        .ok_or(ViewProblem::UnknownTable {
            table: view.table_id,
        })?;
    let columns: Vec<SchemaColumn> = table.columns.iter().map(schema_column).collect();
    check(&view.query, &view.layout, &columns)?;
    Ok(table)
}

fn schema_column(column: &Column) -> SchemaColumn {
    let (values, options) = match &column.kind {
        ColumnKind::Text | ColumnKind::Link => (ValueKind::Text, Vec::new()),
        ColumnKind::Number => (ValueKind::Number, Vec::new()),
        ColumnKind::Date => (ValueKind::Date, Vec::new()),
        ColumnKind::Boolean => (ValueKind::Checkbox, Vec::new()),
        ColumnKind::Select { options, .. } => (
            ValueKind::Options,
            options.iter().map(|option| option.id).collect(),
        ),
        ColumnKind::Entity { .. } => (ValueKind::Entities, Vec::new()),
    };
    SchemaColumn {
        id: column.placement,
        name: column.name.clone(),
        values,
        multi: column.kind.is_multi(),
        options,
    }
}

/// The column a checked view names by placement.
fn placed(table: &Table, placement: uuid::Uuid) -> &Column {
    table
        .columns
        .iter()
        .find(|column| column.placement == placement)
        .expect("the view was checked against this table")
}
