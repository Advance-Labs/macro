use chrono::TimeZone;
use serde_json::json;

use super::*;
use uuid::Uuid;

const NAME: ColumnId = ColumnId::from_uuid(Uuid::from_u128(0xc01a));
const STATUS: ColumnId = ColumnId::from_uuid(Uuid::from_u128(0xc01b));
const DIET: ColumnId = ColumnId::from_uuid(Uuid::from_u128(0xc01c));
const PLUS_ONES: ColumnId = ColumnId::from_uuid(Uuid::from_u128(0xc01d));
const GOING: OptionId = OptionId::from_uuid(Uuid::from_u128(0x0b1));
const DECLINED: OptionId = OptionId::from_uuid(Uuid::from_u128(0x0b2));
const VEGAN: OptionId = OptionId::from_uuid(Uuid::from_u128(0x0b3));

/// `Guests(Name TEXT, Status SELECT[Going|Declined], Diet MULTI SELECT[Vegan],
/// Plus ones NUMBER)`.
fn guests() -> Vec<SchemaColumn> {
    vec![
        SchemaColumn {
            id: NAME,
            name: "Name".into(),
            values: ValueKind::Text,
            multi: false,
            options: vec![],
        },
        SchemaColumn {
            id: STATUS,
            name: "Status".into(),
            values: ValueKind::Options,
            multi: false,
            options: vec![GOING, DECLINED],
        },
        SchemaColumn {
            id: DIET,
            name: "Diet".into(),
            values: ValueKind::Options,
            multi: true,
            options: vec![VEGAN],
        },
        SchemaColumn {
            id: PLUS_ONES,
            name: "Plus ones".into(),
            values: ValueKind::Number,
            multi: false,
            options: vec![],
        },
    ]
}

#[test]
fn a_view_reads_from_json_with_its_filter_tree_and_layout() {
    let view: NewView = serde_json::from_value(json!({
        "name": "Coming",
        "query": {
            "filter": {
                "conjunction": "or",
                "conditions": [
                    {
                        "kind": "condition",
                        "column": STATUS,
                        "test": {"kind": "options", "operator": "isAnyOf", "options": [GOING]},
                    },
                    {
                        "kind": "group",
                        "conjunction": "and",
                        "conditions": [{
                            "kind": "condition",
                            "column": PLUS_ONES,
                            "test": {"kind": "number", "operator": "greaterThan", "value": 1},
                        }],
                    },
                ],
            },
            "sort": [{"column": NAME, "direction": "descending"}],
        },
        "layout": {
            "kind": "board",
            "groupBy": STATUS,
            "lanes": [{"option": DECLINED, "hidden": true}, {"option": null}],
            "cardFields": [NAME],
            "hideEmptyLanes": false,
        },
    }))
    .unwrap();

    assert_eq!(
        view,
        NewView {
            name: "Coming".into(),
            query: ViewQuery {
                filter: Some(FilterGroup {
                    conjunction: Conjunction::Or,
                    conditions: vec![
                        FilterNode::Condition(FilterCondition {
                            column: STATUS,
                            test: FilterTest::Options {
                                operator: SetOperator::IsAnyOf,
                                options: vec![GOING],
                            }
                        }),
                        FilterNode::Group(FilterGroup {
                            conjunction: Conjunction::And,
                            conditions: vec![FilterNode::Condition(FilterCondition {
                                column: PLUS_ONES,
                                test: FilterTest::Number {
                                    operator: NumberOperator::GreaterThan,
                                    value: 1.0,
                                }
                            })],
                        }),
                    ],
                }),
                sort: vec![SortKey {
                    column: NAME,
                    direction: SortDirection::Descending,
                }],
            },
            layout: RequestedLayout::Board {
                group_by: STATUS,
                title: None,
                lanes: vec![
                    Lane {
                        option: Some(DECLINED),
                        hidden: true,
                    },
                    Lane {
                        option: None,
                        hidden: false,
                    },
                ],
                card_fields: vec![NAME],
                hide_empty_lanes: false,
            },
        }
    );
}

#[test]
fn a_view_that_fits_its_table_passes() {
    let query = ViewQuery {
        filter: Some(FilterGroup {
            conjunction: Conjunction::And,
            conditions: vec![
                FilterNode::Condition(FilterCondition {
                    column: NAME,
                    test: FilterTest::Text {
                        operator: TextOperator::Contains,
                        value: "sam".into(),
                    },
                }),
                FilterNode::Condition(FilterCondition {
                    column: DIET,
                    test: FilterTest::Options {
                        operator: SetOperator::HasNone,
                        options: vec![VEGAN],
                    },
                }),
                FilterNode::Condition(FilterCondition {
                    column: PLUS_ONES,
                    test: FilterTest::Presence {
                        operator: PresenceOperator::IsNotEmpty,
                    },
                }),
            ],
        }),
        sort: vec![SortKey {
            column: PLUS_ONES,
            direction: SortDirection::Ascending,
        }],
    };
    let board = ViewLayout::Board {
        group_by: STATUS,
        title: NAME,
        lanes: vec![
            Lane {
                option: Some(GOING),
                hidden: false,
            },
            Lane {
                option: None,
                hidden: true,
            },
        ],
        card_fields: vec![NAME, PLUS_ONES],
        hide_empty_lanes: true,
    };

    assert_eq!(check(&query, &board, &guests()), Ok(()));
}

#[test]
fn a_view_that_does_not_fit_its_table_says_why() {
    let ghost = ColumnId::from_uuid(Uuid::from_u128(0x6057));
    let cases = [
        (
            ViewQuery {
                filter: Some(FilterGroup {
                    conjunction: Conjunction::And,
                    conditions: vec![FilterNode::Condition(FilterCondition {
                        column: ghost,
                        test: FilterTest::Presence {
                            operator: PresenceOperator::IsEmpty,
                        },
                    })],
                }),
                sort: vec![],
            },
            ViewLayout::Table { columns: vec![] },
            ViewProblem::UnknownColumn { column: ghost },
            "no column 00000000-0000-0000-0000-000000006057 in this table",
        ),
        (
            ViewQuery {
                filter: Some(FilterGroup {
                    conjunction: Conjunction::And,
                    conditions: vec![FilterNode::Condition(FilterCondition {
                        column: PLUS_ONES,
                        test: FilterTest::Text {
                            operator: TextOperator::Is,
                            value: "two".into(),
                        },
                    })],
                }),
                sort: vec![],
            },
            ViewLayout::Table { columns: vec![] },
            ViewProblem::TestDoesNotFit {
                column: "Plus ones".into(),
                holds: ValueKind::Number,
                test: ValueKind::Text,
            },
            "\"Plus ones\" holds number values; a text test does not fit it",
        ),
        (
            ViewQuery {
                filter: Some(FilterGroup {
                    conjunction: Conjunction::And,
                    conditions: vec![FilterNode::Condition(FilterCondition {
                        column: DIET,
                        test: FilterTest::Options {
                            operator: SetOperator::IsAnyOf,
                            options: vec![VEGAN],
                        },
                    })],
                }),
                sort: vec![],
            },
            ViewLayout::Table { columns: vec![] },
            ViewProblem::OperatorDoesNotFit {
                column: "Diet".into(),
                multi: true,
            },
            "\"Diet\" holds several values; test it with hasAny, hasAll or hasNone",
        ),
        (
            ViewQuery {
                filter: Some(FilterGroup {
                    conjunction: Conjunction::And,
                    conditions: vec![FilterNode::Condition(FilterCondition {
                        column: STATUS,
                        test: FilterTest::Options {
                            operator: SetOperator::HasAll,
                            options: vec![GOING],
                        },
                    })],
                }),
                sort: vec![],
            },
            ViewLayout::Table { columns: vec![] },
            ViewProblem::OperatorDoesNotFit {
                column: "Status".into(),
                multi: false,
            },
            "\"Status\" holds one value; test it with isAnyOf or isNoneOf",
        ),
        (
            ViewQuery {
                filter: Some(FilterGroup {
                    conjunction: Conjunction::And,
                    conditions: vec![FilterNode::Condition(FilterCondition {
                        column: STATUS,
                        test: FilterTest::Options {
                            operator: SetOperator::IsNoneOf,
                            options: vec![],
                        },
                    })],
                }),
                sort: vec![],
            },
            ViewLayout::Table { columns: vec![] },
            ViewProblem::NothingToMatch {
                column: "Status".into(),
            },
            "a test of \"Status\" names nothing to match",
        ),
        (
            ViewQuery {
                filter: Some(FilterGroup {
                    conjunction: Conjunction::And,
                    conditions: vec![FilterNode::Condition(FilterCondition {
                        column: STATUS,
                        test: FilterTest::Options {
                            operator: SetOperator::IsAnyOf,
                            options: vec![VEGAN],
                        },
                    })],
                }),
                sort: vec![],
            },
            ViewLayout::Table { columns: vec![] },
            ViewProblem::UnknownOption {
                column: "Status".into(),
                option: VEGAN,
            },
            "no option 00000000-0000-0000-0000-0000000000b3 on \"Status\"",
        ),
        (
            ViewQuery {
                filter: Some(FilterGroup {
                    conjunction: Conjunction::And,
                    conditions: vec![FilterNode::Condition(FilterCondition {
                        column: PLUS_ONES,
                        test: FilterTest::Number {
                            operator: NumberOperator::LessThan,
                            value: f64::INFINITY,
                        },
                    })],
                }),
                sort: vec![],
            },
            ViewLayout::Table { columns: vec![] },
            ViewProblem::NotFinite {
                column: "Plus ones".into(),
            },
            "a test of \"Plus ones\" compares against a number that is not finite",
        ),
        (
            ViewQuery {
                filter: None,
                sort: vec![
                    SortKey {
                        column: NAME,
                        direction: SortDirection::Ascending,
                    },
                    SortKey {
                        column: NAME,
                        direction: SortDirection::Descending,
                    },
                ],
            },
            ViewLayout::Table { columns: vec![] },
            ViewProblem::RepeatedColumn {
                column: "Name".into(),
            },
            "\"Name\" is listed twice",
        ),
        (
            ViewQuery::default(),
            ViewLayout::Board {
                group_by: DIET,
                title: NAME,
                lanes: vec![],
                card_fields: vec![],
                hide_empty_lanes: false,
            },
            ViewProblem::BoardNeedsSingleSelect {
                column: "Diet".into(),
            },
            "a board is grouped by a single-select column, so each card has one lane; \"Diet\" \
             is not one",
        ),
        (
            ViewQuery::default(),
            ViewLayout::Board {
                group_by: STATUS,
                title: NAME,
                lanes: vec![
                    Lane {
                        option: None,
                        hidden: false,
                    },
                    Lane {
                        option: None,
                        hidden: true,
                    },
                ],
                card_fields: vec![],
                hide_empty_lanes: false,
            },
            ViewProblem::RepeatedLane,
            "a lane is listed twice",
        ),
        (
            ViewQuery::default(),
            ViewLayout::Board {
                group_by: STATUS,
                title: ghost,
                lanes: vec![],
                card_fields: vec![],
                hide_empty_lanes: false,
            },
            ViewProblem::UnknownColumn { column: ghost },
            "no column 00000000-0000-0000-0000-000000006057 in this table",
        ),
        (
            ViewQuery::default(),
            ViewLayout::Board {
                group_by: STATUS,
                title: NAME,
                lanes: vec![Lane {
                    option: Some(VEGAN),
                    hidden: false,
                }],
                card_fields: vec![],
                hide_empty_lanes: false,
            },
            ViewProblem::UnknownOption {
                column: "Status".into(),
                option: VEGAN,
            },
            "no option 00000000-0000-0000-0000-0000000000b3 on \"Status\"",
        ),
    ];
    for (query, layout, problem, message) in cases {
        let found = check(&query, &layout, &guests());
        assert_eq!(found, Err(problem.clone()), "{message}");
        assert_eq!(problem.to_string(), message);
    }
}

#[test]
fn removing_a_column_drops_its_conditions_sort_and_fields() {
    let query = ViewQuery {
        filter: Some(FilterGroup {
            conjunction: Conjunction::Or,
            conditions: vec![
                FilterNode::Condition(FilterCondition {
                    column: NAME,
                    test: FilterTest::Text {
                        operator: TextOperator::StartsWith,
                        value: "S".into(),
                    },
                }),
                FilterNode::Group(FilterGroup {
                    conjunction: Conjunction::And,
                    conditions: vec![FilterNode::Condition(FilterCondition {
                        column: PLUS_ONES,
                        test: FilterTest::Number {
                            operator: NumberOperator::Is,
                            value: 2.0,
                        },
                    })],
                }),
            ],
        }),
        sort: vec![
            SortKey {
                column: PLUS_ONES,
                direction: SortDirection::Descending,
            },
            SortKey {
                column: NAME,
                direction: SortDirection::Ascending,
            },
        ],
    };

    assert_eq!(
        query.without_column(PLUS_ONES),
        ViewQuery {
            filter: Some(FilterGroup {
                conjunction: Conjunction::Or,
                conditions: vec![FilterNode::Condition(FilterCondition {
                    column: NAME,
                    test: FilterTest::Text {
                        operator: TextOperator::StartsWith,
                        value: "S".into(),
                    }
                })],
            }),
            sort: vec![SortKey {
                column: NAME,
                direction: SortDirection::Ascending,
            }],
        }
    );
    assert_eq!(
        query.without_column(PLUS_ONES).without_column(NAME),
        ViewQuery {
            filter: None,
            sort: vec![],
        }
    );

    let board = ViewLayout::Board {
        group_by: STATUS,
        title: NAME,
        lanes: vec![],
        card_fields: vec![NAME, PLUS_ONES],
        hide_empty_lanes: false,
    };
    assert_eq!(
        board.without_column(PLUS_ONES, Some(NAME)),
        Some(ViewLayout::Board {
            group_by: STATUS,
            title: NAME,
            lanes: vec![],
            card_fields: vec![NAME],
            hide_empty_lanes: false,
        })
    );
    assert_eq!(board.without_column(STATUS, Some(NAME)), None);
    assert_eq!(
        ViewLayout::Table {
            columns: vec![
                ViewColumn {
                    column: NAME,
                    width: Some(240),
                },
                ViewColumn {
                    column: PLUS_ONES,
                    width: None,
                },
            ],
        }
        .without_column(NAME, Some(PLUS_ONES)),
        Some(ViewLayout::Table {
            columns: vec![ViewColumn {
                column: PLUS_ONES,
                width: None,
            }],
        })
    );
}

#[test]
fn removing_a_boards_title_column_titles_it_by_the_next_first_column() {
    let board = ViewLayout::Board {
        group_by: STATUS,
        title: NAME,
        lanes: vec![],
        card_fields: vec![PLUS_ONES],
        hide_empty_lanes: false,
    };

    assert_eq!(
        board.without_column(NAME, Some(STATUS)),
        Some(ViewLayout::Board {
            group_by: STATUS,
            title: STATUS,
            lanes: vec![],
            card_fields: vec![PLUS_ONES],
            hide_empty_lanes: false,
        })
    );
    assert_eq!(
        board.without_column(PLUS_ONES, Some(NAME)),
        Some(ViewLayout::Board {
            group_by: STATUS,
            title: NAME,
            lanes: vec![],
            card_fields: vec![],
            hide_empty_lanes: false,
        })
    );
}

#[test]
fn a_board_asked_for_without_a_title_takes_the_default() {
    let asked = RequestedLayout::Board {
        group_by: STATUS,
        title: None,
        lanes: vec![],
        card_fields: vec![PLUS_ONES],
        hide_empty_lanes: true,
    };

    assert_eq!(
        asked.clone().with_default_title(Some(NAME)),
        Some(ViewLayout::Board {
            group_by: STATUS,
            title: NAME,
            lanes: vec![],
            card_fields: vec![PLUS_ONES],
            hide_empty_lanes: true,
        })
    );
    assert_eq!(asked.with_default_title(None), None);
    assert_eq!(
        RequestedLayout::Board {
            group_by: STATUS,
            title: Some(PLUS_ONES),
            lanes: vec![],
            card_fields: vec![],
            hide_empty_lanes: false,
        }
        .with_default_title(Some(NAME)),
        Some(ViewLayout::Board {
            group_by: STATUS,
            title: PLUS_ONES,
            lanes: vec![],
            card_fields: vec![],
            hide_empty_lanes: false,
        })
    );
}

#[test]
fn a_stored_board_reads_its_title() {
    let layout: ViewLayout = serde_json::from_value(json!({
        "kind": "board",
        "groupBy": STATUS,
        "title": DIET,
        "lanes": [],
        "cardFields": [],
        "hideEmptyLanes": false,
    }))
    .unwrap();

    assert_eq!(
        layout,
        ViewLayout::Board {
            group_by: STATUS,
            title: DIET,
            lanes: vec![],
            card_fields: vec![],
            hide_empty_lanes: false,
        }
    );
    assert!(
        serde_json::from_value::<ViewLayout>(json!({
            "kind": "board",
            "groupBy": STATUS,
            "lanes": [],
            "cardFields": [],
            "hideEmptyLanes": false,
        }))
        .is_err()
    );
}

#[test]
fn removing_an_option_drops_it_from_tests_and_lanes() {
    let query = ViewQuery {
        filter: Some(FilterGroup {
            conjunction: Conjunction::And,
            conditions: vec![
                FilterNode::Condition(FilterCondition {
                    column: STATUS,
                    test: FilterTest::Options {
                        operator: SetOperator::IsNoneOf,
                        options: vec![GOING, DECLINED],
                    },
                }),
                FilterNode::Condition(FilterCondition {
                    column: STATUS,
                    test: FilterTest::Options {
                        operator: SetOperator::IsAnyOf,
                        options: vec![GOING],
                    },
                }),
            ],
        }),
        sort: vec![],
    };

    assert_eq!(
        query.without_option(STATUS, GOING),
        ViewQuery {
            filter: Some(FilterGroup {
                conjunction: Conjunction::And,
                conditions: vec![FilterNode::Condition(FilterCondition {
                    column: STATUS,
                    test: FilterTest::Options {
                        operator: SetOperator::IsNoneOf,
                        options: vec![DECLINED],
                    }
                })]
            }),
            sort: vec![]
        }
    );
    assert_eq!(query.without_option(DIET, GOING), query);

    let board = ViewLayout::Board {
        group_by: STATUS,
        title: NAME,
        lanes: vec![
            Lane {
                option: Some(GOING),
                hidden: false,
            },
            Lane {
                option: Some(DECLINED),
                hidden: true,
            },
        ],
        card_fields: vec![],
        hide_empty_lanes: false,
    };
    assert_eq!(
        board.without_option(STATUS, GOING),
        ViewLayout::Board {
            group_by: STATUS,
            title: NAME,
            lanes: vec![Lane {
                option: Some(DECLINED),
                hidden: true,
            }],
            card_fields: vec![],
            hide_empty_lanes: false,
        }
    );
}

const FIRST: RowId = RowId::from_uuid(Uuid::from_u128(0x0001));
const SECOND: RowId = RowId::from_uuid(Uuid::from_u128(0x0002));
const THIRD: RowId = RowId::from_uuid(Uuid::from_u128(0x0003));
const FOURTH: RowId = RowId::from_uuid(Uuid::from_u128(0x0004));
const MOVED: RowId = RowId::from_uuid(Uuid::from_u128(0x0005));

#[test]
fn a_lane_shows_placed_cards_by_position_then_the_rest_by_creation() {
    let mut lane = vec![
        (FOURTH, None),
        (THIRD, Some("8180".parse::<Position>().unwrap())),
        (SECOND, None),
        (FIRST, Some("8280".parse::<Position>().unwrap())),
    ];
    arrange_lane(&mut lane);
    assert_eq!(
        lane,
        vec![
            (THIRD, Some("8180".parse::<Position>().unwrap())),
            (FIRST, Some("8280".parse::<Position>().unwrap())),
            (SECOND, None),
            (FOURTH, None),
        ]
    );
}

#[test]
fn a_card_lands_between_placed_neighbours_with_one_new_key() {
    let lane = vec![
        (FIRST, Some("80".parse::<Position>().unwrap())),
        (SECOND, Some("8180".parse::<Position>().unwrap())),
        (THIRD, None),
    ];

    assert_eq!(
        place_card(&lane, MOVED, Some(FIRST), Some(SECOND)),
        Ok(vec![(MOVED, "817f80".parse::<Position>().unwrap())])
    );
    assert_eq!(
        place_card(&lane, MOVED, None, Some(FIRST)),
        Ok(vec![(MOVED, "7f80".parse::<Position>().unwrap())])
    );
    assert_eq!(
        place_card(&lane, MOVED, Some(SECOND), None),
        Ok(vec![(MOVED, "8280".parse::<Position>().unwrap())])
    );
    assert_eq!(
        place_card(&[], MOVED, None, None),
        Ok(vec![(MOVED, "80".parse::<Position>().unwrap())])
    );
}

#[test]
fn a_card_landing_among_unplaced_cards_places_the_ones_before_it() {
    let lane = vec![
        (FIRST, Some("80".parse::<Position>().unwrap())),
        (SECOND, None),
        (THIRD, None),
        (FOURTH, None),
    ];

    assert_eq!(
        place_card(&lane, MOVED, Some(THIRD), Some(FOURTH)),
        Ok(vec![
            (SECOND, "817f80".parse::<Position>().unwrap()),
            (THIRD, "8180".parse::<Position>().unwrap()),
            (MOVED, "8280".parse::<Position>().unwrap()),
        ])
    );
    assert_eq!(
        place_card(&lane, MOVED, None, None),
        Ok(vec![
            (SECOND, "817e80".parse::<Position>().unwrap()),
            (THIRD, "817f80".parse::<Position>().unwrap()),
            (FOURTH, "8180".parse::<Position>().unwrap()),
            (MOVED, "8280".parse::<Position>().unwrap()),
        ])
    );
}

#[test]
fn a_neighbour_outside_the_lane_is_refused() {
    let lane = vec![(FIRST, Some("80".parse::<Position>().unwrap()))];
    assert_eq!(
        place_card(&lane, MOVED, Some(SECOND), None),
        Err(PlacementError::NotInLane(SECOND))
    );
}

#[test]
fn a_stored_view_crosses_the_wire_in_camel_case() {
    let created = Utc.with_ymd_and_hms(2026, 10, 1, 9, 0, 0).unwrap();
    let view = DatabaseView {
        id: ViewId::from_uuid(Uuid::from_u128(0x71e)),
        database_id: DatabaseId::from_uuid(Uuid::from_u128(0xdb)),
        table_id: TableId::from_uuid(Uuid::from_u128(0x7ab1)),
        name: "All".into(),
        position: "80".parse::<Position>().unwrap(),
        query: ViewQuery::default(),
        layout: ViewLayout::Table { columns: vec![] },
        created_at: created,
        updated_at: created,
    };
    assert_eq!(
        serde_json::to_value(&view).unwrap(),
        json!({
            "id": Uuid::from_u128(0x71e),
            "databaseId": Uuid::from_u128(0xdb),
            "tableId": Uuid::from_u128(0x7ab1),
            "name": "All",
            "position": "80",
            "query": {"filter": null, "sort": []},
            "layout": {"kind": "table", "columns": []},
            "createdAt": "2026-10-01T09:00:00Z",
            "updatedAt": "2026-10-01T09:00:00Z",
        })
    );
}

#[test]
fn neighbours_that_are_not_adjacent_are_refused() {
    let lane = vec![
        (FIRST, Some("80".parse::<Position>().unwrap())),
        (SECOND, Some("8180".parse::<Position>().unwrap())),
        (THIRD, Some("8280".parse::<Position>().unwrap())),
    ];
    assert_eq!(
        place_card(&lane, MOVED, Some(FIRST), Some(THIRD)),
        Err(PlacementError::NotAdjacent {
            before: FIRST,
            after: THIRD
        })
    );
    assert_eq!(
        place_card(&lane, MOVED, Some(SECOND), Some(FIRST)),
        Err(PlacementError::NotAdjacent {
            before: SECOND,
            after: FIRST
        })
    );
}
