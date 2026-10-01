use super::*;

/// The description is what stops a model creating an optionless select column
/// and then failing every INSERT against it.
#[test]
fn add_column_teaches_that_options_are_explicit() {
    let validated = generate_validated_input_schema::<AddColumn>().expect("schema should validate");

    assert!(
        validated.description.contains("explicit schema"),
        "{}",
        validated.description
    );
    assert!(
        validated.description.contains("AddColumnOptions"),
        "the description must point at the way to add more: {}",
        validated.description
    );
}

#[tokio::test]
async fn adding_options_needs_more_than_view_access() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::View));
    let error = AddColumnOptions {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        column_id: COLUMN_ID,
        labels: vec!["Waitlisted".to_string()],
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect_err("view access cannot change the schema");

    assert!(
        error
            .description
            .contains("does not have permission to edit"),
        "{}",
        error.description
    );
    assert!(calls.lock().unwrap().added_options.is_empty());
}

/// The response carries the labels SQL now accepts, so the model can write the
/// statement that just failed without describing the database again.
#[tokio::test]
async fn adding_options_returns_the_labels_sql_accepts() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let response = AddColumnOptions {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        column_id: COLUMN_ID,
        labels: vec!["Waitlisted".to_string()],
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect("edit access may extend a select column");

    assert_eq!(response.column_id, COLUMN_ID);
    assert_eq!(response.options, vec!["Going", "Declined", "Waitlisted"]);
    assert_eq!(
        calls.lock().unwrap().added_options,
        vec![(COLUMN_ID, vec!["Waitlisted".to_string()])]
    );
}

#[test]
fn describe_column_schema_accepts_omitted_select_options() {
    let schema = serde_json::to_value(schemars::schema_for!(ToolColumn)).unwrap();
    let required = schema["required"].as_array().unwrap();
    assert!(
        !required.contains(&serde_json::json!("options")),
        "non-select columns omit empty options in actual tool responses"
    );
    assert!(required.contains(&serde_json::json!("name")));
}
