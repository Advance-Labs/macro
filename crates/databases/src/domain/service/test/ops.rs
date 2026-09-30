//! Typed ops over the fakes: what each op writes, what refuses a batch, and
//! that a refused batch writes nothing.

use models_databases::{
    CellValue, CellWrite, ColumnKind, DatabaseOp, OpResult, OptionRef, RowChange, RowChanges,
};
use models_properties::shared::EntityReference;

use super::*;
use crate::domain::models::OpRefusal;

fn edit(database_id: DatabaseId) -> EntityAccessReceipt<EditAccessLevel> {
    receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner)
}

fn table_version(world: &Shared, table_id: TableId) -> TableVersion {
    world
        .lock()
        .unwrap()
        .tables
        .iter()
        .find(|table| table.id == table_id)
        .unwrap()
        .version
}

fn option_id(world: &Shared, definition_id: PropertyDefinitionId, label: &str) -> Uuid {
    world.lock().unwrap().definitions[&definition_id]
        .property_options
        .iter()
        .find(|option| option.value == PropertyOptionValue::String(label.into()))
        .unwrap()
        .id
}

fn row_ids(world: &Shared, table_id: TableId) -> Vec<RowId> {
    world.lock().unwrap().rows[&table_id]
        .iter()
        .map(|row| row.id)
        .collect()
}

fn cell(world: &Shared, row: RowId, definition_id: PropertyDefinitionId) -> Option<PropertyValue> {
    world
        .lock()
        .unwrap()
        .cells
        .get(&row)
        .and_then(|cells| cells.get(&definition_id))
        .cloned()
}

#[tokio::test]
async fn an_insert_of_two_rows_mints_them_in_order_with_their_cells() {
    let seeded = seeded().await;
    let going = option_id(
        &seeded.world,
        seeded.status_column.property_definition_id,
        "Going",
    );
    let before = table_version(&seeded.world, seeded.table_id);

    let results = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![DatabaseOp::InsertRows {
                table: seeded.table_id,
                rows: vec![
                    vec![
                        CellWrite {
                            column: seeded.name_column.id,
                            value: CellValue::Text("Alex".into()),
                        },
                        CellWrite {
                            column: seeded.status_column.id,
                            value: CellValue::Options(vec![OptionRef::Id(going)]),
                        },
                    ],
                    vec![
                        CellWrite {
                            column: seeded.name_column.id,
                            value: CellValue::Text("Robin".into()),
                        },
                        CellWrite {
                            column: seeded.plus_ones_column.id,
                            value: CellValue::Number(3.0),
                        },
                    ],
                ],
                create_missing_options: false,
            }],
        )
        .await
        .unwrap();

    let rows = row_ids(&seeded.world, seeded.table_id);
    assert_eq!(rows.len(), 3);
    assert_eq!(
        results,
        vec![OpResult::RowsWritten {
            table_version: TableVersion(before.0 + 1),
            inserted: vec![rows[1], rows[2]],
            affected: 2,
        }]
    );
    assert_eq!(
        cell(
            &seeded.world,
            rows[1],
            seeded.name_column.property_definition_id
        ),
        Some(PropertyValue::Str("Alex".into()))
    );
    assert_eq!(
        cell(
            &seeded.world,
            rows[1],
            seeded.status_column.property_definition_id
        ),
        Some(PropertyValue::SelectOption(vec![going]))
    );
    assert_eq!(
        cell(
            &seeded.world,
            rows[2],
            seeded.name_column.property_definition_id
        ),
        Some(PropertyValue::Str("Robin".into()))
    );
    assert_eq!(
        cell(
            &seeded.world,
            rows[2],
            seeded.plus_ones_column.property_definition_id
        ),
        Some(PropertyValue::Num(3.0))
    );
    assert_eq!(
        cell(
            &seeded.world,
            rows[2],
            seeded.status_column.property_definition_id
        ),
        None
    );
}

#[tokio::test]
async fn a_uniform_update_gives_three_rows_the_same_cells() {
    let seeded = seeded().await;
    seeded
        .service
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: "INSERT INTO guests (name) VALUES ('Alex'), ('Robin')".into(),
                base_versions: None,
            },
        )
        .await
        .unwrap();
    let rows = row_ids(&seeded.world, seeded.table_id);
    let declined = option_id(
        &seeded.world,
        seeded.status_column.property_definition_id,
        "Declined",
    );
    let before = table_version(&seeded.world, seeded.table_id);

    let results = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![DatabaseOp::UpdateRows {
                table: seeded.table_id,
                changes: RowChanges::Uniform {
                    rows: rows.clone(),
                    cells: vec![
                        CellWrite {
                            column: seeded.status_column.id,
                            value: CellValue::Options(vec![OptionRef::Label("declined".into())]),
                        },
                        CellWrite {
                            column: seeded.plus_ones_column.id,
                            value: CellValue::Clear,
                        },
                    ],
                },
                create_missing_options: false,
            }],
        )
        .await
        .unwrap();

    assert_eq!(
        results,
        vec![OpResult::RowsWritten {
            table_version: TableVersion(before.0 + 1),
            inserted: vec![],
            affected: 3,
        }]
    );
    for row in &rows {
        assert_eq!(
            cell(
                &seeded.world,
                *row,
                seeded.status_column.property_definition_id
            ),
            Some(PropertyValue::SelectOption(vec![declined]))
        );
        assert_eq!(
            cell(
                &seeded.world,
                *row,
                seeded.plus_ones_column.property_definition_id
            ),
            None
        );
    }
    assert_eq!(
        cell(
            &seeded.world,
            seeded.row_id,
            seeded.name_column.property_definition_id
        ),
        Some(PropertyValue::Str("Sam".into()))
    );
}

#[tokio::test]
async fn a_per_row_update_gives_each_row_its_own_cells() {
    let seeded = seeded().await;
    seeded
        .service
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: "INSERT INTO guests (name) VALUES ('Alex')".into(),
                base_versions: None,
            },
        )
        .await
        .unwrap();
    let rows = row_ids(&seeded.world, seeded.table_id);

    let results = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![DatabaseOp::UpdateRows {
                table: seeded.table_id,
                changes: RowChanges::PerRow {
                    rows: vec![
                        RowChange {
                            row: rows[0],
                            cells: vec![CellWrite {
                                column: seeded.name_column.id,
                                value: CellValue::Text("Samantha".into()),
                            }],
                        },
                        RowChange {
                            row: rows[1],
                            cells: vec![CellWrite {
                                column: seeded.plus_ones_column.id,
                                value: CellValue::Number(1.0),
                            }],
                        },
                    ],
                },
                create_missing_options: false,
            }],
        )
        .await
        .unwrap();

    assert!(matches!(
        results.as_slice(),
        [OpResult::RowsWritten { affected: 2, .. }]
    ));
    assert_eq!(
        cell(
            &seeded.world,
            rows[0],
            seeded.name_column.property_definition_id
        ),
        Some(PropertyValue::Str("Samantha".into()))
    );
    assert_eq!(
        cell(
            &seeded.world,
            rows[0],
            seeded.plus_ones_column.property_definition_id
        ),
        Some(PropertyValue::Num(2.0))
    );
    assert_eq!(
        cell(
            &seeded.world,
            rows[1],
            seeded.name_column.property_definition_id
        ),
        Some(PropertyValue::Str("Alex".into()))
    );
    assert_eq!(
        cell(
            &seeded.world,
            rows[1],
            seeded.plus_ones_column.property_definition_id
        ),
        Some(PropertyValue::Num(1.0))
    );
}

#[tokio::test]
async fn a_delete_removes_the_rows_and_their_cells() {
    let seeded = seeded().await;
    let before = table_version(&seeded.world, seeded.table_id);

    let results = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![DatabaseOp::DeleteRows {
                table: seeded.table_id,
                rows: vec![seeded.row_id],
            }],
        )
        .await
        .unwrap();

    assert_eq!(
        results,
        vec![OpResult::RowsWritten {
            table_version: TableVersion(before.0 + 1),
            inserted: vec![],
            affected: 1,
        }]
    );
    assert!(row_ids(&seeded.world, seeded.table_id).is_empty());
    assert!(
        !seeded
            .world
            .lock()
            .unwrap()
            .cells
            .contains_key(&seeded.row_id)
    );
}

#[tokio::test]
async fn a_label_the_column_lacks_becomes_an_option_when_the_op_creates_them() {
    let seeded = seeded().await;
    let status = seeded.status_column.property_definition_id;

    seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![DatabaseOp::InsertRows {
                table: seeded.table_id,
                rows: vec![
                    vec![CellWrite {
                        column: seeded.status_column.id,
                        value: CellValue::Options(vec![OptionRef::Label("Maybe".into())]),
                    }],
                    vec![CellWrite {
                        column: seeded.status_column.id,
                        value: CellValue::Options(vec![OptionRef::Label("maybe".into())]),
                    }],
                ],
                create_missing_options: true,
            }],
        )
        .await
        .unwrap();

    let labels: Vec<PropertyOptionValue> = seeded.world.lock().unwrap().definitions[&status]
        .property_options
        .iter()
        .map(|option| option.value.clone())
        .collect();
    assert_eq!(
        labels,
        vec![
            PropertyOptionValue::String("Going".into()),
            PropertyOptionValue::String("Declined".into()),
            PropertyOptionValue::String("Maybe".into()),
        ]
    );
    let maybe = option_id(&seeded.world, status, "Maybe");
    let rows = row_ids(&seeded.world, seeded.table_id);
    assert_eq!(
        cell(&seeded.world, rows[1], status),
        Some(PropertyValue::SelectOption(vec![maybe]))
    );
    assert_eq!(
        cell(&seeded.world, rows[2], status),
        Some(PropertyValue::SelectOption(vec![maybe]))
    );
}

#[tokio::test]
async fn an_unknown_label_is_refused_without_creating_options() {
    let seeded = seeded().await;
    let before = table_version(&seeded.world, seeded.table_id);

    let error = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![DatabaseOp::InsertRows {
                table: seeded.table_id,
                rows: vec![vec![CellWrite {
                    column: seeded.status_column.id,
                    value: CellValue::Options(vec![OptionRef::Label("Maybe".into())]),
                }]],
                create_missing_options: false,
            }],
        )
        .await
        .unwrap_err();

    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    assert_eq!(
        refusal,
        OpRefusal {
            op: 0,
            row: Some(0),
            column: Some(seeded.status_column.id),
            reason: "`Maybe` is not an option of \"Status\"".into(),
        }
    );
    let w = seeded.world.lock().unwrap();
    assert_eq!(
        w.definitions[&seeded.status_column.property_definition_id]
            .property_options
            .len(),
        2
    );
    assert_eq!(w.rows[&seeded.table_id].len(), 1);
    assert_eq!(w.row_write_batches, 0);
    drop(w);
    assert_eq!(table_version(&seeded.world, seeded.table_id), before);
}

#[tokio::test]
async fn an_op_on_another_databases_table_refuses_the_batch_before_anything_is_written() {
    let seeded = seeded().await;
    let other = seeded
        .service
        .create_database(CreateDatabase {
            name: "Elsewhere".into(),
            owner_id: user(OWNER),
            acting_bot: None,
        })
        .await
        .unwrap();
    let other_table = seeded
        .world
        .lock()
        .unwrap()
        .tables
        .iter()
        .find(|table| table.database_id == other.id)
        .unwrap()
        .id;
    let published_before = seeded.world.lock().unwrap().published.len();

    let error = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![
                DatabaseOp::InsertRows {
                    table: seeded.table_id,
                    rows: vec![vec![]],
                    create_missing_options: false,
                },
                DatabaseOp::InsertRows {
                    table: other_table,
                    rows: vec![vec![]],
                    create_missing_options: false,
                },
            ],
        )
        .await
        .unwrap_err();

    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    assert_eq!(
        refusal,
        OpRefusal {
            op: 1,
            row: None,
            column: None,
            reason: format!("table {other_table} is not in this database"),
        }
    );
    let w = seeded.world.lock().unwrap();
    assert_eq!(w.row_write_batches, 0);
    assert_eq!(w.rows[&seeded.table_id].len(), 1);
    assert!(w.rows.get(&other_table).is_none_or(Vec::is_empty));
    assert_eq!(w.published.len(), published_before);
}

#[tokio::test]
async fn a_failing_second_op_leaves_the_first_unapplied() {
    let seeded = seeded().await;
    let status = seeded.status_column.property_definition_id;
    let before = table_version(&seeded.world, seeded.table_id);
    let published_before = seeded.world.lock().unwrap().published.len();
    let ghost = Uuid::now_v7();

    let error = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![
                DatabaseOp::InsertRows {
                    table: seeded.table_id,
                    rows: vec![vec![CellWrite {
                        column: seeded.status_column.id,
                        value: CellValue::Options(vec![OptionRef::Label("Maybe".into())]),
                    }]],
                    create_missing_options: true,
                },
                DatabaseOp::UpdateRows {
                    table: seeded.table_id,
                    changes: RowChanges::PerRow {
                        rows: vec![
                            RowChange {
                                row: seeded.row_id,
                                cells: vec![CellWrite {
                                    column: seeded.name_column.id,
                                    value: CellValue::Text("Samantha".into()),
                                }],
                            },
                            RowChange {
                                row: ghost,
                                cells: vec![CellWrite {
                                    column: seeded.name_column.id,
                                    value: CellValue::Text("Nobody".into()),
                                }],
                            },
                        ],
                    },
                    create_missing_options: false,
                },
            ],
        )
        .await
        .unwrap_err();

    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    assert_eq!(
        refusal,
        OpRefusal {
            op: 1,
            row: Some(1),
            column: None,
            reason: format!("no row {ghost} in this table"),
        }
    );
    assert_eq!(row_ids(&seeded.world, seeded.table_id), vec![seeded.row_id]);
    assert_eq!(
        cell(
            &seeded.world,
            seeded.row_id,
            seeded.name_column.property_definition_id
        ),
        Some(PropertyValue::Str("Sam".into()))
    );
    assert_eq!(
        seeded.world.lock().unwrap().definitions[&status]
            .property_options
            .len(),
        2
    );
    assert_eq!(table_version(&seeded.world, seeded.table_id), before);
    assert_eq!(
        seeded.world.lock().unwrap().published.len(),
        published_before
    );
}

#[tokio::test]
async fn a_value_that_does_not_fit_its_column_names_the_op_row_and_column() {
    let seeded = seeded().await;

    let error = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![DatabaseOp::UpdateRows {
                table: seeded.table_id,
                changes: RowChanges::PerRow {
                    rows: vec![RowChange {
                        row: seeded.row_id,
                        cells: vec![CellWrite {
                            column: seeded.plus_ones_column.id,
                            value: CellValue::Text("two".into()),
                        }],
                    }],
                },
                create_missing_options: false,
            }],
        )
        .await
        .unwrap_err();

    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    assert_eq!(
        refusal,
        OpRefusal {
            op: 0,
            row: Some(0),
            column: Some(seeded.plus_ones_column.id),
            reason: "\"Plus ones\" is a number column; text does not fit it".into(),
        }
    );
}

#[tokio::test]
async fn a_relation_cell_names_rows_of_its_target_table() {
    let seeded = seeded().await;
    let sessions = seeded
        .service
        .create_table(
            edit(seeded.database_id),
            CreateTable {
                database_id: seeded.database_id,
                name: "Sessions".into(),
            },
        )
        .await
        .unwrap();
    let relation = seeded
        .service
        .create_column(
            edit(seeded.database_id),
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
    let relation_definition = seeded
        .world
        .lock()
        .unwrap()
        .columns
        .iter()
        .find(|column| column.id == relation)
        .unwrap()
        .property_definition_id;
    let keynote = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![DatabaseOp::InsertRows {
                table: sessions.id,
                rows: vec![vec![]],
                create_missing_options: false,
            }],
        )
        .await
        .unwrap();
    let [OpResult::RowsWritten { inserted, .. }] = keynote.as_slice() else {
        panic!("expected one insert, got {keynote:?}");
    };
    let keynote = inserted[0];

    seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![DatabaseOp::UpdateRows {
                table: seeded.table_id,
                changes: RowChanges::Uniform {
                    rows: vec![seeded.row_id],
                    cells: vec![CellWrite {
                        column: relation,
                        value: CellValue::Rows(vec![keynote]),
                    }],
                },
                create_missing_options: false,
            }],
        )
        .await
        .unwrap();
    assert_eq!(
        cell(&seeded.world, seeded.row_id, relation_definition),
        Some(PropertyValue::EntityRef(vec![EntityReference {
            entity_id: keynote.to_string(),
            entity_type: PropertyEntityType::DatabaseRow,
            specific_message_id: None,
        }]))
    );

    let error = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![DatabaseOp::UpdateRows {
                table: seeded.table_id,
                changes: RowChanges::Uniform {
                    rows: vec![seeded.row_id],
                    cells: vec![CellWrite {
                        column: relation,
                        value: CellValue::Rows(vec![seeded.row_id]),
                    }],
                },
                create_missing_options: false,
            }],
        )
        .await
        .unwrap_err();
    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    assert_eq!(
        refusal,
        OpRefusal {
            op: 0,
            row: None,
            column: Some(relation),
            reason: format!("row {} is not a row of the related table", seeded.row_id),
        }
    );
    assert_eq!(
        cell(&seeded.world, seeded.row_id, relation_definition),
        Some(PropertyValue::EntityRef(vec![EntityReference {
            entity_id: keynote.to_string(),
            entity_type: PropertyEntityType::DatabaseRow,
            specific_message_id: None,
        }]))
    );
}

#[tokio::test]
async fn a_type_change_through_ops_matches_change_column_type() {
    let through_ops = seeded().await;
    let direct = seeded().await;
    for seeded in [&through_ops, &direct] {
        seeded
            .service
            .exec_sql(
                viewer(OWNER),
                ExecRequest {
                    scope: None,
                    sql: "INSERT INTO guests (name) VALUES ('12')".into(),
                    base_versions: None,
                },
            )
            .await
            .unwrap();
    }

    let results = through_ops
        .service
        .apply_ops(
            edit(through_ops.database_id),
            viewer(OWNER),
            vec![DatabaseOp::ChangeColumnType {
                table: through_ops.table_id,
                column: through_ops.name_column.id,
                to: ColumnKind::Number,
                clear_invalid: true,
            }],
        )
        .await
        .unwrap();
    let changed = direct
        .service
        .change_column_type(
            edit(direct.database_id),
            viewer(OWNER),
            ChangeColumnType {
                table_id: direct.table_id,
                column_id: direct.name_column.id,
                data_type: DataType::Number,
                is_multi_select: false,
                specific_entity_type: None,
                relation: None,
                base_version: table_version(&direct.world, direct.table_id),
                clear_invalid: true,
            },
        )
        .await
        .unwrap();

    assert_eq!(
        results,
        vec![OpResult::ColumnTyped {
            table_version: table_version(&through_ops.world, through_ops.table_id),
            cleared_cells: 1,
            trimmed_cells: 0,
        }]
    );
    assert_eq!(changed.cleared_cells, 1);
    assert_eq!(changed.trimmed_cells, 0);
    let numbers = |seeded: &Seeded| -> Vec<Option<PropertyValue>> {
        let definition = seeded
            .world
            .lock()
            .unwrap()
            .columns
            .iter()
            .find(|column| column.id == seeded.name_column.id)
            .unwrap()
            .property_definition_id;
        row_ids(&seeded.world, seeded.table_id)
            .into_iter()
            .map(|row| cell(&seeded.world, row, definition))
            .collect()
    };
    assert_eq!(
        numbers(&through_ops),
        vec![None, Some(PropertyValue::Num(12.0))]
    );
    assert_eq!(numbers(&through_ops), numbers(&direct));
}

#[tokio::test]
async fn a_type_change_is_sent_on_its_own() {
    let seeded = seeded().await;

    let error = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![
                DatabaseOp::InsertRows {
                    table: seeded.table_id,
                    rows: vec![vec![]],
                    create_missing_options: false,
                },
                DatabaseOp::ChangeColumnType {
                    table: seeded.table_id,
                    column: seeded.name_column.id,
                    to: ColumnKind::Number,
                    clear_invalid: false,
                },
            ],
        )
        .await
        .unwrap_err();

    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    assert_eq!(
        refusal,
        OpRefusal {
            op: 1,
            row: None,
            column: Some(seeded.name_column.id),
            reason: "a column type change is applied on its own; send it as the only op of its \
                     request"
                .into(),
        }
    );
    assert_eq!(seeded.world.lock().unwrap().row_write_batches, 0);
}

#[tokio::test]
async fn a_batch_bumps_each_table_once_and_announces_it_once() {
    let seeded = seeded().await;
    let before = table_version(&seeded.world, seeded.table_id);
    let published_before = seeded.world.lock().unwrap().published.len();
    let events_before = seeded.world.lock().unwrap().broker_events.len();

    let results = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![
                DatabaseOp::InsertRows {
                    table: seeded.table_id,
                    rows: vec![vec![CellWrite {
                        column: seeded.name_column.id,
                        value: CellValue::Text("Alex".into()),
                    }]],
                    create_missing_options: false,
                },
                DatabaseOp::UpdateRows {
                    table: seeded.table_id,
                    changes: RowChanges::Uniform {
                        rows: vec![seeded.row_id],
                        cells: vec![CellWrite {
                            column: seeded.plus_ones_column.id,
                            value: CellValue::Number(0.0),
                        }],
                    },
                    create_missing_options: false,
                },
                DatabaseOp::DeleteRows {
                    table: seeded.table_id,
                    rows: vec![seeded.row_id],
                },
            ],
        )
        .await
        .unwrap();

    let after = TableVersion(before.0 + 1);
    assert_eq!(table_version(&seeded.world, seeded.table_id), after);
    assert!(results.iter().all(|result| matches!(
        result,
        OpResult::RowsWritten { table_version, .. } if *table_version == after
    )));
    let w = seeded.world.lock().unwrap();
    assert_eq!(w.row_write_batches, 1);
    assert_eq!(
        &w.published[published_before..],
        &[(seeded.table_id, after)]
    );
    let events = &w.broker_events[events_before..];
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0]["event_type"], "database.tables_changed");
    assert_eq!(events[0]["metadata"]["attribution"]["actor"], OWNER);
    assert_eq!(
        events[0]["metadata"]["tables"],
        serde_json::json!([{"table_id": seeded.table_id, "version": after.0}])
    );
}
