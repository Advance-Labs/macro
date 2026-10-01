use super::*;

#[tokio::test]
async fn describe_needs_a_view_receipt() {
    let (context, calls) = context(FakeAccess::denying());
    let error = DescribeDatabase {
        database_id: DATABASE_ID,
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect_err("no access means no schema");

    assert!(
        error
            .description
            .contains("does not have permission to read"),
        "{}",
        error.description
    );
    assert_eq!(
        calls.lock().unwrap().described,
        0,
        "the service must not be reached without a receipt"
    );
}

/// View access reads; it does not create tables. The receipt type is what
/// draws that line, and it is drawn before the service is touched.
#[tokio::test]
async fn creating_a_table_needs_more_than_view_access() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::View));
    let error = CreateTable {
        database_id: DATABASE_ID,
        name: "Sessions".to_string(),
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
    assert!(calls.lock().unwrap().created_tables.is_empty());
}

#[tokio::test]
async fn renaming_a_table_needs_more_than_view_access() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::View));
    let error = RenameTable {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        name: "Attendees".to_string(),
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect_err("view access cannot rename a tab");

    assert!(
        error
            .description
            .contains("does not have permission to edit"),
        "{}",
        error.description
    );
    assert!(calls.lock().unwrap().renamed_tables.is_empty());
}

/// The service's compare-and-swap needs the name being replaced; the tool
/// supplies the current one rather than asking the model to repeat it.
#[tokio::test]
async fn renaming_a_table_replaces_its_current_name() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let response = RenameTable {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        name: "Attendees".to_string(),
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect("edit access may rename a tab");

    assert_eq!(response.table_id, TABLE_ID);
    assert_eq!(response.name, "Attendees");
    assert!(response.database.is_some());
    assert_eq!(
        calls.lock().unwrap().renamed_tables,
        vec![(TABLE_ID, "Attendees".to_string(), "Guests".to_string())]
    );
}

#[tokio::test]
async fn renaming_an_unknown_table_points_at_describe() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let error = RenameTable {
        database_id: DATABASE_ID,
        table_id: Uuid::nil(),
        name: "Attendees".to_string(),
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect_err("the table is not in this database");

    assert!(
        error.description.contains("DescribeDatabase"),
        "{}",
        error.description
    );
    assert!(calls.lock().unwrap().renamed_tables.is_empty());
}

#[tokio::test]
async fn creating_a_table_with_edit_access_succeeds() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let response = CreateTable {
        database_id: DATABASE_ID,
        name: "Sessions".to_string(),
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect("edit access may add a tab");

    assert_eq!(response.table_id, TABLE_ID);
    assert_eq!(calls.lock().unwrap().created_tables, vec!["Sessions"]);
    assert_eq!(
        response.database.expect("schema refresh succeeds").tables[0].sql_name,
        "\"Offsite\".\"Guests\"",
        "the SQL name is the display name, quoted"
    );
}

#[tokio::test]
async fn adding_a_column_needs_more_than_view_access() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::View));
    let error = AddColumn {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        name: "Dietary Needs".to_string(),
        data_type: ColumnType::Select,
        is_multi_select: true,
        options: Some(vec!["Vegan".to_string()]),
        link_to_table_id: None,
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
    assert!(calls.lock().unwrap().created_columns.is_empty());
}

/// The tool's vocabulary has to reach the property system unchanged, or a
/// column is created as one type and read back as another.
#[tokio::test]
async fn adding_a_column_passes_the_type_through() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let response = AddColumn {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        name: "Dietary Needs".to_string(),
        data_type: ColumnType::Select,
        is_multi_select: true,
        options: Some(vec!["Vegan".to_string(), "Gluten-free".to_string()]),
        link_to_table_id: None,
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect("edit access may add a column");

    assert_eq!(response.column_id, COLUMN_ID);
    assert_eq!(
        calls.lock().unwrap().created_columns,
        vec![(
            TABLE_ID,
            DataType::SelectString,
            true,
            vec!["Vegan".to_string(), "Gluten-free".to_string()]
        )]
    );
}

#[test]
fn column_types_round_trip_through_the_property_system() {
    for column_type in [
        ColumnType::Text,
        ColumnType::Number,
        ColumnType::Boolean,
        ColumnType::Date,
        ColumnType::Link,
        ColumnType::Select,
        ColumnType::SelectNumber,
        ColumnType::Tag,
        ColumnType::Entity,
    ] {
        let stored: DataType = column_type.into();
        assert_eq!(
            ColumnType::from(stored),
            column_type,
            "{column_type:?} did not round trip"
        );
    }
}
