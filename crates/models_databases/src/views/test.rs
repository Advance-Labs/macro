use chrono::TimeZone;
use serde_json::json;

use super::*;

const NAME: Uuid = Uuid::from_u128(0xc01a);
const STATUS: Uuid = Uuid::from_u128(0xc01b);
const DIET: Uuid = Uuid::from_u128(0xc01c);
const PLUS_ONES: Uuid = Uuid::from_u128(0xc01d);
const GOING: Uuid = Uuid::from_u128(0x0b1);
const DECLINED: Uuid = Uuid::from_u128(0x0b2);
const VEGAN: Uuid = Uuid::from_u128(0x0b3);

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

fn table() -> ViewLayout {
    ViewLayout::Table { columns: vec![] }
}

fn filtered(conditions: Vec<FilterNode>) -> ViewQuery {
    ViewQuery {
        filter: Some(FilterGroup {
            conjunction: Conjunction::And,
            conditions,
        }),
        sort: vec![],
    }
}

fn condition(column: ColumnId, test: FilterTest) -> FilterNode {
    FilterNode::Condition(FilterCondition { column, test })
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
                        condition(
                            STATUS,
                            FilterTest::Options {
                                operator: SetOperator::IsAnyOf,
                                options: vec![GOING],
                            }
                        ),
                        FilterNode::Group(FilterGroup {
                            conjunction: Conjunction::And,
                            conditions: vec![condition(
                                PLUS_ONES,
                                FilterTest::Number {
                                    operator: NumberOperator::GreaterThan,
                                    value: 1.0,
                                }
                            )],
                        }),
                    ],
                }),
                sort: vec![SortKey {
                    column: NAME,
                    direction: SortDirection::Descending,
                }],
            },
            layout: ViewLayout::Board {
                group_by: STATUS,
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
                condition(
                    NAME,
                    FilterTest::Text {
                        operator: TextOperator::Contains,
                        value: "sam".into(),
                    },
                ),
                condition(
                    DIET,
                    FilterTest::Options {
                        operator: SetOperator::HasNone,
                        options: vec![VEGAN],
                    },
                ),
                condition(
                    PLUS_ONES,
                    FilterTest::Presence {
                        operator: PresenceOperator::IsNotEmpty,
                    },
                ),
            ],
        }),
        sort: vec![SortKey {
            column: PLUS_ONES,
            direction: SortDirection::Ascending,
        }],
    };
    let board = ViewLayout::Board {
        group_by: STATUS,
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
    let ghost = Uuid::from_u128(0x6057);
    let cases = [
        (
            filtered(vec![condition(
                ghost,
                FilterTest::Presence {
                    operator: PresenceOperator::IsEmpty,
                },
            )]),
            table(),
            ViewProblem::UnknownColumn { column: ghost },
            "no column 00000000-0000-0000-0000-000000006057 in this table",
        ),
        (
            filtered(vec![condition(
                PLUS_ONES,
                FilterTest::Text {
                    operator: TextOperator::Is,
                    value: "two".into(),
                },
            )]),
            table(),
            ViewProblem::TestDoesNotFit {
                column: "Plus ones".into(),
                holds: ValueKind::Number,
                test: ValueKind::Text,
            },
            "\"Plus ones\" holds number values; a text test does not fit it",
        ),
        (
            filtered(vec![condition(
                DIET,
                FilterTest::Options {
                    operator: SetOperator::IsAnyOf,
                    options: vec![VEGAN],
                },
            )]),
            table(),
            ViewProblem::OperatorDoesNotFit {
                column: "Diet".into(),
                multi: true,
            },
            "\"Diet\" holds several values; test it with hasAny, hasAll or hasNone",
        ),
        (
            filtered(vec![condition(
                STATUS,
                FilterTest::Options {
                    operator: SetOperator::HasAll,
                    options: vec![GOING],
                },
            )]),
            table(),
            ViewProblem::OperatorDoesNotFit {
                column: "Status".into(),
                multi: false,
            },
            "\"Status\" holds one value; test it with isAnyOf or isNoneOf",
        ),
        (
            filtered(vec![condition(
                STATUS,
                FilterTest::Options {
                    operator: SetOperator::IsNoneOf,
                    options: vec![],
                },
            )]),
            table(),
            ViewProblem::NothingToMatch {
                column: "Status".into(),
            },
            "a test of \"Status\" names nothing to match",
        ),
        (
            filtered(vec![condition(
                STATUS,
                FilterTest::Options {
                    operator: SetOperator::IsAnyOf,
                    options: vec![VEGAN],
                },
            )]),
            table(),
            ViewProblem::UnknownOption {
                column: "Status".into(),
                option: VEGAN,
            },
            "no option 00000000-0000-0000-0000-0000000000b3 on \"Status\"",
        ),
        (
            filtered(vec![condition(
                PLUS_ONES,
                FilterTest::Number {
                    operator: NumberOperator::LessThan,
                    value: f64::INFINITY,
                },
            )]),
            table(),
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
            table(),
            ViewProblem::RepeatedColumn {
                column: "Name".into(),
            },
            "\"Name\" is listed twice",
        ),
        (
            ViewQuery::default(),
            ViewLayout::Board {
                group_by: DIET,
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
                condition(
                    NAME,
                    FilterTest::Text {
                        operator: TextOperator::StartsWith,
                        value: "S".into(),
                    },
                ),
                FilterNode::Group(FilterGroup {
                    conjunction: Conjunction::And,
                    conditions: vec![condition(
                        PLUS_ONES,
                        FilterTest::Number {
                            operator: NumberOperator::Is,
                            value: 2.0,
                        },
                    )],
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
                conditions: vec![condition(
                    NAME,
                    FilterTest::Text {
                        operator: TextOperator::StartsWith,
                        value: "S".into(),
                    },
                )],
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
        lanes: vec![],
        card_fields: vec![NAME, PLUS_ONES],
        hide_empty_lanes: false,
    };
    assert_eq!(
        board.without_column(PLUS_ONES),
        Some(ViewLayout::Board {
            group_by: STATUS,
            lanes: vec![],
            card_fields: vec![NAME],
            hide_empty_lanes: false,
        })
    );
    assert_eq!(board.without_column(STATUS), None);
    assert_eq!(
        ViewLayout::Table {
            columns: vec![
                ViewColumn {
                    column: NAME,
                    width: Some(240),
                    hidden: false,
                },
                ViewColumn {
                    column: PLUS_ONES,
                    width: None,
                    hidden: true,
                },
            ],
        }
        .without_column(NAME),
        Some(ViewLayout::Table {
            columns: vec![ViewColumn {
                column: PLUS_ONES,
                width: None,
                hidden: true,
            }],
        })
    );
}

#[test]
fn removing_an_option_drops_it_from_tests_and_lanes() {
    let query = filtered(vec![
        condition(
            STATUS,
            FilterTest::Options {
                operator: SetOperator::IsNoneOf,
                options: vec![GOING, DECLINED],
            },
        ),
        condition(
            STATUS,
            FilterTest::Options {
                operator: SetOperator::IsAnyOf,
                options: vec![GOING],
            },
        ),
    ]);

    assert_eq!(
        query.without_option(STATUS, GOING),
        filtered(vec![condition(
            STATUS,
            FilterTest::Options {
                operator: SetOperator::IsNoneOf,
                options: vec![DECLINED],
            },
        )])
    );
    assert_eq!(query.without_option(DIET, GOING), query);

    let board = ViewLayout::Board {
        group_by: STATUS,
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
            lanes: vec![Lane {
                option: Some(DECLINED),
                hidden: true,
            }],
            card_fields: vec![],
            hide_empty_lanes: false,
        }
    );
}

const FIRST: Uuid = Uuid::from_u128(0x0001);
const SECOND: Uuid = Uuid::from_u128(0x0002);
const THIRD: Uuid = Uuid::from_u128(0x0003);
const FOURTH: Uuid = Uuid::from_u128(0x0004);
const MOVED: Uuid = Uuid::from_u128(0x0005);

#[test]
fn a_lane_shows_placed_cards_by_position_then_the_rest_by_creation() {
    let mut lane = vec![
        (FOURTH, None),
        (THIRD, Some("8180".to_string())),
        (SECOND, None),
        (FIRST, Some("8280".to_string())),
    ];
    arrange_lane(&mut lane);
    assert_eq!(
        lane,
        vec![
            (THIRD, Some("8180".to_string())),
            (FIRST, Some("8280".to_string())),
            (SECOND, None),
            (FOURTH, None),
        ]
    );
}

#[test]
fn a_card_lands_between_placed_neighbours_with_one_new_key() {
    let lane = vec![
        (FIRST, Some("80".to_string())),
        (SECOND, Some("8180".to_string())),
        (THIRD, None),
    ];

    assert_eq!(
        place_card(&lane, MOVED, Some(FIRST), Some(SECOND)),
        Ok(vec![(MOVED, "817f80".to_string())])
    );
    assert_eq!(
        place_card(&lane, MOVED, None, Some(FIRST)),
        Ok(vec![(MOVED, "7f80".to_string())])
    );
    assert_eq!(
        place_card(&lane, MOVED, Some(SECOND), None),
        Ok(vec![(MOVED, "8280".to_string())])
    );
    assert_eq!(
        place_card(&[], MOVED, None, None),
        Ok(vec![(MOVED, "80".to_string())])
    );
}

#[test]
fn a_card_landing_among_unplaced_cards_places_the_ones_before_it() {
    let lane = vec![
        (FIRST, Some("80".to_string())),
        (SECOND, None),
        (THIRD, None),
        (FOURTH, None),
    ];

    assert_eq!(
        place_card(&lane, MOVED, Some(THIRD), Some(FOURTH)),
        Ok(vec![
            (SECOND, "817f80".to_string()),
            (THIRD, "8180".to_string()),
            (MOVED, "8280".to_string()),
        ])
    );
    assert_eq!(
        place_card(&lane, MOVED, None, None),
        Ok(vec![
            (SECOND, "817e80".to_string()),
            (THIRD, "817f80".to_string()),
            (FOURTH, "8180".to_string()),
            (MOVED, "8280".to_string()),
        ])
    );
}

#[test]
fn a_neighbour_outside_the_lane_is_refused() {
    let lane = vec![(FIRST, Some("80".to_string()))];
    assert_eq!(
        place_card(&lane, MOVED, Some(SECOND), None),
        Err(PlacementError::NotInLane(SECOND))
    );
}

#[test]
fn a_stored_view_crosses_the_wire_in_camel_case() {
    let created = Utc.with_ymd_and_hms(2026, 10, 1, 9, 0, 0).unwrap();
    let view = DatabaseView {
        id: Uuid::from_u128(0x71e),
        database_id: Uuid::from_u128(0xdb),
        table_id: Uuid::from_u128(0x7ab1),
        name: "All".into(),
        position: "80".into(),
        query: ViewQuery::default(),
        layout: table(),
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
