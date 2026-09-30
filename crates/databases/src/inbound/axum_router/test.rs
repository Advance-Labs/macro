use super::*;

mod fakes;

#[test]
fn a_column_request_takes_infer_type_in_camel_case_and_its_old_spelling() {
    let camel: CreateColumnRequest = serde_json::from_value(serde_json::json!({
        "inferType": true,
        "binding": {"kind": "existing", "property_definition_id": Uuid::nil()},
    }))
    .unwrap();
    assert!(camel.infer_type);

    let snake: CreateColumnRequest = serde_json::from_value(serde_json::json!({
        "infer_type": true,
        "binding": {"kind": "existing", "property_definition_id": Uuid::nil()},
    }))
    .unwrap();
    assert!(snake.infer_type);

    let omitted: CreateColumnRequest = serde_json::from_value(serde_json::json!({
        "binding": {"kind": "existing", "property_definition_id": Uuid::nil()},
    }))
    .unwrap();
    assert!(!omitted.infer_type);
}

#[test]
fn a_table_order_request_takes_table_ids_in_camel_case() {
    let first = Uuid::from_u128(1);
    let second = Uuid::from_u128(2);
    let request: ReorderTablesRequest = serde_json::from_value(serde_json::json!({
        "tableIds": [second, first],
    }))
    .unwrap();
    assert_eq!(request.table_ids, vec![second, first]);
}

#[test]
fn a_type_change_takes_clear_invalid_in_camel_case_and_refuses_by_default() {
    let body = serde_json::json!({ "dataType": "NUMBER", "baseVersion": 3 });
    let refusing: column_mutations::ChangeColumnTypeRequest = serde_json::from_value(body).unwrap();
    assert!(!refusing.clear_invalid);

    let body = serde_json::json!({ "dataType": "NUMBER", "baseVersion": 3, "clearInvalid": true });
    let clearing: column_mutations::ChangeColumnTypeRequest = serde_json::from_value(body).unwrap();
    assert!(clearing.clear_invalid);
}

#[test]
fn a_column_cast_reads_as_the_type_menu_expects() {
    let cast = crate::domain::models::ColumnCast {
        data_type: DataType::Number,
        is_multi_select: false,
        specific_entity_type: None,
        relation: false,
        cast: crate::domain::models::CastVerdict::Checked,
        reason: None,
        failures: 3,
        summary: Some("3 values aren't numbers".into()),
        examples: vec!["TBD".into(), "n/a".into(), "12.5.0".into()],
    };
    assert_eq!(
        serde_json::to_value(cast).unwrap(),
        serde_json::json!({
            "data_type": "NUMBER",
            "is_multi_select": false,
            "specific_entity_type": null,
            "relation": false,
            "cast": "checked",
            "reason": null,
            "failures": 3,
            "summary": "3 values aren't numbers",
            "examples": ["TBD", "n/a", "12.5.0"],
        })
    );
}

#[test]
fn an_ops_body_reads_every_op_kind() {
    let table = Uuid::from_u128(0x7ab1);
    let column = Uuid::from_u128(0xc01a);
    let row = Uuid::from_u128(0x5a11);
    let request: ops::ApplyOpsRequest = serde_json::from_value(serde_json::json!({
        "ops": [
            {
                "kind": "insert_rows",
                "table": table,
                "rows": [[{"column": column, "value": {"type": "text", "value": "Sam"}}]],
                "createMissingOptions": true,
            },
            {
                "kind": "update_rows",
                "table": table,
                "changes": {
                    "kind": "uniform",
                    "rows": [row],
                    "cells": [{"column": column, "value": {"type": "options", "value": [{"label": "Going"}]}}],
                },
            },
            {
                "kind": "update_rows",
                "table": table,
                "changes": {
                    "kind": "per_row",
                    "rows": [{"row": row, "cells": [{"column": column, "value": {"type": "clear"}}]}],
                },
            },
            {"kind": "delete_rows", "table": table, "rows": [row]},
            {
                "kind": "change_column_type",
                "table": table,
                "column": column,
                "to": {"type": "number"},
                "clearInvalid": true,
            },
        ],
    }))
    .unwrap();

    use models_databases::RowChanges;
    use models_databases::{CellValue, CellWrite, ColumnKind, DatabaseOp, OptionRef, RowChange};
    assert_eq!(
        request.ops,
        vec![
            DatabaseOp::InsertRows {
                table,
                rows: vec![vec![CellWrite {
                    column,
                    value: CellValue::Text("Sam".into()),
                }]],
                create_missing_options: true,
            },
            DatabaseOp::UpdateRows {
                table,
                changes: RowChanges::Uniform {
                    rows: vec![row],
                    cells: vec![CellWrite {
                        column,
                        value: CellValue::Options(vec![OptionRef::Label("Going".into())]),
                    }],
                },
                create_missing_options: false,
            },
            DatabaseOp::UpdateRows {
                table,
                changes: RowChanges::PerRow {
                    rows: vec![RowChange {
                        row,
                        cells: vec![CellWrite {
                            column,
                            value: CellValue::Clear,
                        }],
                    }],
                },
                create_missing_options: false,
            },
            DatabaseOp::DeleteRows {
                table,
                rows: vec![row],
            },
            DatabaseOp::ChangeColumnType {
                table,
                column,
                to: ColumnKind::Number,
                clear_invalid: true,
            },
        ]
    );
}

#[tokio::test]
async fn view_access_cannot_apply_ops() {
    use axum::body::Body;
    use axum::http::{Request, header};
    use entity_access::domain::models::AccessLevel;
    use tower::ServiceExt;

    let database = Uuid::from_u128(0x0dbb);
    let request = || {
        Request::post(format!("/{database}/ops"))
            .header(header::AUTHORIZATION, "Bearer valid")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"ops": []}"#))
            .unwrap()
    };

    let (viewing, service) = fakes::ops_router(AccessLevel::View);
    let response = viewing.oneshot(request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(*service.applied.lock().unwrap(), 0);

    let (editing, service) = fakes::ops_router(AccessLevel::Edit);
    let response = editing.oneshot(request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(*service.applied.lock().unwrap(), 1);
}
