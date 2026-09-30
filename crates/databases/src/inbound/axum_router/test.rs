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
