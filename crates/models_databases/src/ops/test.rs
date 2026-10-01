use chrono::TimeZone;
use serde_json::json;

use super::*;

const TABLE: Uuid = Uuid::from_u128(0x7ab1);
const NAME: Uuid = Uuid::from_u128(0xc01a);
const STATUS: Uuid = Uuid::from_u128(0xc01b);
const SAM: Uuid = Uuid::from_u128(0x5a11);
const ALEX: Uuid = Uuid::from_u128(0xa1e8);

#[test]
fn an_insert_reads_its_rows_cells_and_values_from_json() {
    let op: DatabaseOp = serde_json::from_value(json!({
        "kind": "insert_rows",
        "table": TABLE,
        "rows": [
            [
                {"column": NAME, "value": {"type": "text", "value": "Sam"}},
                {"column": STATUS, "value": {"type": "options", "value": [{"label": "Going"}]}},
            ],
            [
                {"column": NAME, "value": {"type": "clear"}},
            ],
        ],
        "createMissingOptions": true,
    }))
    .unwrap();

    assert_eq!(
        op,
        DatabaseOp::InsertRows {
            table: TABLE,
            rows: vec![
                vec![
                    CellWrite {
                        column: NAME,
                        value: CellValue::Text("Sam".into()),
                    },
                    CellWrite {
                        column: STATUS,
                        value: CellValue::Options(vec![OptionRef::Label("Going".into())]),
                    },
                ],
                vec![CellWrite {
                    column: NAME,
                    value: CellValue::Clear,
                }],
            ],
            create_missing_options: true,
        }
    );
}

#[test]
fn every_value_kind_round_trips_through_json() {
    let values = vec![
        CellValue::Text("Sam".into()),
        CellValue::Number(2.5),
        CellValue::Boolean(true),
        CellValue::Date(Utc.with_ymd_and_hms(2026, 9, 30, 0, 0, 0).unwrap()),
        CellValue::Link(vec!["https://macro.com".into()]),
        CellValue::Options(vec![
            OptionRef::Id(STATUS),
            OptionRef::Label("Going".into()),
        ]),
        CellValue::Entities(vec![EntityRef {
            entity_type: EntityKind::User,
            entity_id: "macro|sam@macro.com".into(),
        }]),
        CellValue::Rows(vec![SAM]),
        CellValue::Clear,
    ];

    let encoded = serde_json::to_value(&values).unwrap();

    assert_eq!(
        encoded,
        json!([
            {"type": "text", "value": "Sam"},
            {"type": "number", "value": 2.5},
            {"type": "boolean", "value": true},
            {"type": "date", "value": "2026-09-30T00:00:00Z"},
            {"type": "link", "value": ["https://macro.com"]},
            {"type": "options", "value": [{"id": STATUS}, {"label": "Going"}]},
            {"type": "entities", "value": [{"entityType": "USER", "entityId": "macro|sam@macro.com"}]},
            {"type": "rows", "value": [SAM]},
            {"type": "clear"},
        ])
    );
    let decoded: Vec<CellValue> = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn updates_name_their_rows_uniformly_or_one_by_one() {
    let uniform: DatabaseOp = serde_json::from_value(json!({
        "kind": "update_rows",
        "table": TABLE,
        "changes": {
            "kind": "uniform",
            "rows": [SAM, ALEX],
            "cells": [{"column": NAME, "value": {"type": "text", "value": "Guest"}}],
        },
    }))
    .unwrap();
    assert_eq!(
        uniform,
        DatabaseOp::UpdateRows {
            table: TABLE,
            changes: RowChanges::Uniform {
                rows: vec![SAM, ALEX],
                cells: vec![CellWrite {
                    column: NAME,
                    value: CellValue::Text("Guest".into()),
                }],
            },
            create_missing_options: false,
        }
    );

    let per_row: DatabaseOp = serde_json::from_value(json!({
        "kind": "update_rows",
        "table": TABLE,
        "changes": {
            "kind": "per_row",
            "rows": [{"row": SAM, "cells": [{"column": NAME, "value": {"type": "text", "value": "Sam"}}]}],
        },
        "createMissingOptions": false,
    }))
    .unwrap();
    assert_eq!(
        per_row,
        DatabaseOp::UpdateRows {
            table: TABLE,
            changes: RowChanges::PerRow {
                rows: vec![RowChange {
                    row: SAM,
                    cells: vec![CellWrite {
                        column: NAME,
                        value: CellValue::Text("Sam".into()),
                    }],
                }],
            },
            create_missing_options: false,
        }
    );
}

#[test]
fn deletes_and_type_changes_read_from_json() {
    let ops: Vec<DatabaseOp> = serde_json::from_value(json!([
        {"kind": "delete_rows", "table": TABLE, "rows": [SAM]},
        {
            "kind": "change_column_type",
            "table": TABLE,
            "column": STATUS,
            "to": {"type": "select", "multi": true},
            "clearInvalid": true,
        },
        {
            "kind": "change_column_type",
            "table": TABLE,
            "column": NAME,
            "to": {"type": "entity", "target": "USER", "multi": false},
        },
        {
            "kind": "change_column_type",
            "table": TABLE,
            "column": NAME,
            "to": {"type": "relation", "database": ALEX, "table": TABLE},
        },
    ]))
    .unwrap();

    assert_eq!(
        ops,
        vec![
            DatabaseOp::DeleteRows {
                table: TABLE,
                rows: vec![SAM],
            },
            DatabaseOp::ChangeColumnType {
                table: TABLE,
                column: STATUS,
                to: ColumnKind::Select { multi: true },
                clear_invalid: true,
            },
            DatabaseOp::ChangeColumnType {
                table: TABLE,
                column: NAME,
                to: ColumnKind::Entity {
                    target: EntityKind::User,
                    multi: false,
                },
                clear_invalid: false,
            },
            DatabaseOp::ChangeColumnType {
                table: TABLE,
                column: NAME,
                to: ColumnKind::Relation {
                    database: ALEX,
                    table: TABLE,
                },
                clear_invalid: false,
            },
        ]
    );
}

#[test]
fn results_say_what_each_op_did_in_camel_case() {
    let results = vec![
        OpResult::RowsWritten {
            table_version: TableVersion(4),
            inserted: vec![SAM],
            affected: 1,
        },
        OpResult::ColumnTyped {
            table_version: TableVersion(5),
            cleared_cells: 2,
            trimmed_cells: 1,
        },
    ];

    assert_eq!(
        serde_json::to_value(&results).unwrap(),
        json!([
            {"kind": "rows_written", "tableVersion": 4, "inserted": [SAM], "affected": 1},
            {"kind": "column_typed", "tableVersion": 5, "clearedCells": 2, "trimmedCells": 1},
        ])
    );
}

#[test]
fn an_option_update_tells_a_missing_colour_from_a_cleared_one() {
    let option = Uuid::from_u128(0x0b7);
    let read = |body| serde_json::from_value::<DatabaseOp>(body).unwrap();

    assert_eq!(
        read(json!({
            "kind": "update_option",
            "table": TABLE,
            "column": STATUS,
            "option": option,
            "label": "Maybe",
        })),
        DatabaseOp::UpdateOption {
            table: TABLE,
            column: STATUS,
            option,
            label: Some("Maybe".into()),
            color: None,
        }
    );
    assert_eq!(
        read(json!({
            "kind": "update_option",
            "table": TABLE,
            "column": STATUS,
            "option": option,
            "color": null,
        })),
        DatabaseOp::UpdateOption {
            table: TABLE,
            column: STATUS,
            option,
            label: None,
            color: Some(None),
        }
    );
    assert_eq!(
        read(json!({
            "kind": "update_option",
            "table": TABLE,
            "column": STATUS,
            "option": option,
            "color": "#12A594",
        })),
        DatabaseOp::UpdateOption {
            table: TABLE,
            column: STATUS,
            option,
            label: None,
            color: Some(Some("#12A594".into())),
        }
    );
    assert_eq!(
        serde_json::to_value(DatabaseOp::UpdateOption {
            table: TABLE,
            column: STATUS,
            option,
            label: None,
            color: Some(None),
        })
        .unwrap(),
        json!({
            "kind": "update_option",
            "table": TABLE,
            "column": STATUS,
            "option": option,
            "color": null,
        })
    );
}

#[test]
fn an_option_removal_names_its_table_column_and_option() {
    let option = Uuid::from_u128(0x0b7);
    let op: DatabaseOp = serde_json::from_value(json!({
        "kind": "delete_option",
        "table": TABLE,
        "column": STATUS,
        "option": option,
    }))
    .unwrap();
    assert_eq!(
        op,
        DatabaseOp::DeleteOption {
            table: TABLE,
            column: STATUS,
            option,
        }
    );
    assert_eq!(op.table(), TABLE);
}

#[test]
fn view_ops_read_their_table_view_and_card_from_json() {
    let view = Uuid::from_u128(0x71e);
    let lane = Uuid::from_u128(0x0b7);
    let read = |body| serde_json::from_value::<DatabaseOp>(body).unwrap();

    assert_eq!(
        read(json!({
            "kind": "update_view",
            "table": TABLE,
            "view": view,
            "name": "By stage",
        })),
        DatabaseOp::UpdateView {
            table: TABLE,
            view,
            name: Some("By stage".into()),
            query: None,
            layout: None,
        }
    );
    assert_eq!(
        read(json!({"kind": "reorder_views", "table": TABLE, "order": [view]})),
        DatabaseOp::ReorderViews {
            table: TABLE,
            order: vec![view],
        }
    );
    assert_eq!(
        read(json!({
            "kind": "move_card",
            "table": TABLE,
            "view": view,
            "row": SAM,
            "lane": lane,
            "before": ALEX,
        })),
        DatabaseOp::MoveCard {
            table: TABLE,
            view,
            row: SAM,
            lane: Some(lane),
            before: Some(ALEX),
            after: None,
        }
    );
    assert_eq!(
        read(json!({"kind": "delete_view", "table": TABLE, "view": view})).table(),
        TABLE
    );
}
