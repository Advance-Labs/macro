//! Guarding an undo: which of a change's inverse ops still apply, given what
//! the table holds now and the changes made since.
//!
//! An undo reverts one change and leaves later ones alone. A cell reverts
//! only while it still holds what the change wrote; one changed since is
//! skipped, and the rest still revert. Anything else the change did reverts
//! whole or not at all: a row it added, only if nobody else wrote that row
//! since; a column it added, only if nobody else wrote cells in it; a
//! rename, option edit, reorder, view change or card move, only while it
//! stands as the change left it; a type change, only if nobody wrote the
//! column since. A removal it made always comes back.

use std::collections::{BTreeMap, BTreeSet};

use models_databases::views::ViewId;
use models_databases::{
    CellValue, ColumnChange, ColumnId, DatabaseOp, OptionId, RowChange, RowChanges, RowId,
    RowsChange, TableChange, TableId, ViewChange,
};
use serde::Serialize;

use super::{ChangeRecord, ColumnChangeKind, Restoration, RowChangeKind, SchemaImage};

/// What the table holds now, as an undo reads it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Current {
    /// The database's schema now.
    pub schema: SchemaImage,
    /// The rows the inverse names that still exist, with their cells.
    pub rows: BTreeMap<RowId, BTreeMap<ColumnId, CellValue>>,
}

/// A cell an undo left alone, because someone changed it after the change
/// being undone.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkippedCell {
    /// The row.
    #[schema(value_type = Uuid)]
    pub row: RowId,
    /// The column.
    #[schema(value_type = Uuid)]
    pub column: ColumnId,
    /// Who changed it since, from the journal; `null` when unknown.
    #[schema(required = true)]
    pub by: Option<String>,
}

/// Why an undo was refused. Nothing was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UndoRefusal {
    /// The change is someone else's: each person undoes their own.
    NotYours,
    /// Nothing undoes the change, such as a table's removal.
    NotUndoable,
    /// Someone else wrote a row the change added.
    RowEditedSince,
    /// Someone else wrote cells in a column the change added.
    ColumnWrittenSince,
    /// What the change set was changed since: a name, an option, an order,
    /// a view, a card's place, or a retyped column's cells.
    ChangedSince,
    /// What the undo would bring back is back already.
    AlreadyBack,
}

/// What guarding an undo decided.
#[derive(Debug, Clone, PartialEq)]
pub enum Guarded {
    /// Apply these ops, with what they put back; `skipped` lists the cells
    /// left alone.
    Apply {
        /// The ops, in order.
        ops: Vec<DatabaseOp>,
        /// The rows they put back and the definitions they bind back.
        restoration: Restoration,
        /// The cells left alone.
        skipped: Vec<SkippedCell>,
    },
    /// Undo nothing.
    Refused {
        /// Why.
        reason: UndoRefusal,
        /// Whose change stands in the way, when one does.
        by: Option<String>,
    },
}

/// Guard undoing `change` as `viewer`, given the table's changes after it,
/// oldest first, and what it holds now.
pub fn guard(
    viewer: &str,
    change: &ChangeRecord,
    later: &[ChangeRecord],
    current: &Current,
) -> Guarded {
    let refused = |reason, by: Option<&ChangeRecord>| Guarded::Refused {
        reason,
        by: by.and_then(|record| record.change.actor.clone()),
    };
    let actor = change.change.actor.as_deref();
    if actor != Some(viewer) {
        return Guarded::Refused {
            reason: UndoRefusal::NotYours,
            by: change.change.actor.clone(),
        };
    }
    let inverse = &change.change.inverse;
    let removes_table = change.change.ops.iter().any(|op| {
        matches!(
            op,
            DatabaseOp::Table {
                change: TableChange::Delete,
                ..
            }
        )
    });
    if removes_table || inverse.ops.is_empty() {
        return refused(UndoRefusal::NotUndoable, None);
    }
    let by_others = |record: &&ChangeRecord| record.change.actor.as_deref() != actor;

    let mut ops = Vec::new();
    let mut restoration = Restoration::default();
    let mut skipped = Vec::new();
    let mut readded: BTreeSet<OptionId> = BTreeSet::new();
    let mut recreated: BTreeSet<ColumnId> = BTreeSet::new();
    let mut removed: BTreeSet<ColumnId> = BTreeSet::new();
    let mut recreated_views: BTreeSet<ViewId> = BTreeSet::new();
    let mut removed_views: BTreeSet<ViewId> = BTreeSet::new();
    for (index, op) in inverse.ops.iter().enumerate() {
        match op {
            DatabaseOp::Rows {
                table,
                change: RowsChange::Update { changes },
            } => {
                let RowChanges::PerRow { rows } = changes else {
                    continue;
                };
                let mut kept = Vec::new();
                for row in rows {
                    let mut cells = Vec::new();
                    for cell in &row.cells {
                        let written = inverse
                            .after
                            .row(row.row)
                            .and_then(|cells| cells.get(&cell.column));
                        let now = current
                            .rows
                            .get(&row.row)
                            .map(|cells| cells.get(&cell.column));
                        match now {
                            Some(now) if now == written => cells.push(cell.clone()),
                            _ => skipped.push(SkippedCell {
                                row: row.row,
                                column: cell.column,
                                by: last_writer(later, row.row, cell.column)
                                    .and_then(|record| record.change.actor.clone()),
                            }),
                        }
                    }
                    if !cells.is_empty() {
                        kept.push(RowChange {
                            row: row.row,
                            cells,
                        });
                    }
                }
                if !kept.is_empty() {
                    ops.push(DatabaseOp::Rows {
                        table: *table,
                        change: RowsChange::Update {
                            changes: RowChanges::PerRow { rows: kept },
                        },
                    });
                }
            }
            DatabaseOp::Rows {
                table,
                change: RowsChange::Delete { rows },
            } => {
                if let Some(writer) = later
                    .iter()
                    .filter(by_others)
                    .rev()
                    .find(|record| record.rows.iter().any(|touch| rows.contains(&touch.row)))
                {
                    return refused(UndoRefusal::RowEditedSince, Some(writer));
                }
                let rows: Vec<RowId> = rows
                    .iter()
                    .copied()
                    .filter(|row| current.rows.contains_key(row))
                    .collect();
                if !rows.is_empty() {
                    ops.push(DatabaseOp::Rows {
                        table: *table,
                        change: RowsChange::Delete { rows },
                    });
                }
            }
            DatabaseOp::Rows {
                change: RowsChange::Insert { .. },
                ..
            } => {
                let restored = inverse
                    .restored_rows
                    .get(&index)
                    .cloned()
                    .unwrap_or_default();
                if restored
                    .iter()
                    .any(|row| current.rows.contains_key(&row.id))
                {
                    return refused(UndoRefusal::AlreadyBack, None);
                }
                if !restored.is_empty() {
                    restoration.rows.insert(ops.len(), restored);
                }
                ops.push(op.clone());
            }
            DatabaseOp::Column {
                column,
                change: column_change,
                ..
            } => {
                let present = current.schema.column(*column);
                match column_change {
                    ColumnChange::Delete => {
                        if let Some(writer) = later.iter().filter(by_others).rev().find(|record| {
                            record
                                .rows
                                .iter()
                                .any(|touch| touch.columns.contains(column))
                        }) {
                            return refused(UndoRefusal::ColumnWrittenSince, Some(writer));
                        }
                        if present.is_none() {
                            continue;
                        }
                        removed.insert(*column);
                    }
                    ColumnChange::Create { .. } => {
                        if present.is_some() {
                            return refused(UndoRefusal::AlreadyBack, None);
                        }
                        recreated.insert(*column);
                    }
                    ColumnChange::Rename { previous_name, .. } => {
                        if let Some(set) = previous_name
                            && present.is_none_or(|present| present.name != *set)
                        {
                            return refused(
                                UndoRefusal::ChangedSince,
                                last_column_change(later, *column),
                            );
                        }
                    }
                    ColumnChange::ChangeType { .. } => {
                        if let Some(writer) = later.iter().rev().find(|record| {
                            record
                                .rows
                                .iter()
                                .any(|touch| touch.columns.contains(column))
                                || record.columns.iter().any(|touch| touch.column == *column)
                        }) {
                            return refused(UndoRefusal::ChangedSince, Some(writer));
                        }
                        if let Some(definition) = inverse.rebinds.get(&index) {
                            restoration.rebinds.insert(ops.len(), *definition);
                        }
                    }
                    ColumnChange::AddOptions { options } => {
                        let options: Vec<_> = options
                            .iter()
                            .filter(|option| {
                                present.is_none_or(|present| {
                                    !present.options.iter().any(|held| held.id == option.id)
                                })
                            })
                            .cloned()
                            .collect();
                        readded.extend(options.iter().map(|option| option.id));
                        if options.is_empty() {
                            continue;
                        }
                        ops.push(DatabaseOp::Column {
                            table: change.change.table,
                            column: *column,
                            change: ColumnChange::AddOptions { options },
                        });
                        continue;
                    }
                    ColumnChange::DeleteOption { option } => {
                        let Some(held) = present.and_then(|present| {
                            present.options.iter().find(|held| held.id == *option)
                        }) else {
                            continue;
                        };
                        let added = added_label(&change.change.ops, *column, *option);
                        if added.is_some_and(|label| label.trim() != held.label) {
                            return refused(
                                UndoRefusal::ChangedSince,
                                last_column_change(later, *column),
                            );
                        }
                    }
                    ColumnChange::UpdateOption { option, .. } => {
                        if !readded.contains(option) {
                            let held = present.and_then(|present| {
                                present.options.iter().find(|held| held.id == *option)
                            });
                            let set = updated_option(&change.change.ops, *column, *option);
                            let stands = match (held, set) {
                                (Some(held), Some((label, color))) => {
                                    label.is_none_or(|label| label.trim() == held.label)
                                        && color.is_none_or(|color| *color == held.color)
                                }
                                _ => false,
                            };
                            if !stands {
                                return refused(
                                    UndoRefusal::ChangedSince,
                                    last_column_change(later, *column),
                                );
                            }
                        }
                    }
                }
                ops.push(op.clone());
            }
            DatabaseOp::Table {
                table,
                change: table_change,
            } => match table_change {
                TableChange::Delete => {
                    if let Some(writer) = later.iter().find(by_others) {
                        return refused(UndoRefusal::RowEditedSince, Some(writer));
                    }
                    ops.push(op.clone());
                }
                TableChange::Rename { previous_name, .. } => {
                    let name = current
                        .schema
                        .table(*table)
                        .map(|table| table.name.as_str());
                    if previous_name
                        .as_deref()
                        .is_some_and(|set| Some(set) != name)
                    {
                        return refused(UndoRefusal::ChangedSince, later.last());
                    }
                    ops.push(op.clone());
                }
                TableChange::ReorderColumns { order } => {
                    let set = change.change.ops.iter().find_map(|op| match op {
                        DatabaseOp::Table {
                            table: named,
                            change: TableChange::ReorderColumns { order },
                        } if named == table => Some(order),
                        _ => None,
                    });
                    let now: Vec<ColumnId> = current
                        .schema
                        .columns_of(*table)
                        .map(|column| column.id)
                        .collect();
                    if let Some(set) = set {
                        if *set != now {
                            return refused(
                                UndoRefusal::ChangedSince,
                                later.iter().rev().find(|record| {
                                    record
                                        .columns
                                        .iter()
                                        .any(|touch| touch.kind == ColumnChangeKind::Reorder)
                                }),
                            );
                        }
                        ops.push(op.clone());
                        continue;
                    }
                    // Putting back the place of a column the undo brings
                    // back: only while the table has exactly those columns.
                    let after: BTreeSet<ColumnId> = now
                        .iter()
                        .copied()
                        .chain(recreated.iter().copied())
                        .filter(|column| !removed.contains(column))
                        .collect();
                    if after == order.iter().copied().collect() {
                        ops.push(op.clone());
                    }
                }
                TableChange::ReorderViews { order } => {
                    let set = change.change.ops.iter().find_map(|op| match op {
                        DatabaseOp::Table {
                            table: named,
                            change: TableChange::ReorderViews { order },
                        } if named == table => Some(order),
                        _ => None,
                    });
                    let now: Vec<ViewId> = current
                        .schema
                        .views
                        .iter()
                        .filter(|view| view.table_id == *table)
                        .map(|view| view.id)
                        .collect();
                    if let Some(set) = set {
                        if *set != now {
                            return refused(UndoRefusal::ChangedSince, later.last());
                        }
                        ops.push(op.clone());
                        continue;
                    }
                    let after: BTreeSet<ViewId> = now
                        .iter()
                        .copied()
                        .chain(recreated_views.iter().copied())
                        .filter(|view| !removed_views.contains(view))
                        .collect();
                    if after == order.iter().copied().collect() {
                        ops.push(op.clone());
                    }
                }
                TableChange::Create { .. } => ops.push(op.clone()),
            },
            DatabaseOp::ReorderTables { .. } => {
                let set = change.change.ops.iter().find_map(|op| match op {
                    DatabaseOp::ReorderTables { order } => Some(order),
                    _ => None,
                });
                let now: Vec<TableId> =
                    current.schema.tables.iter().map(|table| table.id).collect();
                if set.is_some_and(|set| *set != now) {
                    return refused(UndoRefusal::ChangedSince, None);
                }
                ops.push(op.clone());
            }
            DatabaseOp::View {
                view,
                change: view_change,
                ..
            } => {
                if let ViewChange::MoveCard { row, .. } = view_change {
                    let grouping = current
                        .schema
                        .view(*view)
                        .and_then(|board| board.layout.group_by());
                    if let Some(mover) = later.iter().rev().find(|record| {
                        moves_card(record, *row)
                            || record.rows.iter().any(|touch| {
                                touch.row == *row
                                    && (touch.kind == RowChangeKind::Delete
                                        || grouping
                                            .is_some_and(|column| touch.columns.contains(&column)))
                            })
                    }) {
                        return refused(UndoRefusal::ChangedSince, Some(mover));
                    }
                    ops.push(op.clone());
                    continue;
                }
                if let Some(changer) = later.iter().rev().find(|record| names_view(record, *view)) {
                    return refused(UndoRefusal::ChangedSince, Some(changer));
                }
                let present = current.schema.view(*view).is_some();
                match view_change {
                    ViewChange::Create { .. } if present => {
                        return refused(UndoRefusal::AlreadyBack, None);
                    }
                    ViewChange::Create { .. } => {
                        recreated_views.insert(*view);
                    }
                    ViewChange::Delete if !present => continue,
                    ViewChange::Delete => {
                        removed_views.insert(*view);
                    }
                    _ => {}
                }
                ops.push(op.clone());
            }
        }
    }
    Guarded::Apply {
        ops,
        restoration,
        skipped,
    }
}

/// The latest change since that wrote a row's cell, or removed the row.
fn last_writer(later: &[ChangeRecord], row: RowId, column: ColumnId) -> Option<&ChangeRecord> {
    later.iter().rev().find(|record| {
        record.rows.iter().any(|touch| {
            touch.row == row
                && (touch.kind == RowChangeKind::Delete || touch.columns.contains(&column))
        })
    })
}

/// The latest change since that changed a column itself.
fn last_column_change(later: &[ChangeRecord], column: ColumnId) -> Option<&ChangeRecord> {
    later
        .iter()
        .rev()
        .find(|record| record.columns.iter().any(|touch| touch.column == column))
}

/// The label an option was added under by these ops.
fn added_label(ops: &[DatabaseOp], column: ColumnId, option: OptionId) -> Option<&str> {
    ops.iter().find_map(|op| match op {
        DatabaseOp::Column {
            column: named,
            change: ColumnChange::AddOptions { options },
            ..
        } if *named == column => options
            .iter()
            .find(|added| added.id == option)
            .map(|added| added.label.as_str()),
        _ => None,
    })
}

/// What these ops set an option's label and colour to.
fn updated_option(
    ops: &[DatabaseOp],
    column: ColumnId,
    option: OptionId,
) -> Option<(Option<&String>, Option<&Option<String>>)> {
    ops.iter().rev().find_map(|op| match op {
        DatabaseOp::Column {
            column: named,
            change:
                ColumnChange::UpdateOption {
                    option: updated,
                    label,
                    color,
                },
            ..
        } if *named == column && *updated == option => Some((label.as_ref(), color.as_ref())),
        _ => None,
    })
}

fn moves_card(record: &ChangeRecord, row: RowId) -> bool {
    record.change.ops.iter().any(|op| {
        matches!(
            op,
            DatabaseOp::View { change: ViewChange::MoveCard { row: moved, .. }, .. } if *moved == row
        )
    })
}

fn names_view(record: &ChangeRecord, view: ViewId) -> bool {
    record.change.ops.iter().any(|op| {
        matches!(
            op,
            DatabaseOp::View { view: named, change, .. }
                if *named == view && !matches!(change, ViewChange::MoveCard { .. })
        )
    })
}
