//! Relation columns: a cell naming rows of the table the column points at.

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
    let title = seeded
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
        .apply_ops(
            receipt::<EditAccessLevel>(seeded.database_id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            vec![DatabaseOp::InsertRows {
                table: sessions.id,
                rows: vec![vec![CellWrite {
                    column: title,
                    value: CellValue::Text("Keynote".into()),
                }]],
                create_missing_options: false,
            }],
        )
        .await
        .unwrap();
    let [OpResult::RowsWritten { inserted, .. }] = keynote.as_slice() else {
        panic!("expected one insert, got {keynote:?}");
    };
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
        keynote_row: inserted[0],
        relation_column,
    }
}

#[tokio::test]
async fn a_relation_write_moves_only_the_table_holding_the_cell() {
    let Linked {
        seeded,
        sessions_table,
        keynote_row,
        relation_column,
    } = linked().await;
    let (world, svc, db, table_id, row_id) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.row_id,
    );
    let sessions_version = table_version(&world, sessions_table);

    let written = svc
        .apply_ops(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            vec![DatabaseOp::UpdateRows {
                table: table_id,
                changes: RowChanges::Uniform {
                    rows: vec![row_id],
                    cells: vec![CellWrite {
                        column: relation_column.id,
                        value: CellValue::Rows(vec![keynote_row]),
                    }],
                },
                create_missing_options: false,
            }],
        )
        .await
        .unwrap();

    assert_eq!(
        written,
        vec![OpResult::RowsWritten {
            table_version: TableVersion(2),
            inserted: vec![],
            affected: 1,
        }]
    );
    assert_eq!(table_version(&world, sessions_table), sessions_version);
    assert_eq!(
        cell(&world, row_id, relation_column.property_definition_id),
        Some(PropertyValue::EntityRef(vec![
            models_properties::EntityReference {
                entity_id: keynote_row.to_string(),
                entity_type: PropertyEntityType::DatabaseRow,
                specific_message_id: None,
            }
        ]))
    );
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
    svc.apply_ops(
        receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
        viewer(OWNER),
        vec![DatabaseOp::UpdateRows {
            table: table_id,
            changes: RowChanges::Uniform {
                rows: vec![row_id],
                cells: vec![CellWrite {
                    column: relation_column.id,
                    value: CellValue::Rows(vec![keynote_row]),
                }],
            },
            create_missing_options: false,
        }],
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
                clear_invalid: false,
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
