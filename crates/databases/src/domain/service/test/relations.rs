//! Relation columns through the list syntax: a cell of row ids, written as
//! `['id', …]` and read back as a JSON array.

use super::*;

/// The seeded database with a `Sessions(Title)` table holding `Keynote`,
/// and a relation column `Sessions` on `Guests` pointing at it.
struct Linked {
    seeded: Seeded,
    sessions_table: TableId,
    keynote_row: RowId,
    relation_column: Column,
}

async fn linked() -> Linked {
    let seeded = seeded().await;
    let sessions = seeded
        .service
        .create_table(
            receipt::<EditAccessLevel>(seeded.database_id, OWNER, AccessLevel::Owner),
            CreateTable {
                database_id: seeded.database_id,
                name: "Sessions".into(),
            },
        )
        .await
        .unwrap();
    seeded
        .service
        .create_column(
            receipt::<EditAccessLevel>(seeded.database_id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: sessions.id,
                binding: ColumnBinding::NewDefinition {
                    name: "Title".into(),
                    data_type: DataType::String,
                    is_multi_select: false,
                    options: vec![],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    let relation_column = seeded
        .service
        .create_column(
            receipt::<EditAccessLevel>(seeded.database_id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: seeded.table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "Sessions".into(),
                    data_type: DataType::Entity,
                    is_multi_select: true,
                    options: vec![],
                },
                config: Some(ColumnConfig::Link {
                    database_id: seeded.database_id,
                    table_id: sessions.id,
                }),
            },
        )
        .await
        .unwrap();
    let keynote = seeded
        .service
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: "INSERT INTO sessions (title) VALUES ('Keynote')".into(),
                base_versions: None,
            },
        )
        .await
        .unwrap();
    let relation_column = seeded
        .world
        .lock()
        .unwrap()
        .columns
        .iter()
        .find(|column| column.id == relation_column)
        .unwrap()
        .clone();
    Linked {
        seeded,
        sessions_table: sessions.id,
        keynote_row: keynote.inserted_row_ids[0],
        relation_column,
    }
}

#[tokio::test]
async fn a_relation_written_as_a_list_reads_back_as_row_ids() {
    let Linked {
        seeded,
        sessions_table,
        keynote_row,
        relation_column,
    } = linked().await;
    let (world, svc, table_id, row_id) =
        (seeded.world, seeded.service, seeded.table_id, seeded.row_id);

    let empty = svc
        .query_sql(viewer(OWNER), "SELECT sessions FROM guests".into())
        .await
        .unwrap();
    assert_eq!(
        empty.results[0].columns[1].entity_type,
        Some(model_entity::EntityType::DatabaseRow)
    );
    assert_eq!(
        empty.results[0].rows,
        vec![vec![SqlValue::Text(row_id.to_string()), SqlValue::Null]]
    );

    let outcome = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: format!(
                    "UPDATE guests SET sessions = ['{keynote_row}'] WHERE row_id = '{row_id}'"
                ),
                base_versions: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(outcome.changes_applied, 1);
    assert_eq!(
        outcome.new_versions,
        HashMap::from([(table_id, TableVersion(2))]),
        "only the table holding the cell moves"
    );
    assert_eq!(
        world.lock().unwrap().cells[&row_id][&relation_column.property_definition_id],
        PropertyValue::EntityRef(vec![models_properties::EntityReference {
            entity_id: keynote_row.to_string(),
            entity_type: PropertyEntityType::DatabaseRow,
            specific_message_id: None,
        }])
    );

    let linked = svc
        .query_sql(viewer(OWNER), "SELECT name, sessions FROM guests".into())
        .await
        .unwrap();
    assert_eq!(
        linked.results[0].rows,
        vec![vec![
            SqlValue::Text(row_id.to_string()),
            SqlValue::Text("Sam".into()),
            SqlValue::Text(format!("[\"{keynote_row}\"]")),
        ]]
    );
    assert_eq!(linked.read_tables, vec![table_id]);

    let joined = svc
        .query_sql(
            viewer(OWNER),
            "SELECT g.name, s.title FROM guests g JOIN sessions s ON g.sessions = s.row_id".into(),
        )
        .await
        .unwrap();
    assert_eq!(
        joined.results[0].rows,
        vec![vec![
            SqlValue::Text(row_id.to_string()),
            SqlValue::Text("Sam".into()),
            SqlValue::Text("Keynote".into()),
        ]]
    );
    assert_eq!(joined.read_tables, vec![table_id, sessions_table]);

    // A new guest can be linked in the same insert.
    let inserted = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: format!(
                    "INSERT INTO guests (name, sessions) VALUES ('Ada', ['{keynote_row}'])"
                ),
                base_versions: None,
            },
        )
        .await
        .unwrap();
    let attending = svc
        .query_sql(
            viewer(OWNER),
            format!("SELECT name FROM guests WHERE sessions HAS '{keynote_row}' ORDER BY name"),
        )
        .await
        .unwrap();
    assert_eq!(
        attending.results[0].rows,
        vec![
            vec![
                SqlValue::Text(inserted.inserted_row_ids[0].to_string()),
                SqlValue::Text("Ada".into())
            ],
            vec![
                SqlValue::Text(row_id.to_string()),
                SqlValue::Text("Sam".into())
            ],
        ]
    );
}

#[tokio::test]
async fn null_clears_a_relation_and_an_empty_list_is_not_a_value() {
    let Linked {
        seeded,
        keynote_row,
        relation_column,
        ..
    } = linked().await;
    let (world, svc, row_id) = (seeded.world, seeded.service, seeded.row_id);
    svc.exec_sql(
        viewer(OWNER),
        ExecRequest {
            scope: None,
            sql: format!(
                "UPDATE guests SET sessions = ['{keynote_row}'] WHERE row_id = '{row_id}'"
            ),
            base_versions: None,
        },
    )
    .await
    .unwrap();

    let error = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: format!("UPDATE guests SET sessions = [] WHERE row_id = '{row_id}'"),
                base_versions: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            QueryError::Sql(ref message)
                if message == "expected a value: 'text', a number, TRUE, FALSE or NULL, found ] at 30..31"
        ),
        "{error:?}"
    );
    assert_eq!(
        world.lock().unwrap().cells[&row_id][&relation_column.property_definition_id],
        PropertyValue::EntityRef(vec![models_properties::EntityReference {
            entity_id: keynote_row.to_string(),
            entity_type: PropertyEntityType::DatabaseRow,
            specific_message_id: None,
        }])
    );

    let outcome = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: format!("UPDATE guests SET sessions = NULL WHERE row_id = '{row_id}'"),
                base_versions: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(outcome.changes_applied, 1);
    assert!(
        !world.lock().unwrap().cells[&row_id].contains_key(&relation_column.property_definition_id)
    );
    let cleared = svc
        .query_sql(viewer(OWNER), "SELECT sessions FROM guests".into())
        .await
        .unwrap();
    assert_eq!(
        cleared.results[0].rows,
        vec![vec![SqlValue::Text(row_id.to_string()), SqlValue::Null]]
    );
}

#[tokio::test]
async fn relation_values_must_be_ids_and_a_viewer_cannot_write_them() {
    let Linked {
        seeded,
        keynote_row,
        ..
    } = linked().await;
    let (world, svc, row_id) = (seeded.world, seeded.service, seeded.row_id);
    let cells_before = world.lock().unwrap().cells.clone();

    let error = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: format!("UPDATE guests SET sessions = ['Keynote'] WHERE row_id = '{row_id}'"),
                base_versions: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            QueryError::Sql(ref message)
                if message == "\"Sessions\" is an entity column; give an id like 'macro|sam@example.com', not a name"
        ),
        "{error:?}"
    );

    let error = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: format!("SELECT name FROM guests WHERE sessions = '{keynote_row}'"),
                base_versions: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            QueryError::Sql(ref message)
                if message == "\"Sessions\" holds several values; use HAS instead of ="
        ),
        "{error:?}"
    );

    let error = svc
        .exec_sql(
            viewer(VIEWER),
            ExecRequest {
                scope: None,
                sql: format!(
                    "UPDATE guests SET sessions = ['{keynote_row}'] WHERE row_id = '{row_id}'"
                ),
                base_versions: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(error, QueryError::ReadOnly(ref message) if message == "table Guests is read-only"),
        "{error:?}"
    );

    assert_eq!(world.lock().unwrap().cells, cells_before);
}

#[tokio::test]
async fn changing_a_linked_columns_type_requires_clearing_its_relations_first() {
    let Linked {
        seeded,
        keynote_row,
        relation_column,
        ..
    } = linked().await;
    let (world, svc, db, table_id, row_id) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.row_id,
    );
    svc.exec_sql(
        viewer(OWNER),
        ExecRequest {
            scope: None,
            sql: format!(
                "UPDATE guests SET sessions = ['{keynote_row}'] WHERE row_id = '{row_id}'"
            ),
            base_versions: None,
        },
    )
    .await
    .unwrap();

    let result = svc
        .change_column_type(
            receipt(db, OWNER, AccessLevel::Edit),
            viewer(OWNER),
            ChangeColumnType {
                table_id,
                column_id: relation_column.id,
                data_type: DataType::String,
                is_multi_select: false,
                specific_entity_type: None,
                relation: None,
                base_version: TableVersion(2),
            },
        )
        .await;
    assert!(
        matches!(result, Err(DatabaseError::InvalidSchemaOperation(_))),
        "{result:?}"
    );
    let w = world.lock().unwrap();
    assert_eq!(w.tables[0].version, TableVersion(2));
    assert_eq!(
        w.columns
            .iter()
            .find(|column| column.id == relation_column.id)
            .unwrap()
            .property_definition_id,
        relation_column.property_definition_id
    );
}
