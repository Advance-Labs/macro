//! Grouping rows and computing the select list's aggregates per group.

use uuid::Uuid;

use crate::resolve::{AggFn, SelectItem};

use super::{Cell, Row};

/// One output row of an aggregate shape.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// The grouped value; `None` without `GROUP BY` or for empty cells.
    pub key: Option<Cell>,
    /// The select list evaluated for the group.
    pub cells: Vec<Option<Cell>>,
}

/// Group the rows (one group in all without `GROUP BY`) and evaluate every
/// item. Groups come out in first-seen order; sorting is the caller's.
pub fn groups(rows: Vec<Row>, group_by: Option<Uuid>, items: &[SelectItem]) -> Vec<Group> {
    let mut keys: Vec<Option<Cell>> = Vec::new();
    let mut members: Vec<Vec<Row>> = Vec::new();
    match group_by {
        None => {
            keys.push(None);
            members.push(rows);
        }
        Some(column) => {
            for row in rows {
                let key = row
                    .cells
                    .get(&column)
                    .cloned()
                    .filter(|cell| !is_empty(cell));
                match keys.iter().position(|seen| *seen == key) {
                    Some(index) => members[index].push(row),
                    None => {
                        keys.push(key);
                        members.push(vec![row]);
                    }
                }
            }
        }
    }

    keys.into_iter()
        .zip(members)
        .map(|(key, rows)| Group {
            cells: items
                .iter()
                .map(|item| match item {
                    SelectItem::Column(_) => key.clone(),
                    SelectItem::Agg { func, column } => evaluate(*func, *column, &rows),
                })
                .collect(),
            key,
        })
        .collect()
}

/// A multi-valued cell with nothing in it groups with the empty cells.
fn is_empty(cell: &Cell) -> bool {
    matches!(cell, Cell::Options(ids) if ids.is_empty())
        || matches!(cell, Cell::Entities(ids) if ids.is_empty())
}

/// SQL aggregate semantics: `COUNT(*)` counts rows, everything else skips
/// empty cells, and a numeric aggregate over nothing is `NULL`.
fn evaluate(func: AggFn, column: Option<Uuid>, rows: &[Row]) -> Option<Cell> {
    let Some(column) = column else {
        return Some(Cell::Number(rows.len() as f64));
    };
    let present = rows
        .iter()
        .filter_map(|row| row.cells.get(&column))
        .filter(|cell| !is_empty(cell));
    match func {
        AggFn::Count => Some(Cell::Number(present.count() as f64)),
        AggFn::Sum | AggFn::Avg => {
            let numbers: Vec<f64> = present
                .filter_map(|cell| match cell {
                    Cell::Number(n) => Some(*n),
                    _ => None,
                })
                .collect();
            if numbers.is_empty() {
                return None;
            }
            let sum: f64 = numbers.iter().sum();
            Some(Cell::Number(match func {
                AggFn::Sum => sum,
                _ => sum / numbers.len() as f64,
            }))
        }
        AggFn::Min | AggFn::Max => {
            let mut best: Option<Cell> = None;
            for cell in present {
                let replace = match (&best, cell) {
                    (None, _) => true,
                    (Some(Cell::Number(b)), Cell::Number(n)) => {
                        if func == AggFn::Min {
                            n < b
                        } else {
                            n > b
                        }
                    }
                    (Some(Cell::Date(b)), Cell::Date(d)) => {
                        if func == AggFn::Min {
                            d < b
                        } else {
                            d > b
                        }
                    }
                    _ => false,
                };
                if replace {
                    best = Some(cell.clone());
                }
            }
            best
        }
    }
}
