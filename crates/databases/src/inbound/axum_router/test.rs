use super::*;

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
