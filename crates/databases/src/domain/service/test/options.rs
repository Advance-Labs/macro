//! Option ops over the fakes: relabelling, recolouring and removing a select
//! column's options, what each one checks, and who may change an option of a
//! property shared beyond the database.

use models_databases::OptionColor;

use super::*;

/// The options of a definition as `(id, value, colour)`, in display order.
fn options(
    world: &Shared,
    definition_id: PropertyDefinitionId,
) -> Vec<(Uuid, PropertyOptionValue, Option<String>)> {
    let mut options = world.lock().unwrap().definitions[&definition_id]
        .property_options
        .clone();
    options.sort_by_key(|option| option.display_order);
    options
        .into_iter()
        .map(|option| (option.id, option.value, option.color))
        .collect()
}

fn refusal(error: DatabaseError) -> OpRefusal {
    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    refusal
}

#[tokio::test]
async fn relabelling_an_option_keeps_every_cell_that_holds_it() {
    let seeded = seeded().await;
    let status = seeded.status_column.property_definition_id;
    let going = option_id(&seeded.world, status, "Going");
    let declined = option_id(&seeded.world, status, "Declined");
    let before = table_version(&seeded.world, seeded.table_id);
    seeded.world.lock().unwrap().published.clear();

    let results = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![DatabaseOp::UpdateOption {
                table: seeded.table_id,
                column: seeded.status_column.id,
                option: going,
                label: Some("  Attending ".into()),
                color: None,
            }],
        )
        .await
        .unwrap();

    assert_eq!(
        results,
        vec![OpResult::OptionChanged {
            table_version: TableVersion(before.0 + 1),
        }]
    );
    assert_eq!(
        options(&seeded.world, status),
        vec![
            (going, PropertyOptionValue::String("Attending".into()), None),
            (
                declined,
                PropertyOptionValue::String("Declined".into()),
                None
            ),
        ]
    );
    assert_eq!(
        cell(&seeded.world, seeded.row_id, status),
        Some(PropertyValue::SelectOption(vec![going]))
    );
    assert_eq!(
        seeded.world.lock().unwrap().published,
        vec![(seeded.table_id, TableVersion(before.0 + 1))]
    );
}

#[tokio::test]
async fn an_option_takes_a_palette_colour_and_loses_it_again() {
    let seeded = seeded().await;
    let status = seeded.status_column.property_definition_id;
    let going = option_id(&seeded.world, status, "Going");
    let recolour = |color| DatabaseOp::UpdateOption {
        table: seeded.table_id,
        column: seeded.status_column.id,
        option: going,
        label: None,
        color,
    };

    seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![recolour(Some(Some(OptionColor::Teal)))],
        )
        .await
        .unwrap();
    assert_eq!(
        options(&seeded.world, status)[0],
        (
            going,
            PropertyOptionValue::String("Going".into()),
            Some("#12A594".into())
        )
    );

    seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![recolour(Some(None))],
        )
        .await
        .unwrap();
    assert_eq!(
        options(&seeded.world, status)[0],
        (going, PropertyOptionValue::String("Going".into()), None)
    );
}

#[tokio::test]
async fn a_tag_option_keeps_a_colour() {
    let seeded = seeded().await;
    let labels = seeded
        .service
        .create_column(
            edit(seeded.database_id),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: seeded.table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "Labels".into(),
                    data_type: DataType::Tag,
                    is_multi_select: true,
                    options: vec!["VIP".into()],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    let definition = seeded
        .world
        .lock()
        .unwrap()
        .columns
        .iter()
        .find(|column| column.id == labels)
        .unwrap()
        .property_definition_id;
    let vip = option_id(&seeded.world, definition, "VIP");

    let error = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![DatabaseOp::UpdateOption {
                table: seeded.table_id,
                column: labels,
                option: vip,
                label: None,
                color: Some(None),
            }],
        )
        .await
        .unwrap_err();

    assert_eq!(
        refusal(error),
        OpRefusal {
            op: 0,
            row: None,
            column: Some(labels),
            reason: "a tag option always has a colour; pick another instead".into(),
        }
    );
}

#[tokio::test]
async fn a_label_is_refused_when_empty_or_already_taken_ignoring_case() {
    let seeded = seeded().await;
    let status = seeded.status_column.property_definition_id;
    let going = option_id(&seeded.world, status, "Going");
    let relabel = |label: &str| DatabaseOp::UpdateOption {
        table: seeded.table_id,
        column: seeded.status_column.id,
        option: going,
        label: Some(label.into()),
        color: None,
    };

    let cases = [
        ("   ", "an option label must not be empty"),
        ("declined", "`declined` is already an option of \"Status\""),
    ];
    for (label, reason) in cases {
        let error = seeded
            .service
            .apply_ops(
                edit(seeded.database_id),
                viewer(OWNER),
                vec![relabel(label)],
            )
            .await
            .unwrap_err();
        assert_eq!(
            refusal(error),
            OpRefusal {
                op: 0,
                row: None,
                column: Some(seeded.status_column.id),
                reason: reason.into(),
            },
            "{label:?}"
        );
    }

    seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![relabel("GOING")],
        )
        .await
        .unwrap();
    assert_eq!(
        options(&seeded.world, status)[0].1,
        PropertyOptionValue::String("GOING".into())
    );
}

#[tokio::test]
async fn a_numeric_options_label_must_be_a_number() {
    let seeded = seeded().await;
    let size = seeded
        .service
        .create_column(
            edit(seeded.database_id),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: seeded.table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "Size".into(),
                    data_type: DataType::SelectNumber,
                    is_multi_select: false,
                    options: vec!["1".into(), "2".into()],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    let definition = seeded
        .world
        .lock()
        .unwrap()
        .columns
        .iter()
        .find(|column| column.id == size)
        .unwrap()
        .property_definition_id;
    let one = options(&seeded.world, definition)[0].0;
    let relabel = |label: &str| DatabaseOp::UpdateOption {
        table: seeded.table_id,
        column: size,
        option: one,
        label: Some(label.into()),
        color: None,
    };

    let cases = [
        (
            "soon",
            "`soon` is not a number; the options of a numeric select column must be numbers",
        ),
        ("2.0", "`2.0` is already an option of \"Size\""),
    ];
    for (label, reason) in cases {
        let error = seeded
            .service
            .apply_ops(
                edit(seeded.database_id),
                viewer(OWNER),
                vec![relabel(label)],
            )
            .await
            .unwrap_err();
        assert_eq!(
            refusal(error),
            OpRefusal {
                op: 0,
                row: None,
                column: Some(size),
                reason: reason.into(),
            },
            "{label:?}"
        );
    }

    seeded
        .service
        .apply_ops(edit(seeded.database_id), viewer(OWNER), vec![relabel("3")])
        .await
        .unwrap();
    assert_eq!(
        options(&seeded.world, definition)[0].1,
        PropertyOptionValue::Number(3.0)
    );
}

#[tokio::test]
async fn an_option_the_column_lacks_is_refused() {
    let seeded = seeded().await;
    let stale = Uuid::from_u128(0x57a1e);

    for op in [
        DatabaseOp::UpdateOption {
            table: seeded.table_id,
            column: seeded.status_column.id,
            option: stale,
            label: Some("Maybe".into()),
            color: None,
        },
        DatabaseOp::DeleteOption {
            table: seeded.table_id,
            column: seeded.status_column.id,
            option: stale,
        },
    ] {
        let error = seeded
            .service
            .apply_ops(edit(seeded.database_id), viewer(OWNER), vec![op])
            .await
            .unwrap_err();
        assert_eq!(
            refusal(error),
            OpRefusal {
                op: 0,
                row: None,
                column: Some(seeded.status_column.id),
                reason: format!("no option {stale} on \"Status\""),
            }
        );
    }
    assert_eq!(seeded.world.lock().unwrap().write_batches, 0);
}

#[tokio::test]
async fn a_column_without_options_has_none_to_change() {
    let seeded = seeded().await;

    let error = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![DatabaseOp::DeleteOption {
                table: seeded.table_id,
                column: seeded.name_column.id,
                option: Uuid::from_u128(1),
            }],
        )
        .await
        .unwrap_err();

    assert_eq!(
        refusal(error),
        OpRefusal {
            op: 0,
            row: None,
            column: Some(seeded.name_column.id),
            reason: "\"Name\" is a text column; only select and tag columns have options".into(),
        }
    );
}

#[tokio::test]
async fn removing_an_option_empties_single_select_cells_and_trims_multi_select_ones() {
    let seeded = seeded().await;
    let status = seeded.status_column.property_definition_id;
    let going = option_id(&seeded.world, status, "Going");
    let declined = option_id(&seeded.world, status, "Declined");
    let diet = seeded
        .service
        .create_column(
            edit(seeded.database_id),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: seeded.table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "Diet".into(),
                    data_type: DataType::SelectString,
                    is_multi_select: true,
                    options: vec!["Vegan".into(), "Halal".into()],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    let diet_definition = seeded
        .world
        .lock()
        .unwrap()
        .columns
        .iter()
        .find(|column| column.id == diet)
        .unwrap()
        .property_definition_id;
    let vegan = option_id(&seeded.world, diet_definition, "Vegan");
    let halal = option_id(&seeded.world, diet_definition, "Halal");
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
                        column: diet,
                        value: CellValue::Options(vec![OptionRef::Id(vegan), OptionRef::Id(halal)]),
                    }],
                },
                create_missing_options: false,
            }],
        )
        .await
        .unwrap();
    let before = table_version(&seeded.world, seeded.table_id);

    let results = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![
                DatabaseOp::DeleteOption {
                    table: seeded.table_id,
                    column: seeded.status_column.id,
                    option: going,
                },
                DatabaseOp::DeleteOption {
                    table: seeded.table_id,
                    column: diet,
                    option: vegan,
                },
            ],
        )
        .await
        .unwrap();

    assert_eq!(
        results,
        vec![
            OpResult::OptionChanged {
                table_version: TableVersion(before.0 + 1),
            },
            OpResult::OptionChanged {
                table_version: TableVersion(before.0 + 1),
            },
        ]
    );
    assert_eq!(
        options(&seeded.world, status),
        vec![(
            declined,
            PropertyOptionValue::String("Declined".into()),
            None
        )]
    );
    assert_eq!(cell(&seeded.world, seeded.row_id, status), None);
    assert_eq!(
        cell(&seeded.world, seeded.row_id, diet_definition),
        Some(PropertyValue::SelectOption(vec![halal]))
    );
}

#[tokio::test]
async fn a_later_op_sees_the_options_an_earlier_one_changed() {
    let seeded = seeded().await;
    let status = seeded.status_column.property_definition_id;
    let going = option_id(&seeded.world, status, "Going");
    let declined = option_id(&seeded.world, status, "Declined");

    seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![
                DatabaseOp::UpdateOption {
                    table: seeded.table_id,
                    column: seeded.status_column.id,
                    option: going,
                    label: Some("Attending".into()),
                    color: None,
                },
                DatabaseOp::UpdateRows {
                    table: seeded.table_id,
                    changes: RowChanges::Uniform {
                        rows: vec![seeded.row_id],
                        cells: vec![CellWrite {
                            column: seeded.status_column.id,
                            value: CellValue::Options(vec![OptionRef::Label("attending".into())]),
                        }],
                    },
                    create_missing_options: false,
                },
            ],
        )
        .await
        .unwrap();
    assert_eq!(
        cell(&seeded.world, seeded.row_id, status),
        Some(PropertyValue::SelectOption(vec![going]))
    );

    let error = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![
                DatabaseOp::DeleteOption {
                    table: seeded.table_id,
                    column: seeded.status_column.id,
                    option: declined,
                },
                DatabaseOp::UpdateRows {
                    table: seeded.table_id,
                    changes: RowChanges::Uniform {
                        rows: vec![seeded.row_id],
                        cells: vec![CellWrite {
                            column: seeded.status_column.id,
                            value: CellValue::Options(vec![OptionRef::Id(declined)]),
                        }],
                    },
                    create_missing_options: false,
                },
            ],
        )
        .await
        .unwrap_err();
    assert_eq!(
        refusal(error),
        OpRefusal {
            op: 1,
            row: None,
            column: Some(seeded.status_column.id),
            reason: format!("no option {declined} on \"Status\""),
        }
    );
    assert_eq!(
        options(&seeded.world, status),
        vec![
            (going, PropertyOptionValue::String("Attending".into()), None),
            (
                declined,
                PropertyOptionValue::String("Declined".into()),
                None
            ),
        ]
    );
}

#[tokio::test]
async fn an_option_change_reaches_every_table_of_the_database_binding_it() {
    let seeded = seeded().await;
    let status = seeded.status_column.property_definition_id;
    let going = option_id(&seeded.world, status, "Going");
    let rsvps = seeded
        .service
        .create_table(
            edit(seeded.database_id),
            CreateTable {
                database_id: seeded.database_id,
                name: "RSVPs".into(),
            },
        )
        .await
        .unwrap();
    seeded
        .service
        .create_column(
            edit(seeded.database_id),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: rsvps.id,
                binding: ColumnBinding::ExistingDefinition(status),
                config: None,
            },
        )
        .await
        .unwrap();
    let guests_before = table_version(&seeded.world, seeded.table_id);
    let rsvps_before = table_version(&seeded.world, rsvps.id);
    seeded.world.lock().unwrap().published.clear();

    seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![DatabaseOp::UpdateOption {
                table: seeded.table_id,
                column: seeded.status_column.id,
                option: going,
                label: Some("Attending".into()),
                color: None,
            }],
        )
        .await
        .unwrap();

    let mut published = seeded.world.lock().unwrap().published.clone();
    published.sort();
    let mut expected = vec![
        (seeded.table_id, TableVersion(guests_before.0 + 1)),
        (rsvps.id, TableVersion(rsvps_before.0 + 1)),
    ];
    expected.sort();
    assert_eq!(published, expected);
}

/// The seeded table with a column bound to a property OWNER owns outside
/// the database, `Priority (High|Low)`, answering the column.
async fn shared_priority_column(seeded: &Seeded) -> Column {
    let mut priority = definition(
        "Priority",
        DataType::SelectString,
        false,
        PropertyOwner::User {
            user_id: OWNER.into(),
        },
    );
    for (display_order, label) in ["High", "Low"].into_iter().enumerate() {
        priority.property_options.push(PropertyOption {
            id: Uuid::new_v4(),
            property_definition_id: priority.definition.id,
            display_order: display_order as i32,
            value: PropertyOptionValue::String(label.into()),
            color: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        });
    }
    let definition_id = priority.definition.id;
    seeded
        .world
        .lock()
        .unwrap()
        .definitions
        .insert(definition_id, priority);
    let column = seeded
        .service
        .create_column(
            edit(seeded.database_id),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: seeded.table_id,
                binding: ColumnBinding::ExistingDefinition(definition_id),
                config: None,
            },
        )
        .await
        .unwrap();
    seeded
        .world
        .lock()
        .unwrap()
        .columns
        .iter()
        .find(|placement| placement.id == column)
        .unwrap()
        .clone()
}

#[tokio::test]
async fn a_shared_propertys_options_take_the_right_to_edit_that_property() {
    let seeded = seeded().await;
    let priority = shared_priority_column(&seeded).await;
    let high = option_id(&seeded.world, priority.property_definition_id, "High");
    let relabel = DatabaseOp::UpdateOption {
        table: seeded.table_id,
        column: priority.id,
        option: high,
        label: Some("Urgent".into()),
        color: None,
    };

    let error = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![relabel.clone()],
        )
        .await
        .unwrap_err();
    assert_eq!(
        refusal(error),
        OpRefusal {
            op: 0,
            row: None,
            column: Some(priority.id),
            reason: "\"Priority\" is a property shared beyond this database, and you may not \
                     change its options"
                .into(),
        }
    );
    assert_eq!(seeded.world.lock().unwrap().write_batches, 0);

    seeded
        .world
        .lock()
        .unwrap()
        .editable_definitions
        .insert(OWNER.into(), vec![priority.property_definition_id]);
    seeded
        .service
        .apply_ops(edit(seeded.database_id), viewer(OWNER), vec![relabel])
        .await
        .unwrap();
    assert_eq!(
        options(&seeded.world, priority.property_definition_id)[0].1,
        PropertyOptionValue::String("Urgent".into())
    );
}

#[tokio::test]
async fn a_column_says_whether_its_property_is_shared_beyond_the_database() {
    let seeded = seeded().await;
    let priority = shared_priority_column(&seeded).await;

    let detail = seeded
        .service
        .get_database(
            receipt::<ViewAccessLevel>(seeded.database_id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
        )
        .await
        .unwrap();

    let shared: Vec<(ColumnId, bool)> = detail.tables[0]
        .columns
        .iter()
        .map(|column| (column.column.id, column.shared_outside_database))
        .collect();
    assert_eq!(
        shared,
        vec![
            (seeded.name_column.id, false),
            (seeded.status_column.id, false),
            (seeded.plus_ones_column.id, false),
            (priority.id, true),
        ]
    );
}

const SHARED_REFUSAL: &str =
    "\"Priority\" is a property shared beyond this database, and you may not change its options";

/// Creating a missing option adds it to the shared property everywhere it
/// is used, so an insert that creates one takes the same right as editing
/// an option.
#[tokio::test]
async fn an_insert_creating_an_option_of_a_shared_property_takes_the_right_to_edit_it() {
    let seeded = seeded().await;
    let priority = shared_priority_column(&seeded).await;
    let insert = DatabaseOp::InsertRows {
        table: seeded.table_id,
        rows: vec![vec![CellWrite {
            column: priority.id,
            value: CellValue::Options(vec![OptionRef::Label("Someday".into())]),
        }]],
        create_missing_options: true,
    };

    let error = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![insert.clone()],
        )
        .await
        .unwrap_err();
    assert_eq!(
        refusal(error),
        OpRefusal {
            op: 0,
            row: Some(0),
            column: Some(priority.id),
            reason: SHARED_REFUSAL.into(),
        }
    );
    assert_eq!(seeded.world.lock().unwrap().write_batches, 0);

    seeded
        .world
        .lock()
        .unwrap()
        .editable_definitions
        .insert(OWNER.into(), vec![priority.property_definition_id]);
    seeded
        .service
        .apply_ops(edit(seeded.database_id), viewer(OWNER), vec![insert])
        .await
        .unwrap();
    let labels: Vec<PropertyOptionValue> = options(&seeded.world, priority.property_definition_id)
        .into_iter()
        .map(|(_, value, _)| value)
        .collect();
    assert_eq!(
        labels,
        vec![
            PropertyOptionValue::String("High".into()),
            PropertyOptionValue::String("Low".into()),
            PropertyOptionValue::String("Someday".into()),
        ]
    );
}

#[tokio::test]
async fn an_update_creating_an_option_of_a_shared_property_takes_the_right_to_edit_it() {
    let seeded = seeded().await;
    let priority = shared_priority_column(&seeded).await;
    let update = DatabaseOp::UpdateRows {
        table: seeded.table_id,
        changes: RowChanges::Uniform {
            rows: vec![seeded.row_id],
            cells: vec![CellWrite {
                column: priority.id,
                value: CellValue::Options(vec![OptionRef::Label("Someday".into())]),
            }],
        },
        create_missing_options: true,
    };

    let error = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![update.clone()],
        )
        .await
        .unwrap_err();
    assert_eq!(
        refusal(error),
        OpRefusal {
            op: 0,
            row: None,
            column: Some(priority.id),
            reason: SHARED_REFUSAL.into(),
        }
    );
    assert_eq!(seeded.world.lock().unwrap().write_batches, 0);

    seeded
        .world
        .lock()
        .unwrap()
        .editable_definitions
        .insert(OWNER.into(), vec![priority.property_definition_id]);
    seeded
        .service
        .apply_ops(edit(seeded.database_id), viewer(OWNER), vec![update])
        .await
        .unwrap();
    assert_eq!(
        options(&seeded.world, priority.property_definition_id).len(),
        3
    );
}

/// Naming an option the shared property already has creates nothing, so it
/// needs no right beyond the database's.
#[tokio::test]
async fn an_existing_option_of_a_shared_property_is_written_without_the_right_to_edit_it() {
    let seeded = seeded().await;
    let priority = shared_priority_column(&seeded).await;
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
                        column: priority.id,
                        value: CellValue::Options(vec![OptionRef::Label("High".into())]),
                    }],
                },
                create_missing_options: true,
            }],
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn adding_options_to_a_shared_property_takes_the_right_to_edit_it() {
    let seeded = seeded().await;
    let priority = shared_priority_column(&seeded).await;
    let add = || AddColumnOptions {
        table_id: seeded.table_id,
        column_id: priority.id,
        labels: vec!["Someday".into()],
    };

    let error = seeded
        .service
        .add_column_options(edit(seeded.database_id), viewer(OWNER), add())
        .await
        .unwrap_err();
    let DatabaseError::InvalidSchemaOperation(reason) = error else {
        panic!("expected a refused schema change, got {error:?}");
    };
    assert_eq!(reason, SHARED_REFUSAL);
    assert_eq!(
        options(&seeded.world, priority.property_definition_id).len(),
        2
    );

    seeded
        .world
        .lock()
        .unwrap()
        .editable_definitions
        .insert(OWNER.into(), vec![priority.property_definition_id]);
    let column = seeded
        .service
        .add_column_options(edit(seeded.database_id), viewer(OWNER), add())
        .await
        .unwrap();
    assert_eq!(column.definition.property_options.len(), 3);
}
