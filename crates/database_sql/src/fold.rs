//! Stage five: finish a [`Plan`] over the rows (or bins) the server returned.
//!
//! Pure: rows in, result rows out. Joins the relations, applies the residual
//! filter, groups and aggregates or projects, drops repeats for `DISTINCT`,
//! then sorts. SQL semantics where SQL has an opinion (`NULL` compares
//! false, `COUNT(column)` skips empty cells, `SUM` of nothing is `NULL`, a
//! join on a `NULL` matches nothing); Macro's where SQL does not (`LIKE`
//! ignores case, empty cells sort last, select values sort in option order,
//! a multi-valued join column matches by membership).

mod aggregate;
mod join;
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

/// One fetched row: the entity id and the cells the plan asked for. After
/// a join, the cells of every matched relation under their keys, with the
/// `FROM` row's id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    /// The row entity id.
    pub id: Uuid,
    /// Cells by column; a missing column is an empty cell.
    pub cells: HashMap<Uuid, Cell>,
}

/// One `groupSoup` bin: the grouped value and how many rows it holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bin {
    /// The group's value; `None` for rows with an empty cell.
    pub key: Option<Cell>,
    /// Rows in the group.
    pub count: u64,
}

/// The result: one `Vec` per row, one `Option<Cell>` per select item, in
/// select-list order.
pub type Table = Vec<Vec<Option<Cell>>>;

/// Finish a plan from the rows fetched for each relation, `FROM` first.
/// For a row shape, the second value is the `FROM` row behind each result
/// row, in result order.
pub fn fold_relations(
    catalog: &Catalog,
    plan: &Plan,
    fetched: Vec<Vec<Row>>,
) -> (Table, Vec<Uuid>) {
    fold_joined(catalog, plan, join::join(plan, fetched))
}

/// Finish a plan over rows that are already joined (or come from one
/// relation).
pub fn fold_rows(catalog: &Catalog, plan: &Plan, rows: Vec<Row>) -> Table {
    fold_joined(catalog, plan, rows).0
}

fn fold_joined(catalog: &Catalog, plan: &Plan, rows: Vec<Row>) -> (Table, Vec<Uuid>) {
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
            let projected = rows.into_iter().map(|mut row| {
                let cells: Vec<Option<Cell>> = columns
                    .iter()
                    .map(|column| row.cells.remove(column))
                    .collect();
                (row.id, cells)
            });
            let (ids, table): (Vec<Uuid>, Table) = if plan.distinct {
                window(plan, distinct(projected)).unzip()
            } else {
                window(plan, projected).unzip()
            };
            (table, ids)
        }
        Shape::Aggregate { group_by, items } => {
            let mut groups = aggregate::groups(rows, *group_by, items);
            sort::groups(catalog, &mut groups, &plan.order_by, *group_by, items);
            (
                window(plan, groups.into_iter().map(|group| group.cells)).collect(),
                Vec::new(),
            )
        }
    }
}

/// `OFFSET` then `LIMIT`, after ordering.
fn window<T>(plan: &Plan, rows: impl Iterator<Item = T>) -> impl Iterator<Item = T> {
    rows.skip(plan.offset.unwrap_or(0) as usize)
        .take(plan.limit.map_or(usize::MAX, |limit| limit as usize))
}

/// Keep the first of every set of equal result rows, in order. Cells are
/// compared by their printed form, which is total where `f64` is not.
fn distinct(
    rows: impl Iterator<Item = (Uuid, Vec<Option<Cell>>)>,
) -> impl Iterator<Item = (Uuid, Vec<Option<Cell>>)> {
    let mut seen = std::collections::HashSet::new();
    rows.filter(move |(_, cells)| seen.insert(format!("{cells:?}")))
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
