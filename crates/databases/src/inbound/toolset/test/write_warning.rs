use super::*;

#[test]
fn warnings_serialize_as_one_string_in_order() {
    let warnings = WriteWarnings(vec![
        WriteWarning::OptionsNotAdded {
            cause: "The label is too long.".into(),
        },
        WriteWarning::SchemaNotRefreshed {
            database_id: DATABASE_ID,
            cause: "The databases service failed.".into(),
        },
    ]);

    assert_eq!(
        serde_json::to_value(&warnings).unwrap(),
        serde_json::json!(format!(
            "The type changed, but the extra options were not added: The label is too long. \
             Retry them with AddColumnOptions. The change was saved, but its schema could not \
             be refreshed: The databases service failed. Call DescribeDatabase with databaseId \
             {DATABASE_ID} before continuing; do not repeat this successful mutation."
        ))
    );
}
