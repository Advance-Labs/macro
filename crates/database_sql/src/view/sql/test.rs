use chrono::{TimeZone, Utc};
use models_databases::views::{
    Conjunction, DateOperator, FilterCondition, FilterGroup, FilterNode, FilterTest,
    NumberOperator, PresenceOperator, SetOperator, SortDirection, SortKey, TextOperator,
    ViewLayout, ViewProblem, ViewQuery,
};

use super::*;
use crate::resolve::{Query, compile};
use crate::test_support::*;
use crate::view::compile_view;

const SAM: &str = "macro|sam@example.com";
const ANA: &str = "macro|o'neil@example.com";

/// The view `compile_view`'s literal test compiles.
fn open_work() -> ViewQuery {
    ViewQuery {
        filter: Some(FilterGroup {
            conjunction: Conjunction::And,
            conditions: vec![
                FilterNode::Condition(FilterCondition {
                    column: SUMMARY_PLACEMENT,
                    test: FilterTest::Text {
                        operator: TextOperator::Contains,
                        value: "50%_off\\".into(),
                    },
                }),
                FilterNode::Group(FilterGroup {
                    conjunction: Conjunction::Or,
                    conditions: vec![
                        FilterNode::Condition(FilterCondition {
                            column: STATUS_PLACEMENT,
                            test: FilterTest::Options {
                                operator: SetOperator::IsNoneOf,
                                options: vec![WONT_DO],
                            },
                        }),
                        FilterNode::Condition(FilterCondition {
                            column: POINTS_PLACEMENT,
                            test: FilterTest::Number {
                                operator: NumberOperator::GreaterThan,
                                value: 3.0,
                            },
                        }),
                    ],
                }),
            ],
        }),
        sort: vec![
            SortKey {
                column: DUE_PLACEMENT,
                direction: SortDirection::Descending,
            },
            SortKey {
                column: SUMMARY_PLACEMENT,
                direction: SortDirection::Ascending,
            },
        ],
    }
}

#[test]
fn a_view_reads_as_its_select() {
    let view = issues_view(open_work(), ViewLayout::Table { columns: vec![] });

    assert_eq!(
        view_as_sql(&view, &issues_catalog()).unwrap(),
        "SELECT * FROM \"work\".\"issues\" \
         WHERE \"summary\" LIKE '%50\\%\\_off\\\\%' ESCAPE '\\' \
         AND ((\"status\" NOT IN ('Won''t do') OR \"status\" IS NULL) OR \"points\" > 3) \
         ORDER BY \"due date\" DESC, \"summary\" ASC, row_position"
    );
}

#[test]
fn a_view_without_filter_or_sort_reads_as_the_whole_table() {
    let view = issues_view(ViewQuery::default(), ViewLayout::Table { columns: vec![] });

    assert_eq!(
        view_as_sql(&view, &issues_catalog()).unwrap(),
        "SELECT * FROM \"work\".\"issues\" ORDER BY row_position"
    );
}

#[test]
fn quotes_in_names_and_values_are_doubled() {
    let view = issues_view(
        ViewQuery {
            filter: Some(FilterGroup {
                conjunction: Conjunction::And,
                conditions: vec![FilterNode::Condition(FilterCondition {
                    column: SPEC_PLACEMENT,
                    test: FilterTest::Text {
                        operator: TextOperator::Is,
                        value: "it's".into(),
                    },
                })],
            }),
            sort: vec![],
        },
        ViewLayout::Table { columns: vec![] },
    );

    assert_eq!(
        view_as_sql(&view, &issues_catalog()).unwrap(),
        "SELECT * FROM \"work\".\"issues\" WHERE \"\"\"spec\"\" link\" = 'it''s' ORDER BY row_position"
    );
}

/// Whatever the view, its SQL compiles to the query the view compiles to.
#[test]
fn the_sql_compiles_back_to_the_view_query() {
    let due = Utc
        .with_ymd_and_hms(2026, 10, 1, 9, 30, 15)
        .unwrap()
        .checked_add_signed(chrono::Duration::milliseconds(250))
        .unwrap();
    let every_test = FilterGroup {
        conjunction: Conjunction::Or,
        conditions: vec![
            FilterNode::Condition(FilterCondition {
                column: DUE_PLACEMENT,
                test: FilterTest::Presence {
                    operator: PresenceOperator::IsEmpty,
                },
            }),
            FilterNode::Condition(FilterCondition {
                column: LABELS_PLACEMENT,
                test: FilterTest::Presence {
                    operator: PresenceOperator::IsNotEmpty,
                },
            }),
            FilterNode::Condition(FilterCondition {
                column: SUMMARY_PLACEMENT,
                test: FilterTest::Text {
                    operator: TextOperator::IsNot,
                    value: "Won't \"ship\"".into(),
                },
            }),
            FilterNode::Condition(FilterCondition {
                column: SUMMARY_PLACEMENT,
                test: FilterTest::Text {
                    operator: TextOperator::DoesNotContain,
                    value: "a_b%c\\".into(),
                },
            }),
            FilterNode::Condition(FilterCondition {
                column: SPEC_PLACEMENT,
                test: FilterTest::Text {
                    operator: TextOperator::StartsWith,
                    value: "https://".into(),
                },
            }),
            FilterNode::Condition(FilterCondition {
                column: SUMMARY_PLACEMENT,
                test: FilterTest::Text {
                    operator: TextOperator::EndsWith,
                    value: "'".into(),
                },
            }),
            FilterNode::Condition(FilterCondition {
                column: POINTS_PLACEMENT,
                test: FilterTest::Number {
                    operator: NumberOperator::IsNot,
                    value: -2.75,
                },
            }),
            FilterNode::Condition(FilterCondition {
                column: POINTS_PLACEMENT,
                test: FilterTest::Number {
                    operator: NumberOperator::LessThanOrEqual,
                    value: 0.1,
                },
            }),
            FilterNode::Condition(FilterCondition {
                column: POINTS_PLACEMENT,
                test: FilterTest::Number {
                    operator: NumberOperator::Is,
                    value: 1e21,
                },
            }),
            FilterNode::Condition(FilterCondition {
                column: DUE_PLACEMENT,
                test: FilterTest::Date {
                    operator: DateOperator::OnOrAfter,
                    value: due,
                },
            }),
            FilterNode::Condition(FilterCondition {
                column: BLOCKED_PLACEMENT,
                test: FilterTest::Checkbox { checked: false },
            }),
            FilterNode::Condition(FilterCondition {
                column: BLOCKED_PLACEMENT,
                test: FilterTest::Checkbox { checked: true },
            }),
            FilterNode::Condition(FilterCondition {
                column: STATUS_PLACEMENT,
                test: FilterTest::Options {
                    operator: SetOperator::IsAnyOf,
                    options: vec![WONT_DO, TODO],
                },
            }),
            FilterNode::Condition(FilterCondition {
                column: LABELS_PLACEMENT,
                test: FilterTest::Options {
                    operator: SetOperator::HasAll,
                    options: vec![BUG, FEATURE],
                },
            }),
            FilterNode::Condition(FilterCondition {
                column: LABELS_PLACEMENT,
                test: FilterTest::Options {
                    operator: SetOperator::HasNone,
                    options: vec![FEATURE],
                },
            }),
            FilterNode::Condition(FilterCondition {
                column: ASSIGNEE_PLACEMENT,
                test: FilterTest::Entities {
                    operator: SetOperator::IsNoneOf,
                    entities: vec![SAM.into(), ANA.into()],
                },
            }),
            FilterNode::Condition(FilterCondition {
                column: REVIEWERS_PLACEMENT,
                test: FilterTest::Entities {
                    operator: SetOperator::HasAny,
                    entities: vec![SAM.into(), ANA.into()],
                },
            }),
            FilterNode::Condition(FilterCondition {
                column: PARENT_PLACEMENT,
                test: FilterTest::Entities {
                    operator: SetOperator::IsAnyOf,
                    entities: vec!["00000000-0000-0000-0000-000000000101".into()],
                },
            }),
        ],
    };
    let nested = FilterGroup {
        conjunction: Conjunction::Or,
        conditions: vec![
            FilterNode::Group(FilterGroup {
                conjunction: Conjunction::And,
                conditions: vec![
                    FilterNode::Condition(FilterCondition {
                        column: BLOCKED_PLACEMENT,
                        test: FilterTest::Checkbox { checked: true },
                    }),
                    FilterNode::Group(FilterGroup {
                        conjunction: Conjunction::Or,
                        conditions: vec![
                            FilterNode::Condition(FilterCondition {
                                column: LABELS_PLACEMENT,
                                test: FilterTest::Options {
                                    operator: SetOperator::HasAny,
                                    options: vec![BUG, FEATURE],
                                },
                            }),
                            FilterNode::Condition(FilterCondition {
                                column: SUMMARY_PLACEMENT,
                                test: FilterTest::Text {
                                    operator: TextOperator::Contains,
                                    value: "or".into(),
                                },
                            }),
                        ],
                    }),
                ],
            }),
            FilterNode::Condition(FilterCondition {
                column: STATUS_PLACEMENT,
                test: FilterTest::Options {
                    operator: SetOperator::IsNoneOf,
                    options: vec![DOING],
                },
            }),
        ],
    };
    let views = vec![
        ViewQuery::default(),
        open_work(),
        ViewQuery {
            filter: Some(every_test),
            sort: vec![SortKey {
                column: STATUS_PLACEMENT,
                direction: SortDirection::Descending,
            }],
        },
        ViewQuery {
            filter: Some(nested),
            sort: vec![
                SortKey {
                    column: SPEC_PLACEMENT,
                    direction: SortDirection::Ascending,
                },
                SortKey {
                    column: REVIEWERS_PLACEMENT,
                    direction: SortDirection::Descending,
                },
            ],
        },
        ViewQuery {
            filter: Some(FilterGroup {
                conjunction: Conjunction::And,
                conditions: vec![FilterNode::Condition(FilterCondition {
                    column: SUMMARY_PLACEMENT,
                    test: FilterTest::Text {
                        operator: TextOperator::IsNot,
                        value: "x".into(),
                    },
                })],
            }),
            sort: vec![],
        },
    ];

    for query in views {
        let view = issues_view(query, ViewLayout::Table { columns: vec![] });
        let sql = view_as_sql(&view, &issues_catalog()).unwrap();
        assert_eq!(
            compile(&issues_catalog(), &sql),
            Ok(Query::Select(
                compile_view(&view, &issues_catalog()).unwrap()
            )),
            "{sql}"
        );
    }
}

#[test]
fn a_view_of_a_table_out_of_sight_is_refused() {
    let view = models_databases::views::DatabaseView {
        table_id: DEALS,
        ..issues_view(ViewQuery::default(), ViewLayout::Table { columns: vec![] })
    };

    assert_eq!(
        view_as_sql(&view, &issues_catalog()),
        Err(ViewProblem::UnknownTable { table: DEALS })
    );
}
