use super::*;

/// The service's compare-and-swap needs the label being replaced; the tool
/// reads it rather than asking the model to repeat it.
#[tokio::test]
async fn renaming_a_column_replaces_its_current_label() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let response = RenameColumn {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        column_id: COLUMN_ID,
        name: " RSVP ".to_string(),
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect("edit access may rename a column");

    assert_eq!(response.column_id, COLUMN_ID);
    assert_eq!(response.name, "RSVP");
    assert!(response.database.is_some());
    assert_eq!(
        calls.lock().unwrap().renamed_columns,
        vec![(
            TABLE_ID,
            COLUMN_ID,
            " RSVP ".to_string(),
            "Status".to_string()
        )]
    );
}

#[tokio::test]
async fn renaming_an_unknown_column_points_at_describe() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let error = RenameColumn {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        column_id: Uuid::nil(),
        name: "RSVP".to_string(),
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect_err("the column is not in this table");

    assert!(
        error.description.contains("DescribeDatabase"),
        "{}",
        error.description
    );
    assert!(calls.lock().unwrap().renamed_columns.is_empty());
}

/// The model never passes a version: the tool converts against the version it
/// just read, then adds the labels no row has yet.
#[tokio::test]
async fn changing_a_column_type_uses_the_current_version_and_adds_extra_options() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let response = ChangeColumnType {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        column_id: COLUMN_ID,
        data_type: ColumnType::Select,
        is_multi_select: false,
        options: Some(vec!["Waitlisted".to_string()]),
        specific_entity_type: None,
        link_to_table_id: None,
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect("edit access may change a column's type");

    assert_eq!(response.column_id, COLUMN_ID);
    assert!(response.database.is_some());
    assert!(response.warning.is_none());
    let calls = calls.lock().unwrap();
    assert_eq!(calls.changed_column_types.len(), 1);
    let change = &calls.changed_column_types[0];
    assert_eq!(change.table_id, TABLE_ID);
    assert_eq!(change.column_id, COLUMN_ID);
    assert_eq!(change.data_type, DataType::SelectString);
    assert!(!change.is_multi_select);
    assert_eq!(change.specific_entity_type, None);
    assert_eq!(change.relation, None);
    assert_eq!(change.base_version, TableVersion(3));
    assert_eq!(
        calls.added_options,
        vec![(COLUMN_ID, vec!["Waitlisted".to_string()])]
    );
}

#[tokio::test]
async fn changing_a_column_to_a_relation_targets_this_database_and_holds_many_rows() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let parties = Uuid::from_u128(0x7ab1_0000_0000_0000_0000_0000_0000_0002);
    ChangeColumnType {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        column_id: COLUMN_ID,
        data_type: ColumnType::Entity,
        is_multi_select: false,
        options: None,
        specific_entity_type: None,
        link_to_table_id: Some(parties),
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect("a relation is an entity column with a target table");

    let calls = calls.lock().unwrap();
    let change = &calls.changed_column_types[0];
    assert_eq!(change.data_type, DataType::Entity);
    assert!(change.is_multi_select);
    assert_eq!(change.relation, Some((DATABASE_ID, parties)));
    assert!(calls.added_options.is_empty());
}

#[tokio::test]
async fn a_relation_with_another_type_is_refused_before_the_service() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let error = ChangeColumnType {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        column_id: COLUMN_ID,
        data_type: ColumnType::Text,
        is_multi_select: false,
        options: None,
        specific_entity_type: None,
        link_to_table_id: Some(TABLE_ID),
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect_err("a relation holds row ids, not text");

    assert!(
        error.description.contains("entity"),
        "{}",
        error.description
    );
    let calls = calls.lock().unwrap();
    assert!(calls.changed_column_types.is_empty());
    assert_eq!(calls.described, 0);
}

#[test]
fn entity_kinds_reach_the_property_system_by_their_stored_names() {
    let parsed: ChangeColumnType = serde_json::from_value(serde_json::json!({
        "databaseId": DATABASE_ID,
        "tableId": TABLE_ID,
        "columnId": COLUMN_ID,
        "dataType": "entity",
        "specificEntityType": "CALENDAR_EVENT",
    }))
    .unwrap();
    assert_eq!(
        parsed
            .specific_entity_type
            .map(models_properties::EntityType::from),
        Some(models_properties::EntityType::CalendarEvent)
    );
    assert!(
        serde_json::from_value::<ChangeColumnType>(serde_json::json!({
            "databaseId": DATABASE_ID,
            "tableId": TABLE_ID,
            "columnId": COLUMN_ID,
            "dataType": "entity",
            "specificEntityType": "DATABASE_ROW",
        }))
        .is_err(),
        "relations are made with linkToTableId"
    );
}

#[tokio::test]
async fn deleting_a_column_guards_on_the_version_just_read() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let response = DeleteColumn {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        column_id: COLUMN_ID,
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect("edit access may delete a column");

    assert_eq!(response.column_id, COLUMN_ID);
    assert!(response.database.is_some());
    assert_eq!(
        calls.lock().unwrap().deleted_columns,
        vec![(TABLE_ID, COLUMN_ID, TableVersion(3))]
    );
}

#[tokio::test]
async fn reordering_columns_guards_on_the_version_just_read() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let response = ReorderColumns {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        column_ids: vec![COLUMN_ID],
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect("edit access may reorder columns");

    assert_eq!(response.table_id, TABLE_ID);
    assert_eq!(
        calls.lock().unwrap().reordered_columns,
        vec![(TABLE_ID, vec![COLUMN_ID], TableVersion(3))]
    );
}

#[tokio::test]
async fn reordering_tables_passes_the_full_order_and_answers_the_schema() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let other_table = Uuid::from_u128(0x7ab1e);
    let response = ReorderTables {
        database_id: DATABASE_ID,
        table_ids: vec![other_table, TABLE_ID],
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect("edit access may reorder tables");

    assert_eq!(response.table_ids, vec![other_table, TABLE_ID]);
    assert!(response.database.is_some());
    assert_eq!(
        calls.lock().unwrap().reordered_tables,
        vec![vec![other_table, TABLE_ID]]
    );
}

#[tokio::test]
async fn reordering_tables_needs_more_than_view_access() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::View));
    let error = ReorderTables {
        database_id: DATABASE_ID,
        table_ids: vec![TABLE_ID],
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect_err("view access cannot reorder tabs");

    assert!(
        error
            .description
            .contains("does not have permission to edit"),
        "{}",
        error.description
    );
    assert!(calls.lock().unwrap().reordered_tables.is_empty());
}

#[tokio::test]
async fn deleting_a_table_needs_more_than_view_access() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::View));
    let error = DeleteTable {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect_err("view access cannot delete a tab");

    assert!(
        error
            .description
            .contains("does not have permission to edit"),
        "{}",
        error.description
    );
    assert!(calls.lock().unwrap().deleted_tables.is_empty());
}

#[tokio::test]
async fn deleting_a_table_returns_the_schema_after_it() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let response = DeleteTable {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect("edit access may delete a tab");

    assert_eq!(response.table_id, TABLE_ID);
    assert!(response.database.is_some());
    assert_eq!(calls.lock().unwrap().deleted_tables, vec![TABLE_ID]);
}

#[tokio::test]
async fn renaming_a_database_returns_its_new_name() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let response = RenameDatabase {
        database_id: DATABASE_ID,
        name: "Party Planning".to_string(),
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect("edit access may rename a database");

    assert_eq!(response.database_id, DATABASE_ID);
    assert_eq!(response.name, "Party Planning");
    assert!(response.database.is_some());
    assert_eq!(
        calls.lock().unwrap().renamed_databases,
        vec!["Party Planning".to_string()]
    );
}
