//! Stage five: finish a [`Plan`] over the rows (or bins) the server returned.
//!
//! Pure: rows in, result rows out. Applies the residual filter, groups and
//! aggregates or projects, then sorts. SQL semantics where SQL has an
//! opinion (`NULL` compares false, `COUNT(column)` skips empty cells, `SUM`
//! of nothing is `NULL`); Macro's where SQL does not (`LIKE` ignores case,
//! empty cells sort last, select values sort in option order).

mod aggregate;
mod predicate;
mod sort;
#[cfg(test)]
mod test;

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::catalog::Catalog;
use crate::resolve::{AggFn, OrderKey, SelectItem};
use crate::split::{Plan, Shape};

/// A cell as fetched. An absent cell is `NULL`; an absent multi-valued cell
/// is the empty set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum Cell {
    /// Text or link.
    Text(String),
    /// A number.
    Number(f64),
    /// A checkbox.
    Bool(bool),
    /// A date-time.
    Date(DateTime<Utc>),
    /// The selected option ids; one for single-select columns.
    Options(Vec<Uuid>),
    /// The referenced entity ids; one for single-valued columns.
    Entities(Vec<String>),
}

/// One fetched row: the entity id and the cells the plan asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The row entity id.
    pub id: Uuid,
    /// Cells by column; a missing column is an empty cell.
    pub cells: HashMap<Uuid, Cell>,
}

/// One `groupSoup` bin: the grouped value and how many rows it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Bin {
    /// The group's value; `None` for rows with an empty cell.
    pub key: Option<Cell>,
    /// Rows in the group.
    pub count: u64,
}

/// The result: one `Vec` per row, one `Option<Cell>` per select item, in
/// select-list order.
pub type Table = Vec<Vec<Option<Cell>>>;

/// Finish a plan whose query fetched rows.
pub fn fold_rows(catalog: &Catalog, plan: &Plan, rows: Vec<Row>) -> Table {
    let rows: Vec<Row> = match &plan.residual {
        Some(filter) => rows
            .into_iter()
            .filter(|row| predicate::holds(filter, row))
            .collect(),
        None => rows,
    };

    match &plan.shape {
        Shape::Rows(columns) => {
            let mut rows = rows;
            sort::rows(catalog, &mut rows, &plan.order_by);
            rows.into_iter()
                .map(|mut row| {
                    columns
                        .iter()
                        .map(|column| row.cells.remove(column))
                        .collect()
                })
                .collect()
        }
        Shape::Aggregate { group_by, items } => {
            let mut groups = aggregate::groups(rows, *group_by, items);
            sort::groups(catalog, &mut groups, &plan.order_by, *group_by, items);
            groups.into_iter().map(|group| group.cells).collect()
        }
    }
}

/// Finish a `GroupSoup` plan from its bins.
pub fn fold_bins(catalog: &Catalog, plan: &Plan, bins: Vec<Bin>) -> Table {
    let Shape::Aggregate { group_by, items } = &plan.shape else {
        unreachable!("only aggregate plans count bins");
    };
    let mut groups: Vec<aggregate::Group> = bins
        .into_iter()
        .map(|bin| aggregate::Group {
            key: bin.key.clone(),
            cells: items
                .iter()
                .map(|item| match item {
                    SelectItem::Column(_) => bin.key.clone(),
                    SelectItem::Agg {
                        func: AggFn::Count,
                        column: None,
                    } => Some(Cell::Number(bin.count as f64)),
                    SelectItem::Agg { .. } => unreachable!("bins only answer COUNT(*)"),
                })
                .collect(),
        })
        .collect();
    sort::groups(catalog, &mut groups, &plan.order_by, *group_by, items);
    groups.into_iter().map(|group| group.cells).collect()
}

/// Where an `ORDER BY` key lives in a group's output.
fn group_order_index(
    key: &OrderKey,
    group_by: Option<Uuid>,
    items: &[SelectItem],
) -> Option<usize> {
    match key {
        OrderKey::Item(index) => Some(*index),
        OrderKey::Column(column) => {
            debug_assert_eq!(
                Some(*column),
                group_by,
                "resolve allows only the group column"
            );
            items
                .iter()
                .position(|item| *item == SelectItem::Column(*column))
        }
    }
}
