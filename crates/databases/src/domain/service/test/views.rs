//! View ops over the fakes: typed views of a table, a board's hand-arranged
//! cards, and what removing or retyping a column or an option does to the
//! views that refer to it.

use models_databases::views::{
    CardPosition, Conjunction, FilterCondition, FilterGroup, FilterNode, FilterTest, Lane, NewView,
    NumberOperator, RequestedLayout, SetOperator, SortDirection, SortKey, ViewColumn, ViewLayout,
    ViewPosition, ViewQuery,
};

use super::*;

fn refusal(error: DatabaseError) -> OpRefusal {
    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    refusal
}

fn board(seeded: &Seeded) -> ViewLayout {
    ViewLayout::Board {
        group_by: seeded.status_column.id,
        title: seeded.name_column.id,
        lanes: vec![],
        card_fields: vec![seeded.name_column.id],
        hide_empty_lanes: false,
    }
}

/// Create a view, answering it as stored.
async fn create_view(
    seeded: &Seeded,
    name: &str,
    query: ViewQuery,
    layout: ViewLayout,
) -> DatabaseView {
    let results = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![DatabaseOp::CreateView {
                table: seeded.table_id,
                view: NewView {
                    name: name.into(),
                    query,
                    layout: layout.into(),
                },
            }]),
        )
        .await
        .unwrap();
    let [OpResult::ViewWritten { view, .. }] = results.as_slice() else {
        panic!("expected a written view, got {results:?}");
    };
    *view.clone()
}

/// Rows Alex (Going) and Robin (Declined) after the seeded Sam (Going),
/// answering `[sam, alex, robin]`.
async fn three_guests(seeded: &Seeded) -> [RowId; 3] {
    let results = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![DatabaseOp::InsertRows {
                table: seeded.table_id,
                rows: [("Alex", "Going"), ("Robin", "Declined")]
                    .into_iter()
                    .map(|(name, status)| {
                        vec![
                            CellWrite {
                                column: seeded.name_column.id,
                                value: CellValue::Text(name.into()),
                            },
                            CellWrite {
                                column: seeded.status_column.id,
                                value: CellValue::Options(vec![OptionRef::Label(status.into())]),
                            },
                        ]
                    })
                    .collect(),
            }]),
        )
        .await
        .unwrap();
    let [OpResult::RowsWritten { inserted, .. }] = results.as_slice() else {
        panic!("expected one insert, got {results:?}");
    };
    [seeded.row_id, inserted[0], inserted[1]]
}

#[tokio::test]
async fn a_new_view_goes_after_the_tables_others_and_comes_with_the_detail() {
    let seeded = seeded().await;
    let before = table_version(&seeded.world, seeded.table_id);

    let table_view = create_view(
        &seeded,
        "  Everyone ",
        ViewQuery::default(),
        ViewLayout::Table {
            columns: vec![ViewColumn {
                column: seeded.name_column.id,
                width: Some(240),
            }],
        },
    )
    .await;
    let stages = create_view(&seeded, "Stages", ViewQuery::default(), board(&seeded)).await;

    assert_eq!(
        (
            table_view.database_id,
            table_view.table_id,
            table_view.name.as_str(),
            table_view.position.as_str()
        ),
        (seeded.database_id, seeded.table_id, "Everyone", "80")
    );
    assert_eq!(
        table_view.layout,
        ViewLayout::Table {
            columns: vec![ViewColumn {
                column: seeded.name_column.id,
                width: Some(240),
            }],
        }
    );
    assert_eq!(
        (stages.name.as_str(), stages.position.as_str()),
        ("Stages", "8180")
    );
    assert_eq!(
        table_version(&seeded.world, seeded.table_id),
        TableVersion(before.0 + 2)
    );
    let detail = seeded
        .service
        .get_database(receipt::<ViewAccessLevel>(
            seeded.database_id,
            OWNER,
            AccessLevel::Owner,
        ))
        .await
        .unwrap();
    assert_eq!(detail.tables[0].views, vec![table_view, stages]);
}

#[tokio::test]
async fn a_view_is_refused_when_it_does_not_fit_its_table() {
    let seeded = seeded().await;
    create_view(&seeded, "Everyone", ViewQuery::default(), board(&seeded)).await;
    let create = |name: &str, query: ViewQuery, layout: ViewLayout| DatabaseOp::CreateView {
        table: seeded.table_id,
        view: NewView {
            name: name.into(),
            query,
            layout: layout.into(),
        },
    };
    let cases = [
        (
            create("everyone", ViewQuery::default(), board(&seeded)),
            "a view named `everyone` already exists on this table".to_string(),
        ),
        (
            create(" ", ViewQuery::default(), board(&seeded)),
            "a view's name must not be empty".to_string(),
        ),
        (
            create(
                "Big parties",
                ViewQuery {
                    filter: Some(FilterGroup {
                        conjunction: Conjunction::And,
                        conditions: vec![FilterNode::Condition(FilterCondition {
                            column: seeded.name_column.id,
                            test: FilterTest::Number {
                                operator: NumberOperator::GreaterThan,
                                value: 2.0,
                            },
                        })],
                    }),
                    sort: vec![],
                },
                ViewLayout::Table { columns: vec![] },
            ),
            "\"Name\" holds text values; a number test does not fit it".to_string(),
        ),
        (
            create(
                "By name",
                ViewQuery::default(),
                ViewLayout::Board {
                    group_by: seeded.name_column.id,
                    title: seeded.name_column.id,
                    lanes: vec![],
                    card_fields: vec![],
                    hide_empty_lanes: false,
                },
            ),
            "a board is grouped by a single-select column, so each card has one lane; \"Name\" \
             is not one"
                .to_string(),
        ),
    ];
    for (op, reason) in cases {
        let error = seeded
            .service
            .apply_ops(
                edit(seeded.database_id),
                viewer(OWNER),
                OpBatch::from(vec![op]),
            )
            .await
            .unwrap_err();
        assert_eq!(
            refusal(error),
            OpRefusal {
                op: 0,
                row: None,
                column: None,
                taken: None,
                reason: reason.clone(),
            },
            "{reason}"
        );
    }
    assert_eq!(seeded.world.lock().unwrap().views.len(), 1);
}

#[tokio::test]
async fn an_update_changes_what_it_names_and_a_regrouped_board_forgets_its_cards() {
    let seeded = seeded().await;
    let stages = create_view(&seeded, "Stages", ViewQuery::default(), board(&seeded)).await;
    seeded.world.lock().unwrap().positions.insert(
        stages.id,
        vec![CardPosition {
            row: seeded.row_id,
            lane: None,
            position: "80".parse::<Position>().unwrap(),
        }],
    );
    let sorted = ViewQuery {
        filter: None,
        sort: vec![SortKey {
            column: seeded.plus_ones_column.id,
            direction: SortDirection::Descending,
        }],
    };

    seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![DatabaseOp::UpdateView {
                table: seeded.table_id,
                view: stages.id,
                name: Some("By size".into()),
                query: Some(sorted.clone()),
                layout: None,
            }]),
        )
        .await
        .unwrap();
    {
        let w = seeded.world.lock().unwrap();
        let stored = &w.views[0];
        assert_eq!(
            (stored.name.as_str(), &stored.query, &stored.layout),
            ("By size", &sorted, &board(&seeded))
        );
        assert_eq!(w.positions[&stages.id].len(), 1);
    }

    seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![DatabaseOp::UpdateView {
                table: seeded.table_id,
                view: stages.id,
                name: None,
                query: None,
                layout: Some(RequestedLayout::Table { columns: vec![] }),
            }]),
        )
        .await
        .unwrap();
    assert_eq!(seeded.world.lock().unwrap().positions.get(&stages.id), None);
}

#[tokio::test]
async fn views_reorder_when_the_order_names_each_of_them_once() {
    let seeded = seeded().await;
    let first = create_view(&seeded, "First", ViewQuery::default(), board(&seeded)).await;
    let second = create_view(&seeded, "Second", ViewQuery::default(), board(&seeded)).await;
    let third = create_view(&seeded, "Third", ViewQuery::default(), board(&seeded)).await;
    let reorder = |order: Vec<ViewId>| DatabaseOp::ReorderViews {
        table: seeded.table_id,
        order,
    };

    let error = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![reorder(vec![third.id, first.id])]),
        )
        .await
        .unwrap_err();
    assert_eq!(
        refusal(error).reason,
        "the order must name every view of this table exactly once"
    );

    let results = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![reorder(vec![third.id, first.id, second.id])]),
        )
        .await
        .unwrap();
    let [OpResult::ViewsReordered { positions, .. }] = results.as_slice() else {
        panic!("expected a reorder, got {results:?}");
    };
    assert_eq!(
        positions,
        &vec![
            ViewPosition {
                view: third.id,
                position: "7f80".parse::<Position>().unwrap(),
            },
            ViewPosition {
                view: first.id,
                position: "80".parse::<Position>().unwrap(),
            },
            ViewPosition {
                view: second.id,
                position: "8180".parse::<Position>().unwrap(),
            },
        ]
    );
    let detail = seeded
        .service
        .get_database(receipt::<ViewAccessLevel>(
            seeded.database_id,
            OWNER,
            AccessLevel::Owner,
        ))
        .await
        .unwrap();
    let names: Vec<&str> = detail.tables[0]
        .views
        .iter()
        .map(|view| view.name.as_str())
        .collect();
    assert_eq!(names, vec!["Third", "First", "Second"]);
}

#[tokio::test]
async fn a_deleted_view_takes_its_card_places_with_it() {
    let seeded = seeded().await;
    let stages = create_view(&seeded, "Stages", ViewQuery::default(), board(&seeded)).await;
    seeded.world.lock().unwrap().positions.insert(
        stages.id,
        vec![CardPosition {
            row: seeded.row_id,
            lane: None,
            position: "80".parse::<Position>().unwrap(),
        }],
    );

    let results = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![DatabaseOp::DeleteView {
                table: seeded.table_id,
                view: stages.id,
            }]),
        )
        .await
        .unwrap();

    assert!(matches!(results.as_slice(), [OpResult::ViewDeleted { .. }]));
    let w = seeded.world.lock().unwrap();
    assert!(w.views.is_empty());
    assert_eq!(w.positions.get(&stages.id), None);
}

#[tokio::test]
async fn moving_a_card_to_another_lane_sets_its_cell_and_places_it_there() {
    let seeded = seeded().await;
    let [sam, _alex, robin] = three_guests(&seeded).await;
    let stages = create_view(&seeded, "Stages", ViewQuery::default(), board(&seeded)).await;
    let status = seeded.status_column.property_definition_id;
    let declined = option_id(&seeded.world, status, "Declined");
    let before = table_version(&seeded.world, seeded.table_id);

    let results = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![DatabaseOp::MoveCard {
                table: seeded.table_id,
                view: stages.id,
                row: sam,
                lane: Some(declined),
                before: None,
                after: Some(robin),
            }]),
        )
        .await
        .unwrap();

    assert_eq!(
        results,
        vec![OpResult::CardMoved {
            table_version: TableVersion(before.0 + 1),
            positions: vec![CardPosition {
                row: sam,
                lane: Some(declined),
                position: "80".parse::<Position>().unwrap(),
            }],
        }]
    );
    assert_eq!(
        cell(&seeded.world, sam, status),
        Some(PropertyValue::SelectOption(vec![declined.into_uuid()]))
    );

    seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![DatabaseOp::MoveCard {
                table: seeded.table_id,
                view: stages.id,
                row: sam,
                lane: None,
                before: None,
                after: None,
            }]),
        )
        .await
        .unwrap();
    assert_eq!(cell(&seeded.world, sam, status), None);
}

#[tokio::test]
async fn moving_a_card_within_its_lane_places_the_unplaced_cards_before_it() {
    let seeded = seeded().await;
    let [sam, alex, _robin] = three_guests(&seeded).await;
    let extra = insert_names(&seeded, &["Kim"]).await[0];
    let going = option_id(
        &seeded.world,
        seeded.status_column.property_definition_id,
        "Going",
    );
    seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![DatabaseOp::UpdateRows {
                table: seeded.table_id,
                changes: RowChanges::Uniform {
                    rows: vec![extra],
                    cells: vec![CellWrite {
                        column: seeded.status_column.id,
                        value: CellValue::Options(vec![OptionRef::Id(going)]),
                    }],
                },
            }]),
        )
        .await
        .unwrap();
    let stages = create_view(&seeded, "Stages", ViewQuery::default(), board(&seeded)).await;

    let results = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![
                DatabaseOp::MoveCard {
                    table: seeded.table_id,
                    view: stages.id,
                    row: sam,
                    lane: Some(going),
                    before: Some(alex),
                    after: Some(extra),
                },
                DatabaseOp::MoveCard {
                    table: seeded.table_id,
                    view: stages.id,
                    row: extra,
                    lane: Some(going),
                    before: None,
                    after: Some(alex),
                },
            ]),
        )
        .await
        .unwrap();

    assert_eq!(
        results
            .iter()
            .map(|result| match result {
                OpResult::CardMoved { positions, .. } => positions.clone(),
                other => panic!("expected a moved card, got {other:?}"),
            })
            .collect::<Vec<_>>(),
        vec![
            vec![
                CardPosition {
                    row: alex,
                    lane: Some(going),
                    position: "7f80".parse::<Position>().unwrap(),
                },
                CardPosition {
                    row: sam,
                    lane: Some(going),
                    position: "80".parse::<Position>().unwrap(),
                },
            ],
            vec![CardPosition {
                row: extra,
                lane: Some(going),
                position: "7e80".parse::<Position>().unwrap(),
            }],
        ]
    );
    let mut stored = seeded.world.lock().unwrap().positions[&stages.id].clone();
    stored.sort_by(|left, right| left.position.cmp(&right.position));
    let order: Vec<RowId> = stored.iter().map(|placed| placed.row).collect();
    assert_eq!(order, vec![extra, alex, sam]);
}

#[tokio::test]
async fn a_sorted_board_keeps_its_cards_in_the_sorts_order() {
    let seeded = seeded().await;
    let sorted = create_view(
        &seeded,
        "By size",
        ViewQuery {
            filter: None,
            sort: vec![SortKey {
                column: seeded.plus_ones_column.id,
                direction: SortDirection::Ascending,
            }],
        },
        board(&seeded),
    )
    .await;
    let everyone = create_view(
        &seeded,
        "Everyone",
        ViewQuery::default(),
        ViewLayout::Table { columns: vec![] },
    )
    .await;
    let move_on = |view: ViewId| DatabaseOp::MoveCard {
        table: seeded.table_id,
        view,
        row: seeded.row_id,
        lane: None,
        before: None,
        after: None,
    };

    for (view, reason) in [
        (
            sorted.id,
            "\"By size\" is sorted, so its cards keep the sort's order; remove the sort to \
             arrange them by hand",
        ),
        (
            everyone.id,
            "\"Everyone\" is a table view; only a board's cards move",
        ),
    ] {
        let error = seeded
            .service
            .apply_ops(
                edit(seeded.database_id),
                viewer(OWNER),
                OpBatch::from(vec![move_on(view)]),
            )
            .await
            .unwrap_err();
        assert_eq!(
            refusal(error),
            OpRefusal {
                op: 0,
                row: None,
                column: None,
                taken: None,
                reason: reason.into(),
            }
        );
    }
    assert!(seeded.world.lock().unwrap().positions.is_empty());
}

#[tokio::test]
async fn a_card_moves_only_next_to_cards_of_its_new_lane() {
    let seeded = seeded().await;
    let [sam, _alex, robin] = three_guests(&seeded).await;
    let stages = create_view(&seeded, "Stages", ViewQuery::default(), board(&seeded)).await;
    let going = option_id(
        &seeded.world,
        seeded.status_column.property_definition_id,
        "Going",
    );

    let error = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![DatabaseOp::MoveCard {
                table: seeded.table_id,
                view: stages.id,
                row: sam,
                lane: Some(going),
                before: Some(robin),
                after: None,
            }]),
        )
        .await
        .unwrap_err();

    assert_eq!(
        refusal(error).reason,
        format!("row {robin} is not a card of that lane")
    );
}

#[tokio::test]
async fn removing_a_column_takes_it_out_of_views_unless_a_board_groups_by_it() {
    let seeded = seeded().await;
    let filtered = create_view(
        &seeded,
        "Big parties",
        ViewQuery {
            filter: Some(FilterGroup {
                conjunction: Conjunction::And,
                conditions: vec![FilterNode::Condition(FilterCondition {
                    column: seeded.plus_ones_column.id,
                    test: FilterTest::Number {
                        operator: NumberOperator::GreaterThan,
                        value: 1.0,
                    },
                })],
            }),
            sort: vec![SortKey {
                column: seeded.plus_ones_column.id,
                direction: SortDirection::Descending,
            }],
        },
        ViewLayout::Board {
            group_by: seeded.status_column.id,
            title: seeded.name_column.id,
            lanes: vec![],
            card_fields: vec![seeded.name_column.id, seeded.plus_ones_column.id],
            hide_empty_lanes: false,
        },
    )
    .await;
    let seeded = &seeded;
    let delete = |column: ColumnId| async move {
        let version = table_version(&seeded.world, seeded.table_id);
        seeded
            .service
            .apply_ops(
                edit(seeded.database_id),
                viewer(OWNER),
                OpBatch {
                    ops: vec![DatabaseOp::DeleteColumn {
                        table: seeded.table_id,
                        column,
                    }],
                    base_versions: HashMap::from([(seeded.table_id, version)]),
                },
            )
            .await
    };

    delete(seeded.plus_ones_column.id).await.unwrap();
    {
        let w = seeded.world.lock().unwrap();
        let stored = w.views.iter().find(|view| view.id == filtered.id).unwrap();
        assert_eq!(stored.query, ViewQuery::default());
        assert_eq!(
            stored.layout,
            ViewLayout::Board {
                group_by: seeded.status_column.id,
                title: seeded.name_column.id,
                lanes: vec![],
                card_fields: vec![seeded.name_column.id],
                hide_empty_lanes: false,
            }
        );
    }

    let error = delete(seeded.status_column.id).await.unwrap_err();
    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused removal, got {error:?}");
    };
    assert_eq!(
        refusal.reason,
        SchemaError::BoardGroupsByRemovedColumn {
            board: "Big parties".into()
        }
        .to_string()
    );

    seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![DatabaseOp::DeleteView {
                table: seeded.table_id,
                view: filtered.id,
            }]),
        )
        .await
        .unwrap();
    delete(seeded.status_column.id).await.unwrap();
}

#[tokio::test]
async fn a_board_created_without_a_title_is_titled_by_the_first_column() {
    let seeded = seeded().await;
    let results = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![DatabaseOp::CreateView {
                table: seeded.table_id,
                view: NewView {
                    name: "Stages".into(),
                    query: ViewQuery::default(),
                    layout: RequestedLayout::Board {
                        group_by: seeded.status_column.id,
                        title: None,
                        lanes: vec![],
                        card_fields: vec![seeded.plus_ones_column.id],
                        hide_empty_lanes: false,
                    },
                },
            }]),
        )
        .await
        .unwrap();
    let [OpResult::ViewWritten { view, .. }] = results.as_slice() else {
        panic!("expected a written view, got {results:?}");
    };

    assert_eq!(
        view.layout,
        ViewLayout::Board {
            group_by: seeded.status_column.id,
            title: seeded.name_column.id,
            lanes: vec![],
            card_fields: vec![seeded.plus_ones_column.id],
            hide_empty_lanes: false,
        }
    );
    assert_eq!(seeded.world.lock().unwrap().views[0].layout, view.layout);
}

#[tokio::test]
async fn an_update_without_a_title_keeps_the_boards_title() {
    let seeded = seeded().await;
    let stages = create_view(
        &seeded,
        "Stages",
        ViewQuery::default(),
        ViewLayout::Board {
            group_by: seeded.status_column.id,
            title: seeded.plus_ones_column.id,
            lanes: vec![],
            card_fields: vec![],
            hide_empty_lanes: false,
        },
    )
    .await;

    seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![DatabaseOp::UpdateView {
                table: seeded.table_id,
                view: stages.id,
                name: None,
                query: None,
                layout: Some(RequestedLayout::Board {
                    group_by: seeded.status_column.id,
                    title: None,
                    lanes: vec![],
                    card_fields: vec![],
                    hide_empty_lanes: true,
                }),
            }]),
        )
        .await
        .unwrap();

    assert_eq!(
        seeded.world.lock().unwrap().views[0].layout,
        ViewLayout::Board {
            group_by: seeded.status_column.id,
            title: seeded.plus_ones_column.id,
            lanes: vec![],
            card_fields: vec![],
            hide_empty_lanes: true,
        }
    );
}

#[tokio::test]
async fn a_board_titled_by_an_unknown_column_is_refused() {
    let seeded = seeded().await;
    let ghost = ColumnId::new();
    let error = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![DatabaseOp::CreateView {
                table: seeded.table_id,
                view: NewView {
                    name: "Stages".into(),
                    query: ViewQuery::default(),
                    layout: RequestedLayout::Board {
                        group_by: seeded.status_column.id,
                        title: Some(ghost),
                        lanes: vec![],
                        card_fields: vec![],
                        hide_empty_lanes: false,
                    },
                },
            }]),
        )
        .await
        .unwrap_err();

    assert_eq!(
        refusal(error).reason,
        format!("no column {ghost} in this table")
    );
    assert!(seeded.world.lock().unwrap().views.is_empty());
}

#[tokio::test]
async fn removing_a_boards_title_column_titles_it_by_the_next_first_column() {
    let seeded = seeded().await;
    let stages = create_view(
        &seeded,
        "Stages",
        ViewQuery::default(),
        ViewLayout::Board {
            group_by: seeded.status_column.id,
            title: seeded.name_column.id,
            lanes: vec![],
            card_fields: vec![seeded.name_column.id, seeded.plus_ones_column.id],
            hide_empty_lanes: false,
        },
    )
    .await;
    let version = table_version(&seeded.world, seeded.table_id);

    seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch {
                ops: vec![DatabaseOp::DeleteColumn {
                    table: seeded.table_id,
                    column: seeded.name_column.id,
                }],
                base_versions: HashMap::from([(seeded.table_id, version)]),
            },
        )
        .await
        .unwrap();

    let w = seeded.world.lock().unwrap();
    let stored = w.views.iter().find(|view| view.id == stages.id).unwrap();
    assert_eq!(
        stored.layout,
        ViewLayout::Board {
            group_by: seeded.status_column.id,
            title: seeded.status_column.id,
            lanes: vec![],
            card_fields: vec![seeded.plus_ones_column.id],
            hide_empty_lanes: false,
        }
    );
}

#[tokio::test]
async fn removing_an_option_takes_it_out_of_views_lanes_and_card_places() {
    let seeded = seeded().await;
    let status = seeded.status_column.property_definition_id;
    let going = option_id(&seeded.world, status, "Going");
    let declined = option_id(&seeded.world, status, "Declined");
    let stages = create_view(
        &seeded,
        "Stages",
        ViewQuery {
            filter: Some(FilterGroup {
                conjunction: Conjunction::Or,
                conditions: vec![FilterNode::Condition(FilterCondition {
                    column: seeded.status_column.id,
                    test: FilterTest::Options {
                        operator: SetOperator::IsAnyOf,
                        options: vec![going, declined],
                    },
                })],
            }),
            sort: vec![],
        },
        ViewLayout::Board {
            group_by: seeded.status_column.id,
            title: seeded.name_column.id,
            lanes: vec![
                Lane {
                    option: Some(going),
                    hidden: false,
                },
                Lane {
                    option: Some(declined),
                    hidden: true,
                },
            ],
            card_fields: vec![],
            hide_empty_lanes: false,
        },
    )
    .await;
    seeded.world.lock().unwrap().positions.insert(
        stages.id,
        vec![CardPosition {
            row: seeded.row_id,
            lane: Some(going),
            position: "80".parse::<Position>().unwrap(),
        }],
    );

    seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![DatabaseOp::DeleteOption {
                table: seeded.table_id,
                column: seeded.status_column.id,
                option: going,
            }]),
        )
        .await
        .unwrap();

    let w = seeded.world.lock().unwrap();
    let stored = &w.views[0];
    assert_eq!(
        stored.query,
        ViewQuery {
            filter: Some(FilterGroup {
                conjunction: Conjunction::Or,
                conditions: vec![FilterNode::Condition(FilterCondition {
                    column: seeded.status_column.id,
                    test: FilterTest::Options {
                        operator: SetOperator::IsAnyOf,
                        options: vec![declined],
                    },
                })],
            }),
            sort: vec![],
        }
    );
    assert_eq!(
        stored.layout,
        ViewLayout::Board {
            group_by: seeded.status_column.id,
            title: seeded.name_column.id,
            lanes: vec![Lane {
                option: Some(declined),
                hidden: true,
            }],
            card_fields: vec![],
            hide_empty_lanes: false,
        }
    );
    assert_eq!(w.positions[&stages.id], vec![]);
}

#[tokio::test]
async fn a_new_type_drops_the_tests_of_the_old_one_but_not_under_a_board() {
    let seeded = seeded().await;
    let filtered = create_view(
        &seeded,
        "Big parties",
        ViewQuery {
            filter: Some(FilterGroup {
                conjunction: Conjunction::And,
                conditions: vec![FilterNode::Condition(FilterCondition {
                    column: seeded.plus_ones_column.id,
                    test: FilterTest::Number {
                        operator: NumberOperator::GreaterThan,
                        value: 1.0,
                    },
                })],
            }),
            sort: vec![SortKey {
                column: seeded.plus_ones_column.id,
                direction: SortDirection::Ascending,
            }],
        },
        ViewLayout::Table { columns: vec![] },
    )
    .await;
    let retype =
        |column: ColumnId, to: models_databases::ColumnKind| DatabaseOp::ChangeColumnType {
            table: seeded.table_id,
            column,
            to,
        };

    seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![retype(
                seeded.plus_ones_column.id,
                models_databases::ColumnKind::Text,
            )]),
        )
        .await
        .unwrap();
    let stored = seeded
        .world
        .lock()
        .unwrap()
        .views
        .iter()
        .find(|view| view.id == filtered.id)
        .unwrap()
        .query
        .clone();
    assert_eq!(
        stored,
        ViewQuery {
            filter: None,
            sort: vec![SortKey {
                column: seeded.plus_ones_column.id,
                direction: SortDirection::Ascending,
            }],
        }
    );

    create_view(&seeded, "Stages", ViewQuery::default(), board(&seeded)).await;
    let error = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            OpBatch::from(vec![retype(
                seeded.status_column.id,
                models_databases::ColumnKind::Text,
            )]),
        )
        .await
        .unwrap_err();
    assert_eq!(
        refusal(error).reason,
        "The board \"Stages\" groups its cards by this column; delete the board or group it by \
         another column before changing its type."
    );
}

#[tokio::test]
async fn a_boards_card_places_read_back_for_its_database_alone() {
    let seeded = seeded().await;
    let stages = create_view(&seeded, "Stages", ViewQuery::default(), board(&seeded)).await;
    let placed = vec![CardPosition {
        row: seeded.row_id,
        lane: None,
        position: "80".parse::<Position>().unwrap(),
    }];
    seeded
        .world
        .lock()
        .unwrap()
        .positions
        .insert(stages.id, placed.clone());
    let view = receipt::<ViewAccessLevel>(seeded.database_id, VIEWER, AccessLevel::View);

    assert_eq!(
        seeded
            .service
            .view_positions(view, stages.id)
            .await
            .unwrap(),
        placed
    );
    let elsewhere = receipt::<ViewAccessLevel>(DatabaseId::new(), OWNER, AccessLevel::Owner);
    assert!(matches!(
        seeded.service.view_positions(elsewhere, stages.id).await,
        Err(DatabaseError::NotFound)
    ));
}
