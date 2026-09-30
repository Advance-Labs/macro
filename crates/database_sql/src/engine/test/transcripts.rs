//! Transcripts of the engine answering four statements, checked in as JSON
//! under `fixtures/transcripts/`. The browser driver's tests replay them
//! against its GraphQL source, so both sides agree on what crosses the wasm
//! boundary: every step the engine takes and the page or bins fed back.

use std::collections::HashMap;

use serde_json::{Value, json};

use super::*;
use crate::catalog::{Column, ColumnKind, SelectOption, Table, TableSource};

const DEALS: Uuid = Uuid::from_u128(0x01990000_0000_7000_8000_00000000d001);
const PEOPLE: Uuid = Uuid::from_u128(0x01990000_0000_7000_8000_00000000d002);
const NAME: Uuid = Uuid::from_u128(0x01990000_0000_7000_8000_00000000c001);
const AMOUNT: Uuid = Uuid::from_u128(0x01990000_0000_7000_8000_00000000c002);
const STAGE: Uuid = Uuid::from_u128(0x01990000_0000_7000_8000_00000000c003);
const OWNER: Uuid = Uuid::from_u128(0x01990000_0000_7000_8000_00000000c004);
const LEAD: Uuid = Uuid::from_u128(0x01990000_0000_7000_8000_00000000a001);
const WON: Uuid = Uuid::from_u128(0x01990000_0000_7000_8000_00000000a002);
const ACME: Uuid = Uuid::from_u128(0x01990000_0000_7000_8000_00000000e001);
const GLOBEX: Uuid = Uuid::from_u128(0x01990000_0000_7000_8000_00000000e002);
const INITECH: Uuid = Uuid::from_u128(0x01990000_0000_7000_8000_00000000e003);
const SAM: Uuid = Uuid::from_u128(0x01990000_0000_7000_8000_00000000f001);

/// `crm.deals` (a text, a number, a single select, a relation to people)
/// and `crm.people`, sharing the `name` definition, as the browser builds
/// the catalog from a database's detail.
fn crm() -> Catalog {
    Catalog {
        tables: vec![
            Table {
                id: DEALS,
                database: "crm".into(),
                name: "deals".into(),
                columns: vec![
                    Column {
                        id: NAME,
                        name: "name".into(),
                        kind: ColumnKind::Text,
                    },
                    Column {
                        id: AMOUNT,
                        name: "amount".into(),
                        kind: ColumnKind::Number,
                    },
                    Column {
                        id: STAGE,
                        name: "stage".into(),
                        kind: ColumnKind::Select {
                            multi: false,
                            options: vec![
                                SelectOption {
                                    id: LEAD,
                                    label: "Lead".into(),
                                },
                                SelectOption {
                                    id: WON,
                                    label: "Won".into(),
                                },
                            ],
                        },
                    },
                    Column {
                        id: OWNER,
                        name: "owner".into(),
                        kind: ColumnKind::Entity { multi: true },
                    },
                ],
                source: TableSource::Database,
            },
            Table {
                id: PEOPLE,
                database: "crm".into(),
                name: "people".into(),
                columns: vec![Column {
                    id: NAME,
                    name: "name".into(),
                    kind: ColumnKind::Text,
                }],
                source: TableSource::Database,
            },
        ],
    }
}

/// What the driver feeds back for one step.
enum Feed {
    Page(Page),
    Bins(Vec<Bin>),
}

/// Run `sql`, answering the engine's steps in order with `feeds`.
fn transcript(sql: &str, feeds: Vec<Feed>) -> Value {
    let catalog = crm();
    let (mut engine, mut step) = Engine::start(&catalog, sql).unwrap();
    let mut exchanges = Vec::new();
    for feed in feeds {
        let next = match (&step, &feed) {
            (Step::Fetch(request), Feed::Page(page)) => {
                engine.feed_page(request.id, page.clone()).unwrap()
            }
            (Step::Bins(request), Feed::Bins(bins)) => {
                engine.feed_bins(request.id, bins.clone()).unwrap()
            }
            (step, _) => panic!("the engine asked for {step:?}"),
        };
        exchanges.push(match feed {
            Feed::Page(page) => json!({ "step": step, "page": page }),
            Feed::Bins(bins) => json!({ "step": step, "bins": bins }),
        });
        step = next;
    }
    let Step::Done(outcome) = step else {
        panic!("the engine still wants {step:?}")
    };
    json!({ "catalog": catalog, "sql": sql, "exchanges": exchanges, "outcome": outcome })
}

fn fixture(name: &str) -> Value {
    let path = format!(
        "{}/fixtures/transcripts/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn deal(id: Uuid, name: &str, amount: f64, stage: Uuid, owner: Option<Uuid>) -> Row {
    let mut cells = HashMap::from([
        (NAME, Cell::Text(name.into())),
        (AMOUNT, Cell::Number(amount)),
        (STAGE, Cell::Options(vec![stage])),
    ]);
    if let Some(owner) = owner {
        cells.insert(OWNER, Cell::Entities(vec![owner.to_string()]));
    }
    Row { id, cells }
}

#[test]
fn a_select_column_filter_is_one_soup_query() {
    assert_eq!(
        transcript(
            "SELECT name, amount FROM crm.deals WHERE stage = 'Won' ORDER BY amount DESC",
            vec![Feed::Page(Page {
                rows: vec![
                    deal(ACME, "Acme", 12000.0, WON, Some(SAM)),
                    deal(GLOBEX, "Globex", 50000.0, WON, None),
                ],
                next: None,
            })],
        ),
        fixture("select-column-filter")
    );
}

#[test]
fn a_count_per_select_option_is_answered_by_bins() {
    assert_eq!(
        transcript(
            "SELECT stage, COUNT(*) FROM crm.deals GROUP BY stage",
            vec![Feed::Bins(vec![
                Bin {
                    key: Some(Cell::Options(vec![WON])),
                    count: 2,
                },
                Bin {
                    key: Some(Cell::Options(vec![LEAD])),
                    count: 1,
                },
                Bin {
                    key: None,
                    count: 4,
                },
            ])],
        ),
        fixture("count-per-option")
    );
}

#[test]
fn a_join_asks_for_the_joined_rows_it_needs() {
    assert_eq!(
        transcript(
            "SELECT d.name, p.name FROM crm.deals d JOIN crm.people p ON d.owner = p.row_id",
            vec![
                Feed::Page(Page {
                    rows: vec![
                        deal(ACME, "Acme", 12000.0, WON, Some(SAM)),
                        deal(INITECH, "Initech", 300.0, LEAD, None),
                    ],
                    next: None,
                }),
                Feed::Page(Page {
                    rows: vec![Row {
                        id: SAM,
                        cells: HashMap::from([(NAME, Cell::Text("Sam".into()))]),
                    }],
                    next: None,
                }),
            ],
        ),
        fixture("join")
    );
}

#[test]
fn pages_follow_the_cursor_to_the_end() {
    assert_eq!(
        transcript(
            "SELECT name FROM crm.deals",
            vec![
                Feed::Page(Page {
                    rows: vec![deal(ACME, "Acme", 12000.0, WON, Some(SAM))],
                    next: Some("second-page".into()),
                }),
                Feed::Page(Page {
                    rows: vec![deal(GLOBEX, "Globex", 50000.0, WON, None)],
                    next: None,
                }),
            ],
        ),
        fixture("paging")
    );
}
