use super::*;

#[tokio::test]
async fn number_text_conversion_writes_converted_cells_through_the_cell_store() {
    let seeded = seeded().await;
    let (world, svc, db, table_id, row_id, plus_ones) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.row_id,
        seeded.plus_ones_column,
    );
    let old = plus_ones.property_definition_id;
    let outcome = svc
        .change_column_type(
            receipt(db, OWNER, AccessLevel::Edit),
            viewer(OWNER),
            ChangeColumnType {
                table_id,
                column_id: plus_ones.id,
                data_type: DataType::String,
                is_multi_select: false,
                specific_entity_type: None,
                relation: None,
                base_version: TableVersion(1),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        outcome.table_versions,
        HashMap::from([(table_id, TableVersion(2))])
    );
    let as_text = {
        let w = world.lock().unwrap();
        let column = w.columns.iter().find(|c| c.id == plus_ones.id).unwrap();
        assert_ne!(column.property_definition_id, old);
        assert!(!column.infer_type);
        assert_eq!(w.definitions[&old].definition.data_type, DataType::Number);
        assert_eq!(
            w.definitions[&column.property_definition_id]
                .definition
                .data_type,
            DataType::String
        );
        assert_eq!(
            w.definitions[&column.property_definition_id]
                .definition
                .display_name,
            "Plus ones"
        );
        assert_eq!(
            w.cells[&row_id][&column.property_definition_id],
            PropertyValue::Str("2".into())
        );
        assert_eq!(
            w.cells[&row_id][&old],
            PropertyValue::Num(2.0),
            "the old definition's cell is left behind; no column reads it"
        );
        assert_eq!(w.published.last(), Some(&(table_id, TableVersion(2))));
        column.property_definition_id
    };
    let read = svc
        .query_sql(viewer(OWNER), "SELECT \"Plus ones\" FROM guests".into())
        .await
        .unwrap();
    assert_eq!(
        read.results[0].rows,
        vec![vec![
            SqlValue::Text(row_id.to_string()),
            SqlValue::Text("2".into())
        ]]
    );

    svc.change_column_type(
        receipt(db, OWNER, AccessLevel::Edit),
        viewer(OWNER),
        ChangeColumnType {
            table_id,
            column_id: plus_ones.id,
            data_type: DataType::Number,
            is_multi_select: false,
            specific_entity_type: None,
            relation: None,
            base_version: TableVersion(2),
        },
    )
    .await
    .unwrap();
    let w = world.lock().unwrap();
    let column = w.columns.iter().find(|c| c.id == plus_ones.id).unwrap();
    assert_ne!(column.property_definition_id, as_text);
    assert_eq!(
        w.cells[&row_id][&column.property_definition_id],
        PropertyValue::Num(2.0)
    );
    assert!(w.definitions.contains_key(&as_text));
}

#[tokio::test]
async fn invalid_or_lossy_conversions_do_not_modify_the_column() {
    for value in ["0012", "9007199254740993", "2.00", " 2", "hello"] {
        let seeded = seeded().await;
        let (world, svc, db, table_id, row_id, name) = (
            seeded.world,
            seeded.service,
            seeded.database_id,
            seeded.table_id,
            seeded.row_id,
            seeded.name_column,
        );
        let old = name.property_definition_id;
        world
            .lock()
            .unwrap()
            .cells
            .get_mut(&row_id)
            .unwrap()
            .insert(old, PropertyValue::Str(value.into()));
        let result = svc
            .change_column_type(
                receipt(db, OWNER, AccessLevel::Edit),
                viewer(OWNER),
                ChangeColumnType {
                    table_id,
                    column_id: name.id,
                    data_type: DataType::Number,
                    is_multi_select: false,
                    specific_entity_type: None,
                    relation: None,
                    base_version: TableVersion(1),
                },
            )
            .await;
        assert!(
            matches!(result, Err(DatabaseError::InvalidSchemaOperation(_))),
            "{value}: {result:?}"
        );
        let w = world.lock().unwrap();
        assert_eq!(w.definitions.len(), 3);
        assert_eq!(w.tables[0].version, TableVersion(1));
        assert_eq!(
            w.columns
                .iter()
                .find(|c| c.id == name.id)
                .unwrap()
                .property_definition_id,
            old
        );
        assert_eq!(w.cells[&row_id][&old], PropertyValue::Str(value.into()));
    }
}

#[tokio::test]
async fn selecting_text_preserves_option_labels_and_select_preserves_unused_options() {
    let seeded = seeded().await;
    let (world, svc, db, table_id, row_id, status) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.row_id,
        seeded.status_column,
    );
    svc.change_column_type(
        receipt(db, OWNER, AccessLevel::Edit),
        viewer(OWNER),
        ChangeColumnType {
            table_id,
            column_id: status.id,
            data_type: DataType::SelectString,
            is_multi_select: true,
            specific_entity_type: None,
            relation: None,
            base_version: TableVersion(1),
        },
    )
    .await
    .unwrap();
    {
        let w = world.lock().unwrap();
        let column = w.columns.iter().find(|c| c.id == status.id).unwrap();
        let definition = &w.definitions[&column.property_definition_id];
        assert!(definition.definition.is_multi_select);
        assert_eq!(
            catalog::option_labels(definition)
                .into_iter()
                .map(|(_, label)| label)
                .collect::<Vec<_>>(),
            vec!["Going", "Declined"]
        );
        let going = definition.property_options[0].id;
        assert_eq!(
            w.cells[&row_id][&column.property_definition_id],
            PropertyValue::SelectOption(vec![going])
        );
    }
    let read = svc
        .query_sql(viewer(OWNER), "SELECT status FROM guests".into())
        .await
        .unwrap();
    assert_eq!(
        read.results[0].rows[0][1],
        SqlValue::Text("[\"Going\"]".into()),
        "a multi-select reads as a JSON array of labels"
    );

    svc.change_column_type(
        receipt(db, OWNER, AccessLevel::Edit),
        viewer(OWNER),
        ChangeColumnType {
            table_id,
            column_id: status.id,
            data_type: DataType::String,
            is_multi_select: false,
            specific_entity_type: None,
            relation: None,
            base_version: TableVersion(2),
        },
    )
    .await
    .unwrap();
    let w = world.lock().unwrap();
    let column = w.columns.iter().find(|c| c.id == status.id).unwrap();
    assert_eq!(
        w.cells[&row_id][&column.property_definition_id],
        PropertyValue::Str("Going".into())
    );
}

#[tokio::test]
async fn reorder_validates_complete_ids_and_delete_preserves_definitions() {
    let seeded = seeded().await;
    let (world, svc, db, table_id) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
    );
    let ids = [
        seeded.name_column.id,
        seeded.status_column.id,
        seeded.plus_ones_column.id,
    ];
    for invalid in [
        vec![],
        vec![ids[0]; 3],
        vec![Uuid::new_v4(); 3],
        vec![ids[0], ids[1]],
    ] {
        assert!(matches!(
            svc.reorder_columns(
                receipt(db, OWNER, AccessLevel::Edit),
                table_id,
                invalid,
                TableVersion(1)
            )
            .await,
            Err(DatabaseError::InvalidSchemaOperation(_))
        ));
    }
    let reordered = svc
        .reorder_columns(
            receipt(db, OWNER, AccessLevel::Edit),
            table_id,
            vec![ids[2], ids[1], ids[0]],
            TableVersion(1),
        )
        .await
        .unwrap();
    assert_eq!(
        reordered.table_versions,
        HashMap::from([(table_id, TableVersion(2))])
    );
    let detail = svc
        .get_database(
            receipt::<ViewAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
        )
        .await
        .unwrap();
    assert_eq!(
        detail.tables[0]
            .columns
            .iter()
            .map(|column| column.column.id)
            .collect::<Vec<_>>(),
        vec![ids[2], ids[1], ids[0]]
    );

    let deleted = svc
        .delete_column(
            receipt(db, OWNER, AccessLevel::Edit),
            table_id,
            ids[0],
            TableVersion(2),
        )
        .await
        .unwrap();
    assert_eq!(
        deleted.table_versions,
        HashMap::from([(table_id, TableVersion(3))])
    );
    {
        let w = world.lock().unwrap();
        assert!(!w.columns.iter().any(|c| c.id == ids[0]));
        assert_eq!(w.definitions.len(), 3);
        assert_eq!(w.published.last(), Some(&(table_id, TableVersion(3))));
    }
    let answer = svc
        .query_sql(viewer(OWNER), "SELECT * FROM guests".into())
        .await
        .unwrap();
    assert_eq!(
        answer.results[0]
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        vec!["row_id", "Plus ones", "Status"]
    );
}

#[tokio::test]
async fn schema_mutations_reject_wrong_database_stale_and_trashed_database() {
    let seeded = seeded().await;
    let (world, svc, db, table_id) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
    );
    let ids = vec![
        seeded.name_column.id,
        seeded.status_column.id,
        seeded.plus_ones_column.id,
    ];
    let change = |base_version: TableVersion| ChangeColumnType {
        table_id,
        column_id: seeded.name_column.id,
        data_type: DataType::String,
        is_multi_select: false,
        specific_entity_type: None,
        relation: None,
        base_version,
    };

    let elsewhere = Uuid::new_v4();
    assert!(matches!(
        svc.change_column_type(
            receipt(elsewhere, OWNER, AccessLevel::Edit),
            viewer(OWNER),
            change(TableVersion(1))
        )
        .await,
        Err(DatabaseError::NotFound)
    ));
    assert!(matches!(
        svc.delete_column(
            receipt(elsewhere, OWNER, AccessLevel::Edit),
            table_id,
            seeded.name_column.id,
            TableVersion(1)
        )
        .await,
        Err(DatabaseError::NotFound)
    ));
    assert!(matches!(
        svc.reorder_columns(
            receipt(elsewhere, OWNER, AccessLevel::Edit),
            table_id,
            ids.clone(),
            TableVersion(1)
        )
        .await,
        Err(DatabaseError::NotFound)
    ));

    assert!(matches!(
        svc.change_column_type(
            receipt(db, OWNER, AccessLevel::Edit),
            viewer(OWNER),
            change(TableVersion(0))
        )
        .await,
        Err(DatabaseError::VersionConflict)
    ));
    assert!(matches!(
        svc.delete_column(
            receipt(db, OWNER, AccessLevel::Edit),
            table_id,
            seeded.name_column.id,
            TableVersion(0)
        )
        .await,
        Err(DatabaseError::VersionConflict)
    ));
    assert!(matches!(
        svc.reorder_columns(
            receipt(db, OWNER, AccessLevel::Edit),
            table_id,
            ids.clone(),
            TableVersion(0)
        )
        .await,
        Err(DatabaseError::VersionConflict)
    ));

    world.lock().unwrap().databases[0].trashed_at = Some(Utc::now());
    assert!(matches!(
        svc.change_column_type(
            receipt(db, OWNER, AccessLevel::Edit),
            viewer(OWNER),
            change(TableVersion(1))
        )
        .await,
        Err(DatabaseError::NotFound)
    ));
    assert!(matches!(
        svc.delete_column(
            receipt(db, OWNER, AccessLevel::Edit),
            table_id,
            seeded.name_column.id,
            TableVersion(1)
        )
        .await,
        Err(DatabaseError::NotFound)
    ));
    assert!(matches!(
        svc.reorder_columns(
            receipt(db, OWNER, AccessLevel::Edit),
            table_id,
            ids,
            TableVersion(1)
        )
        .await,
        Err(DatabaseError::NotFound)
    ));

    let w = world.lock().unwrap();
    assert_eq!(w.tables[0].version, TableVersion(1));
    assert_eq!(w.columns.len(), 3);
    assert_eq!(w.definitions.len(), 3);
}

#[tokio::test]
async fn lookup_dependencies_prevent_deleting_or_retyping_their_source_column() {
    let seeded = seeded().await;
    let (world, svc, db, table_id, name) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.name_column,
    );
    {
        let mut w = world.lock().unwrap();
        let mut lookup = w.columns[0].clone();
        lookup.id = Uuid::now_v7();
        lookup.config = Some(ColumnConfig::Lookup {
            via_column_id: name.id,
            target: "Name".into(),
        });
        w.columns.push(lookup);
    }
    assert!(matches!(
        svc.change_column_type(
            receipt(db, OWNER, AccessLevel::Edit),
            viewer(OWNER),
            ChangeColumnType {
                table_id,
                column_id: name.id,
                data_type: DataType::String,
                is_multi_select: false,
                specific_entity_type: None,
                relation: None,
                base_version: TableVersion(1),
            }
        )
        .await,
        Err(DatabaseError::InvalidSchemaOperation(_))
    ));
    assert!(matches!(
        svc.delete_column(
            receipt(db, OWNER, AccessLevel::Edit),
            table_id,
            name.id,
            TableVersion(1)
        )
        .await,
        Err(DatabaseError::InvalidSchemaOperation(_))
    ));
    let w = world.lock().unwrap();
    assert_eq!(w.tables[0].version, TableVersion(1));
    assert_eq!(w.columns.len(), 4);
}
