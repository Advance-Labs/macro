use super::*;

#[tokio::test]
async fn label_rename_preserves_the_binding_moves_the_sql_name_and_retries_idempotently() {
    let seeded = seeded().await;
    let (world, svc, db, table_id, row_id, column) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.row_id,
        seeded.name_column,
    );
    let result = svc
        .rename_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Edit),
            table_id,
            column.id,
            "  Task  ".into(),
            "Name".into(),
        )
        .await
        .unwrap();
    assert_eq!(result.column.id, column.id);
    assert_eq!(
        result.column.property_definition_id,
        column.property_definition_id
    );
    assert_eq!(result.column.display_name.as_deref(), Some("Task"));
    assert_eq!(result.table_version, TableVersion(2));
    assert_eq!(
        world.lock().unwrap().published.last(),
        Some(&(table_id, TableVersion(2)))
    );

    // SQL follows the label: the cell is the same, the name is new.
    let answer = svc
        .query_sql(viewer(OWNER), "SELECT task FROM guests".into())
        .await
        .unwrap();
    assert_eq!(answer.results[0].columns[1].name, "Task");
    assert_eq!(
        answer.results[0].rows,
        vec![vec![
            SqlValue::Text(row_id.to_string()),
            SqlValue::Text("Sam".into())
        ]]
    );
    let error = svc
        .query_sql(viewer(OWNER), "SELECT name FROM guests".into())
        .await
        .unwrap_err();
    assert!(
        matches!(error, QueryError::Sql(ref message) if message == "unknown column \"name\" in Offsite.Guests"),
        "{error:?}"
    );
    let detail = svc
        .get_database(
            receipt::<ViewAccessLevel>(db, VIEWER, AccessLevel::View),
            viewer(VIEWER),
        )
        .await
        .unwrap();
    let renamed = detail.tables[0]
        .columns
        .iter()
        .find(|entry| entry.column.id == column.id)
        .unwrap();
    assert_eq!(renamed.sql_name, "\"Task\"");
    assert_eq!(renamed.definition.definition.display_name, "Name");
    assert_eq!(renamed.column.display_name.as_deref(), Some("Task"));

    let retried = svc
        .rename_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Edit),
            table_id,
            column.id,
            "Task".into(),
            "Name".into(),
        )
        .await
        .unwrap();
    assert_eq!(retried.table_version, result.table_version);
    let error = svc
        .rename_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Edit),
            table_id,
            column.id,
            "Work item".into(),
            "Name".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DatabaseError::InvalidSchemaOperation(_)));
    assert_eq!(
        world
            .lock()
            .unwrap()
            .columns
            .iter()
            .find(|candidate| candidate.id == column.id)
            .unwrap()
            .display_name
            .as_deref(),
        Some("Task")
    );
}

#[tokio::test]
async fn rename_checks_effective_labels_and_creation_respects_renamed_labels() {
    let seeded = seeded().await;
    let (svc, db, table_id, name_column, status_column) = (
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.name_column,
        seeded.status_column,
    );
    for invalid in [" ", " status ", &"x".repeat(201)] {
        let error = svc
            .rename_column(
                receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Edit),
                table_id,
                name_column.id,
                invalid.into(),
                "Name".into(),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(error, DatabaseError::InvalidSchemaOperation(_)),
            "{invalid}"
        );
    }
    svc.rename_column(
        receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Edit),
        table_id,
        name_column.id,
        "Task".into(),
        "Name".into(),
    )
    .await
    .unwrap();
    let error = svc
        .rename_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Edit),
            table_id,
            status_column.id,
            " task ".into(),
            "Status".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DatabaseError::InvalidSchemaOperation(_)));
    let error = svc
        .create_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Edit),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id,
                binding: ColumnBinding::NewDefinition {
                    name: " TASK ".into(),
                    data_type: DataType::String,
                    is_multi_select: false,
                    options: vec![],
                },
                config: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DatabaseError::InvalidSchemaOperation(_)));
}

#[tokio::test]
async fn rename_refuses_foreign_columns_tables_and_trashed_database() {
    let seeded = seeded().await;
    let (world, svc, db, table_id, column) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.name_column,
    );
    let other = svc
        .create_database(CreateDatabase {
            name: "Other".into(),
            owner_id: user(OWNER),
            acting_bot: None,
        })
        .await
        .unwrap();
    for (receipt_db, target_table, target_column) in [
        (other.id, table_id, column.id),
        (db, Uuid::new_v4(), column.id),
        (db, table_id, Uuid::new_v4()),
    ] {
        let error = svc
            .rename_column(
                receipt::<EditAccessLevel>(receipt_db, OWNER, AccessLevel::Edit),
                target_table,
                target_column,
                "Task".into(),
                "Name".into(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, DatabaseError::NotFound));
    }
    svc.trash_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    let error = svc
        .rename_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            table_id,
            column.id,
            "Task".into(),
            "Name".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DatabaseError::NotFound));
    assert!(
        world
            .lock()
            .unwrap()
            .columns
            .iter()
            .find(|candidate| candidate.id == column.id)
            .unwrap()
            .display_name
            .is_none()
    );
}

#[tokio::test]
async fn reusing_a_previous_label_keeps_the_renamed_columns_values_intact() {
    let seeded = seeded().await;
    let (svc, db, table_id, row_id, column) = (
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.row_id,
        seeded.name_column,
    );
    svc.rename_column(
        receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Edit),
        table_id,
        column.id,
        "Task".into(),
        "Name".into(),
    )
    .await
    .unwrap();
    let added_id = svc
        .create_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Edit),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "Name".into(),
                    data_type: DataType::String,
                    is_multi_select: false,
                    options: vec![],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    let detail = svc
        .get_database(
            receipt::<ViewAccessLevel>(db, OWNER, AccessLevel::View),
            viewer(OWNER),
        )
        .await
        .unwrap();
    let columns = &detail.tables[0].columns;
    let renamed = columns
        .iter()
        .find(|entry| entry.column.id == column.id)
        .unwrap();
    let added = columns
        .iter()
        .find(|entry| entry.column.id == added_id)
        .unwrap();
    assert_eq!(renamed.sql_name, "\"Task\"");
    assert_eq!(renamed.column.display_name.as_deref(), Some("Task"));
    assert_eq!(added.sql_name, "\"Name\"");
    assert_eq!(added.definition.definition.display_name, "Name");
    assert_ne!(
        added.definition.definition.id,
        renamed.definition.definition.id
    );

    // The old label now names the new, empty column; the value stayed with
    // the renamed one.
    let answer = svc
        .query_sql(viewer(OWNER), "SELECT task, name FROM guests".into())
        .await
        .unwrap();
    assert_eq!(
        answer.results[0].rows,
        vec![vec![
            SqlValue::Text(row_id.to_string()),
            SqlValue::Text("Sam".into()),
            SqlValue::Null,
        ]]
    );
}
