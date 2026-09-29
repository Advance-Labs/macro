//! Ordering rows and groups. Empty cells sort last in either direction;
//! select values sort in the column's option order; entities by id.

use std::cmp::Ordering;

use uuid::Uuid;

use crate::catalog::{Catalog, ColumnKind};
use crate::resolve::{Dir, Order, OrderKey, SelectItem};

use super::aggregate::Group;
use super::{Cell, Row, group_order_index};

/// Sort fetched rows by column keys. A key on an item index cannot occur
/// here: a row shape has no aggregates to refer to.
pub fn rows(catalog: &Catalog, rows: &mut [Row], order_by: &[Order]) {
    rows.sort_by(|a, b| {
        order_by
            .iter()
            .map(|order| {
                let OrderKey::Column(column) = order.key else {
                    unreachable!("row shapes order by columns only");
                };
                directed(
                    compare(catalog, a.cells.get(&column), b.cells.get(&column)),
                    order.dir,
                )
            })
            .find(|ordering| *ordering != Ordering::Equal)
            .unwrap_or(Ordering::Equal)
    });
}

/// Sort groups by select-list position.
pub fn groups(
    catalog: &Catalog,
    groups: &mut [Group],
    order_by: &[Order],
    group_by: Option<Uuid>,
    items: &[SelectItem],
) {
    groups.sort_by(|a, b| {
        order_by
            .iter()
            .map(|order| {
                let ordering = match group_order_index(&order.key, group_by, items) {
                    Some(index) => {
                        compare(catalog, a.cells[index].as_ref(), b.cells[index].as_ref())
                    }
                    // ORDER BY the group column when it is not selected.
                    None => compare(catalog, a.key.as_ref(), b.key.as_ref()),
                };
                directed(ordering, order.dir)
            })
            .find(|ordering| *ordering != Ordering::Equal)
            .unwrap_or(Ordering::Equal)
    });
}

/// Reverse for `DESC`, but keep empty cells last.
fn directed(ordering: Ranked, dir: Dir) -> Ordering {
    match (ordering, dir) {
        (Ranked::Both(ordering), Dir::Asc) => ordering,
        (Ranked::Both(ordering), Dir::Desc) => ordering.reverse(),
        (Ranked::EmptyLeft, _) => Ordering::Greater,
        (Ranked::EmptyRight, _) => Ordering::Less,
        (Ranked::BothEmpty, _) => Ordering::Equal,
    }
}

/// A comparison that remembers which side was empty.
enum Ranked {
    Both(Ordering),
    EmptyLeft,
    EmptyRight,
    BothEmpty,
}

fn compare(catalog: &Catalog, a: Option<&Cell>, b: Option<&Cell>) -> Ranked {
    match (
        a.filter(|cell| !is_empty(cell)),
        b.filter(|cell| !is_empty(cell)),
    ) {
        (None, None) => Ranked::BothEmpty,
        (None, Some(_)) => Ranked::EmptyLeft,
        (Some(_), None) => Ranked::EmptyRight,
        (Some(a), Some(b)) => Ranked::Both(match (a, b) {
            (Cell::Text(a), Cell::Text(b)) => a.to_lowercase().cmp(&b.to_lowercase()),
            (Cell::Number(a), Cell::Number(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (Cell::Bool(a), Cell::Bool(b)) => a.cmp(b),
            (Cell::Date(a), Cell::Date(b)) => a.cmp(b),
            (Cell::Options(a), Cell::Options(b)) => {
                option_rank(catalog, a).cmp(&option_rank(catalog, b))
            }
            (Cell::Entities(a), Cell::Entities(b)) => a.cmp(b),
            _ => Ordering::Equal,
        }),
    }
}

fn is_empty(cell: &Cell) -> bool {
    matches!(cell, Cell::Options(ids) if ids.is_empty())
        || matches!(cell, Cell::Entities(ids) if ids.is_empty())
}

/// Options in the order the column declares them; the rank of a cell is
/// the rank of each option it holds, in order.
fn option_rank(catalog: &Catalog, options: &[Uuid]) -> Vec<usize> {
    options
        .iter()
        .map(|option| {
            catalog
                .tables
                .iter()
                .flat_map(|table| &table.columns)
                .find_map(|column| match &column.kind {
                    ColumnKind::Select { options, .. } => {
                        options.iter().position(|candidate| candidate.id == *option)
                    }
                    _ => None,
                })
                .unwrap_or(usize::MAX)
        })
        .collect()
}
