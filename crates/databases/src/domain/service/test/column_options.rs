//! A select column's options are explicit schema: added, parsed and
//! validated before any cell may hold them.

use super::*;

#[tokio::test]
async fn a_select_column_with_no_options_accepts_nothing() {
    let seeded = seeded().await;
    let (svc, database_id, table_id) = (seeded.service, seeded.database_id, seeded.table_id);
    let stage = svc
        .create_column(
            receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "Stage".into(),
                    data_type: DataType::SelectString,
                    is_multi_select: false,
                    options: vec![],
                },
                config: None,
            },
        )
        .await
        .unwrap();

    let error = svc
        .apply_ops(
            receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            vec![DatabaseOp::InsertRows {
                table: table_id,
                rows: vec![vec![CellWrite {
                    column: stage,
                    value: CellValue::Options(vec![OptionRef::Label("Main".into())]),
                }]],
                create_missing_options: false,
            }],
        )
        .await
        .unwrap_err();
    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    assert_eq!(refusal.reason, "`Main` is not an option of \"Stage\"");
}

/// The point of the operation: the write that failed succeeds once the option
/// exists, and the table's version moves because its schema did.
#[tokio::test]
async fn add_column_options_extends_what_ops_accept_and_bumps_the_version() {
    let seeded = seeded().await;
    let (world, svc, db, guests, status) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.status_column.id,
    );
    let published_before = world.lock().unwrap().published.len();

    let column = svc
        .add_column_options(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            AddColumnOptions {
                table_id: guests,
                column_id: status,
                labels: vec!["Waitlisted".into()],
            },
        )
        .await
        .expect("edit access may extend a select column");

    assert_eq!(column.sql_name, "\"Status\"");
    assert_eq!(
        catalog::option_labels(&column.definition)
            .into_iter()
            .map(|(_, label)| label)
            .collect::<Vec<_>>(),
        vec!["Going", "Declined", "Waitlisted"],
        "new options are appended, so existing labels do not move"
    );

    {
        let w = world.lock().unwrap();
        assert_eq!(w.tables[0].version, TableVersion(2));
        assert_eq!(
            w.published.len(),
            published_before + 1,
            "the schema change is announced for liveness"
        );
    }

    let inserted = svc
        .apply_ops(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            vec![DatabaseOp::InsertRows {
                table: guests,
                rows: vec![vec![CellWrite {
                    column: status,
                    value: CellValue::Options(vec![OptionRef::Label("Waitlisted".into())]),
                }]],
                create_missing_options: false,
            }],
        )
        .await
        .expect("the option now resolves");
    let [OpResult::RowsWritten { inserted, .. }] = inserted.as_slice() else {
        panic!("expected one insert, got {inserted:?}");
    };
    let waitlisted = option_id(&world, column.definition.definition.id, "Waitlisted");
    assert_eq!(
        cell(&world, inserted[0], column.definition.definition.id),
        Some(PropertyValue::SelectOption(vec![waitlisted]))
    );
}

/// Re-sending a label the column already has changes nothing: no duplicate
/// option, no version bump, no event — and no error either.
#[tokio::test]
async fn adding_an_existing_option_is_a_no_op() {
    let seeded = seeded().await;
    let (world, svc, db, guests, status) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.status_column.id,
    );
    let published_before = world.lock().unwrap().published.len();

    let column = svc
        .add_column_options(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            AddColumnOptions {
                table_id: guests,
                column_id: status,
                labels: vec!["going".into(), "  Declined  ".into()],
            },
        )
        .await
        .expect("an option that is already there is not an error");

    assert_eq!(column.definition.property_options.len(), 2);
    let w = world.lock().unwrap();
    assert_eq!(w.tables[0].version, TableVersion(1));
    assert_eq!(w.published.len(), published_before);
}

#[tokio::test]
async fn options_are_refused_on_a_column_that_cannot_hold_them() {
    let seeded = seeded().await;
    let (svc, db, guests, name_column) = (
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.name_column.id,
    );

    let err = svc
        .create_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: guests,
                binding: ColumnBinding::NewDefinition {
                    name: "Notes".into(),
                    data_type: DataType::String,
                    is_multi_select: false,
                    options: vec!["Main".into()],
                },
                config: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            DatabaseError::InvalidSchemaOperation(SchemaError::OptionsOnPlainColumn)
        ),
        "{err:?}"
    );

    // …and the same on the standalone operation, against the text column the
    // seeded table already has.
    let err = svc
        .add_column_options(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            AddColumnOptions {
                table_id: guests,
                column_id: name_column,
                labels: vec!["Main".into()],
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            DatabaseError::InvalidSchemaOperation(SchemaError::ColumnTakesNoOptions)
        ),
        "{err:?}"
    );
}

/// A numeric select stores numbers, so its labels have to be numbers, and
/// labels naming the same number are one option.
#[tokio::test]
async fn numeric_select_options_are_parsed_as_numbers() {
    let seeded = seeded().await;
    let (world, svc, db, guests) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
    );

    let err = svc
        .create_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: guests,
                binding: ColumnBinding::NewDefinition {
                    name: "Priority".into(),
                    data_type: DataType::SelectNumber,
                    is_multi_select: false,
                    options: vec!["soon".into()],
                },
                config: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, DatabaseError::InvalidSchemaOperation(SchemaError::OptionNotNumber { label }) if label == "soon"),
        "{err:?}"
    );

    let priority = svc
        .create_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: guests,
                binding: ColumnBinding::NewDefinition {
                    name: "Priority".into(),
                    data_type: DataType::SelectNumber,
                    is_multi_select: false,
                    options: vec!["1".into(), "2.0".into(), "2".into()],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    let w = world.lock().unwrap();
    let definition = w
        .columns
        .iter()
        .find(|column| column.id == priority)
        .unwrap()
        .property_definition_id;
    assert_eq!(
        w.definitions[&definition]
            .property_options
            .iter()
            .map(|option| option.value.clone())
            .collect::<Vec<_>>(),
        vec![
            PropertyOptionValue::Number(1.0),
            PropertyOptionValue::Number(2.0)
        ]
    );
}

#[tokio::test]
async fn option_labels_are_validated() {
    let seeded = seeded().await;
    let (svc, db, guests) = (seeded.service, seeded.database_id, seeded.table_id);

    for bad in ["   ", ""] {
        let err = svc
            .create_column(
                receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
                viewer(OWNER),
                CreateColumn {
                    infer_type: false,
                    table_id: guests,
                    binding: ColumnBinding::NewDefinition {
                        name: "Stage".into(),
                        data_type: DataType::SelectString,
                        is_multi_select: false,
                        options: vec![bad.into()],
                    },
                    config: None,
                },
            )
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                DatabaseError::InvalidSchemaOperation(SchemaError::EmptyOptionLabel)
            ),
            "{err:?}"
        );
    }

    let err = svc
        .create_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: guests,
                binding: ColumnBinding::NewDefinition {
                    name: "Stage".into(),
                    data_type: DataType::SelectString,
                    is_multi_select: false,
                    options: vec!["x".repeat(MAX_OPTION_LABEL_LEN + 1)],
                },
                config: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            DatabaseError::InvalidSchemaOperation(SchemaError::OptionLabelTooLong {
                max: MAX_OPTION_LABEL_LEN
            })
        ),
        "{err:?}"
    );
}

#[tokio::test]
async fn add_column_options_respects_receipts() {
    let seeded = seeded().await;
    let (world, svc, db, guests, status) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.status_column.id,
    );
    let elsewhere = Uuid::new_v4();

    let err = svc
        .add_column_options(
            receipt::<EditAccessLevel>(elsewhere, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            AddColumnOptions {
                table_id: guests,
                column_id: status,
                labels: vec!["Waitlisted".into()],
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, DatabaseError::NotFound),
        "a receipt for another database reaches nothing: {err:?}"
    );

    let err = svc
        .add_column_options(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            AddColumnOptions {
                table_id: guests,
                column_id: Uuid::new_v4(),
                labels: vec!["Waitlisted".into()],
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound), "{err:?}");

    // Nothing was written on the way to either refusal.
    assert_eq!(world.lock().unwrap().definitions.len(), 3);
}
