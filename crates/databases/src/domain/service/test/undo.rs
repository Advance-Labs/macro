//! Undoing one's own change over the fakes: guarded against later edits by
//! others, and redone by undoing the undo.

use models_databases::RowChange;
use models_databases::views::{
    Conjunction, FilterCondition, FilterGroup, FilterNode, FilterTest, NewView, NumberOperator,
    RequestedLayout, ViewId, ViewQuery,
};

use super::*;
use crate::domain::journal::{SkippedCell, UndoOutcome, UndoRefusal};
use crate::domain::models::{ChangeId, CommittedChange};

const JULIA: &str = "macro|julia@macro.com";

fn edit_as(database_id: DatabaseId, user: &'static str) -> EntityAccessReceipt<EditAccessLevel> {
    receipt::<EditAccessLevel>(database_id, user, AccessLevel::Edit)
}

/// Apply ops as `user`, answering the one change they made.
async fn change_as(seeded: &Seeded, user: &'static str, ops: Vec<DatabaseOp>) -> ChangeId {
    let applied = seeded
        .service
        .apply_ops_with_changes(edit_as(seeded.database_id, user), viewer(user), ops.into())
        .await
        .unwrap();
    let [change] = applied.changes.as_slice() else {
        panic!("one table changed: {:?}", applied.changes);
    };
    change.change
}

async fn undo_as(seeded: &Seeded, user: &'static str, change: ChangeId) -> UndoOutcome {
    seeded
        .service
        .undo_change(edit_as(seeded.database_id, user), viewer(user), change)
        .await
        .unwrap()
}

fn set_cells(seeded: &Seeded, row: RowId, cells: Vec<CellWrite>) -> Vec<DatabaseOp> {
    vec![DatabaseOp::Rows {
        table: seeded.table_id,
        change: RowsChange::Update {
            changes: RowChanges::PerRow {
                rows: vec![RowChange { row, cells }],
            },
        },
    }]
}

fn latest_change(seeded: &Seeded) -> CommittedChange {
    let world = seeded.world.lock().unwrap();
    let last = world.journal.last().unwrap();
    CommittedChange {
        table: last.entry.table,
        version: last.entry.version,
        change: last.id,
    }
}

#[tokio::test]
async fn undo_reverts_the_cells_nobody_changed_since_and_keeps_julias() {
    let seeded = seeded().await;
    let sam = seeded.row_id;
    let wolf_edit = change_as(
        &seeded,
        OWNER,
        set_cells(
            &seeded,
            sam,
            vec![
                CellWrite {
                    column: seeded.name_column.id,
                    value: CellValue::Text("Samuel".into()),
                },
                CellWrite {
                    column: seeded.plus_ones_column.id,
                    value: CellValue::Number(3.0),
                },
            ],
        ),
    )
    .await;
    change_as(
        &seeded,
        JULIA,
        set_cells(
            &seeded,
            sam,
            vec![CellWrite {
                column: seeded.plus_ones_column.id,
                value: CellValue::Number(5.0),
            }],
        ),
    )
    .await;

    let outcome = undo_as(&seeded, OWNER, wolf_edit).await;

    assert_eq!(
        outcome,
        UndoOutcome::Partial {
            skipped: vec![SkippedCell {
                row: sam,
                column: seeded.plus_ones_column.id,
                by: Some(JULIA.into()),
            }],
            changes: vec![latest_change(&seeded)],
        }
    );
    assert_eq!(
        cell(
            &seeded.world,
            sam,
            seeded.name_column.property_definition_id
        ),
        Some(PropertyValue::Str("Sam".into()))
    );
    assert_eq!(
        cell(
            &seeded.world,
            sam,
            seeded.plus_ones_column.property_definition_id
        ),
        Some(PropertyValue::Num(5.0))
    );
}

#[tokio::test]
async fn undoing_an_insert_is_refused_once_julia_edited_the_row() {
    let seeded = seeded().await;
    let insert = change_as(
        &seeded,
        OWNER,
        vec![DatabaseOp::Rows {
            table: seeded.table_id,
            change: RowsChange::Insert {
                rows: vec![vec![CellWrite {
                    column: seeded.name_column.id,
                    value: CellValue::Text("Alex".into()),
                }]],
            },
        }],
    )
    .await;
    let alex = *row_ids(&seeded.world, seeded.table_id).last().unwrap();
    change_as(
        &seeded,
        JULIA,
        set_cells(
            &seeded,
            alex,
            vec![CellWrite {
                column: seeded.plus_ones_column.id,
                value: CellValue::Number(1.0),
            }],
        ),
    )
    .await;

    let outcome = undo_as(&seeded, OWNER, insert).await;

    assert_eq!(
        outcome,
        UndoOutcome::Refused {
            reason: UndoRefusal::RowEditedSince,
            by: Some(JULIA.into()),
        }
    );
    assert_eq!(
        row_ids(&seeded.world, seeded.table_id),
        vec![seeded.row_id, alex]
    );
    assert_eq!(
        cell(
            &seeded.world,
            alex,
            seeded.name_column.property_definition_id
        ),
        Some(PropertyValue::Str("Alex".into()))
    );
}

#[tokio::test]
async fn undoing_a_row_delete_brings_the_row_back_exactly_after_julia_edits_others() {
    let seeded = seeded().await;
    let robin = insert_names(&seeded, &["Robin"]).await[0];
    let sam = seeded.row_id;
    let before = seeded.world.lock().unwrap().rows[&seeded.table_id].clone();
    let sam_cells = seeded.world.lock().unwrap().cells[&sam].clone();
    let delete = change_as(
        &seeded,
        OWNER,
        vec![DatabaseOp::Rows {
            table: seeded.table_id,
            change: RowsChange::Delete { rows: vec![sam] },
        }],
    )
    .await;
    change_as(
        &seeded,
        JULIA,
        set_cells(
            &seeded,
            robin,
            vec![CellWrite {
                column: seeded.plus_ones_column.id,
                value: CellValue::Number(4.0),
            }],
        ),
    )
    .await;

    let outcome = undo_as(&seeded, OWNER, delete).await;

    assert_eq!(
        outcome,
        UndoOutcome::Reverted {
            changes: vec![latest_change(&seeded)],
        }
    );
    assert_eq!(seeded.world.lock().unwrap().rows[&seeded.table_id], before);
    assert_eq!(seeded.world.lock().unwrap().cells[&sam], sam_cells);
    assert_eq!(
        cell(
            &seeded.world,
            robin,
            seeded.plus_ones_column.property_definition_id
        ),
        Some(PropertyValue::Num(4.0))
    );
}

#[tokio::test]
async fn undoing_a_column_delete_restores_the_column_its_cells_and_the_view_filter() {
    let seeded = seeded().await;
    let sam = seeded.row_id;
    let crowd = ViewId::new();
    let crowd_query = ViewQuery {
        filter: Some(FilterGroup {
            conjunction: Conjunction::And,
            conditions: vec![FilterNode::Condition(FilterCondition {
                column: seeded.plus_ones_column.id,
                test: FilterTest::Number {
                    operator: NumberOperator::GreaterThan,
                    value: 1.0,
                },
            })],
        }),
        sort: Vec::new(),
    };
    change_as(
        &seeded,
        OWNER,
        vec![DatabaseOp::View {
            table: seeded.table_id,
            view: crowd,
            change: ViewChange::Create {
                view: NewView {
                    name: "Crowds".into(),
                    query: crowd_query.clone(),
                    layout: RequestedLayout::Table {
                        columns: Vec::new(),
                    },
                },
            },
        }],
    )
    .await;
    let columns_before: Vec<ColumnId> = seeded
        .world
        .lock()
        .unwrap()
        .columns
        .iter()
        .filter(|column| column.table_id == seeded.table_id)
        .map(|column| column.id)
        .collect();
    let delete = change_as(
        &seeded,
        OWNER,
        vec![DatabaseOp::Column {
            table: seeded.table_id,
            column: seeded.plus_ones_column.id,
            change: ColumnChange::Delete,
        }],
    )
    .await;

    let outcome = undo_as(&seeded, OWNER, delete).await;

    assert_eq!(
        outcome,
        UndoOutcome::Reverted {
            changes: vec![latest_change(&seeded)],
        }
    );
    let world = seeded.world.lock().unwrap();
    let mut columns: Vec<&Column> = world
        .columns
        .iter()
        .filter(|column| column.table_id == seeded.table_id)
        .collect();
    columns.sort_by(|left, right| left.position.cmp(&right.position));
    assert_eq!(
        columns.iter().map(|column| column.id).collect::<Vec<_>>(),
        columns_before
    );
    let plus_ones = columns
        .iter()
        .find(|column| column.id == seeded.plus_ones_column.id)
        .unwrap();
    assert_eq!(
        plus_ones.property_definition_id,
        seeded.plus_ones_column.property_definition_id
    );
    assert_eq!(
        world.cells[&sam].get(&seeded.plus_ones_column.property_definition_id),
        Some(&PropertyValue::Num(2.0))
    );
    let view = world.views.iter().find(|view| view.id == crowd).unwrap();
    assert_eq!(view.query, crowd_query);
}

#[tokio::test]
async fn an_undo_is_redone_by_undoing_the_undo() {
    let seeded = seeded().await;
    let sam = seeded.row_id;
    let rename = change_as(
        &seeded,
        OWNER,
        set_cells(
            &seeded,
            sam,
            vec![CellWrite {
                column: seeded.name_column.id,
                value: CellValue::Text("Samantha".into()),
            }],
        ),
    )
    .await;

    let undone = undo_as(&seeded, OWNER, rename).await;
    let UndoOutcome::Reverted { changes } = &undone else {
        panic!("the undo reverts: {undone:?}");
    };
    let name_after_undo = cell(
        &seeded.world,
        sam,
        seeded.name_column.property_definition_id,
    );
    let redone = undo_as(&seeded, OWNER, changes[0].change).await;

    assert_eq!(name_after_undo, Some(PropertyValue::Str("Sam".into())));
    assert_eq!(
        redone,
        UndoOutcome::Reverted {
            changes: vec![latest_change(&seeded)],
        }
    );
    assert_eq!(
        cell(
            &seeded.world,
            sam,
            seeded.name_column.property_definition_id
        ),
        Some(PropertyValue::Str("Samantha".into()))
    );
}

#[tokio::test]
async fn nobody_undoes_someone_elses_change() {
    let seeded = seeded().await;
    let sam = seeded.row_id;
    let julias = change_as(
        &seeded,
        JULIA,
        set_cells(
            &seeded,
            sam,
            vec![CellWrite {
                column: seeded.name_column.id,
                value: CellValue::Text("Sammy".into()),
            }],
        ),
    )
    .await;

    let outcome = undo_as(&seeded, OWNER, julias).await;

    assert_eq!(
        outcome,
        UndoOutcome::Refused {
            reason: UndoRefusal::NotYours,
            by: Some(JULIA.into()),
        }
    );
    assert_eq!(
        cell(
            &seeded.world,
            sam,
            seeded.name_column.property_definition_id
        ),
        Some(PropertyValue::Str("Sammy".into()))
    );
}
