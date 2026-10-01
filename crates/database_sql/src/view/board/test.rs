use models_databases::views::{
    CardPosition, Lane, SortDirection, SortKey, ViewLayout, ViewProblem, ViewQuery,
};
use uuid::Uuid;

use super::*;
use crate::fold::Cell;
use crate::run::{OutcomeColumn, OutcomeKind};
use crate::test_support::*;

const FIRST: Uuid = Uuid::from_u128(0x101);
const SECOND: Uuid = Uuid::from_u128(0x102);
const THIRD: Uuid = Uuid::from_u128(0x103);
const FOURTH: Uuid = Uuid::from_u128(0x104);
const FIFTH: Uuid = Uuid::from_u128(0x105);
const SIXTH: Uuid = Uuid::from_u128(0x106);
const NOT_AN_OPTION: Uuid = Uuid::from_u128(0x999);

fn board_layout(lanes: Vec<Lane>, hide_empty_lanes: bool) -> ViewLayout {
    ViewLayout::Board {
        group_by: STATUS_PLACEMENT,
        lanes,
        card_fields: vec![SUMMARY_PLACEMENT],
        hide_empty_lanes,
    }
}

/// The rows a view's read found, in its order: each row's status cell.
fn outcome(rows: &[(Uuid, Option<Cell>)]) -> Outcome {
    Outcome {
        columns: vec![
            OutcomeColumn {
                name: "summary".into(),
                column: Some(SUMMARY),
                kind: OutcomeKind::Text,
            },
            OutcomeColumn {
                name: "status".into(),
                column: Some(STATUS),
                kind: OutcomeKind::Select,
            },
        ],
        rows: rows
            .iter()
            .map(|(_, status)| vec![Some(Cell::Text("card".into())), status.clone()])
            .collect(),
        row_ids: rows.iter().map(|(row, _)| *row).collect(),
        read_tables: vec![ISSUES],
        ..Outcome::default()
    }
}

#[test]
fn cards_sit_in_their_lanes_in_hand_arranged_order() {
    let view = issues_view(
        ViewQuery::default(),
        ViewLayout::Board {
            group_by: STATUS_PLACEMENT,
            lanes: vec![Lane {
                option: Some(DOING),
                hidden: false,
            }],
            card_fields: vec![SUMMARY_PLACEMENT],
            hide_empty_lanes: true,
        },
    );
    let outcome = Outcome {
        columns: vec![
            OutcomeColumn {
                name: "summary".into(),
                column: Some(SUMMARY),
                kind: OutcomeKind::Text,
            },
            OutcomeColumn {
                name: "status".into(),
                column: Some(STATUS),
                kind: OutcomeKind::Select,
            },
        ],
        rows: vec![
            vec![
                Some(Cell::Text("Fix login".into())),
                Some(Cell::Options(vec![TODO])),
            ],
            vec![
                Some(Cell::Text("Write docs".into())),
                Some(Cell::Options(vec![DOING])),
            ],
            vec![Some(Cell::Text("Triage".into())), None],
            vec![
                Some(Cell::Text("Ship it".into())),
                Some(Cell::Options(vec![DOING])),
            ],
            vec![
                Some(Cell::Text("Plan".into())),
                Some(Cell::Options(vec![TODO])),
            ],
            vec![
                Some(Cell::Text("Orphan".into())),
                Some(Cell::Options(vec![NOT_AN_OPTION])),
            ],
        ],
        row_ids: vec![FIRST, SECOND, THIRD, FOURTH, FIFTH, SIXTH],
        read_tables: vec![ISSUES],
        ..Outcome::default()
    };
    let positions = vec![
        CardPosition {
            row: FOURTH,
            lane: Some(DOING),
            position: "a0".into(),
        },
        CardPosition {
            row: FIFTH,
            lane: Some(TODO),
            position: "a1".into(),
        },
        // Placed when it was in Doing; it has since moved to Todo.
        CardPosition {
            row: FIRST,
            lane: Some(DOING),
            position: "Zz".into(),
        },
    ];

    assert_eq!(
        board(&view, &issues_catalog(), &outcome, &positions),
        Ok(Board {
            lanes: vec![
                BoardLane {
                    option: Some(DOING),
                    hidden: false,
                    cards: vec![FOURTH, SECOND],
                },
                BoardLane {
                    option: None,
                    hidden: false,
                    cards: vec![THIRD, SIXTH],
                },
                BoardLane {
                    option: Some(TODO),
                    hidden: false,
                    cards: vec![FIFTH, FIRST],
                },
                BoardLane {
                    option: Some(WONT_DO),
                    hidden: true,
                    cards: vec![],
                },
            ],
        })
    );
}

#[test]
fn unpositioned_cards_follow_positioned_ones_by_row_id() {
    let view = issues_view(ViewQuery::default(), board_layout(vec![], false));
    let rows = outcome(&[
        (THIRD, Some(Cell::Options(vec![TODO]))),
        (SECOND, Some(Cell::Options(vec![TODO]))),
        (FIRST, Some(Cell::Options(vec![TODO]))),
        (FOURTH, Some(Cell::Options(vec![TODO]))),
    ]);
    let positions = vec![
        CardPosition {
            row: FOURTH,
            lane: Some(TODO),
            position: "a1".into(),
        },
        CardPosition {
            row: THIRD,
            lane: Some(TODO),
            position: "a0".into(),
        },
    ];

    let board = board(&view, &issues_catalog(), &rows, &positions).unwrap();

    assert_eq!(
        board.lanes[1],
        BoardLane {
            option: Some(TODO),
            hidden: false,
            cards: vec![THIRD, FOURTH, FIRST, SECOND],
        }
    );
}

#[test]
fn a_sorted_view_keeps_the_read_order_in_every_lane() {
    let view = issues_view(
        ViewQuery {
            filter: None,
            sort: vec![SortKey {
                column: SUMMARY_PLACEMENT,
                direction: SortDirection::Descending,
            }],
        },
        board_layout(vec![], false),
    );
    let rows = outcome(&[
        (THIRD, Some(Cell::Options(vec![TODO]))),
        (FIRST, Some(Cell::Options(vec![DOING]))),
        (SECOND, Some(Cell::Options(vec![TODO]))),
    ]);
    let positions = vec![CardPosition {
        row: SECOND,
        lane: Some(TODO),
        position: "a0".into(),
    }];

    assert_eq!(
        board(&view, &issues_catalog(), &rows, &positions),
        Ok(Board {
            lanes: vec![
                BoardLane {
                    option: None,
                    hidden: false,
                    cards: vec![],
                },
                BoardLane {
                    option: Some(TODO),
                    hidden: false,
                    cards: vec![THIRD, SECOND],
                },
                BoardLane {
                    option: Some(DOING),
                    hidden: false,
                    cards: vec![FIRST],
                },
                BoardLane {
                    option: Some(WONT_DO),
                    hidden: false,
                    cards: vec![],
                },
            ],
        })
    );
}

/// A lane's option and whether it is hidden.
type ShownLane = (Option<Uuid>, bool);

#[test]
fn listed_lanes_come_first_and_keep_their_hidden_flag() {
    let cases: Vec<(&str, Vec<Lane>, bool, Vec<ShownLane>)> = vec![
        (
            "nothing listed: no option first, then the column's order",
            vec![],
            false,
            vec![
                (None, false),
                (Some(TODO), false),
                (Some(DOING), false),
                (Some(WONT_DO), false),
            ],
        ),
        (
            "the no-option lane listed last stays last",
            vec![
                Lane {
                    option: Some(WONT_DO),
                    hidden: false,
                },
                Lane {
                    option: None,
                    hidden: false,
                },
            ],
            false,
            vec![
                (Some(WONT_DO), false),
                (None, false),
                (Some(TODO), false),
                (Some(DOING), false),
            ],
        ),
        (
            "a listed hidden lane is hidden even with cards",
            vec![Lane {
                option: Some(TODO),
                hidden: true,
            }],
            false,
            vec![
                (Some(TODO), true),
                (None, false),
                (Some(DOING), false),
                (Some(WONT_DO), false),
            ],
        ),
        (
            "hiding empty lanes hides every lane without cards, listed or not",
            vec![Lane {
                option: Some(DOING),
                hidden: false,
            }],
            true,
            vec![
                (Some(DOING), true),
                (None, true),
                (Some(TODO), false),
                (Some(WONT_DO), true),
            ],
        ),
    ];

    for (case, lanes, hide_empty_lanes, expected) in cases {
        let view = issues_view(ViewQuery::default(), board_layout(lanes, hide_empty_lanes));
        let rows = outcome(&[(FIRST, Some(Cell::Options(vec![TODO])))]);

        let board = board(&view, &issues_catalog(), &rows, &[]).unwrap();

        assert_eq!(
            board
                .lanes
                .iter()
                .map(|lane| (lane.option, lane.hidden))
                .collect::<Vec<_>>(),
            expected,
            "{case}"
        );
    }
}

#[test]
fn an_empty_options_cell_is_a_card_without_an_option() {
    let view = issues_view(ViewQuery::default(), board_layout(vec![], false));
    let rows = outcome(&[(FIRST, Some(Cell::Options(vec![])))]);

    let board = board(&view, &issues_catalog(), &rows, &[]).unwrap();

    assert_eq!(
        board.lanes[0],
        BoardLane {
            option: None,
            hidden: false,
            cards: vec![FIRST],
        }
    );
}

#[test]
fn only_a_board_view_of_a_visible_table_lays_out() {
    let table = issues_view(ViewQuery::default(), ViewLayout::Table { columns: vec![] });
    assert_eq!(
        board(&table, &issues_catalog(), &outcome(&[]), &[]),
        Err(ViewProblem::NotABoard)
    );

    let elsewhere = models_databases::views::DatabaseView {
        table_id: DEALS,
        ..issues_view(ViewQuery::default(), board_layout(vec![], false))
    };
    assert_eq!(
        board(&elsewhere, &issues_catalog(), &outcome(&[]), &[]),
        Err(ViewProblem::UnknownTable { table: DEALS })
    );

    let by_tags = issues_view(
        ViewQuery::default(),
        ViewLayout::Board {
            group_by: LABELS_PLACEMENT,
            lanes: vec![],
            card_fields: vec![],
            hide_empty_lanes: false,
        },
    );
    assert_eq!(
        board(&by_tags, &issues_catalog(), &outcome(&[]), &[]),
        Err(ViewProblem::BoardNeedsSingleSelect {
            column: "labels".into(),
        })
    );
}
