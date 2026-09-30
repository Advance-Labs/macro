use std::collections::HashMap;

mod transcripts;

use filter_ast::Expr;
use item_filters::ast::properties::{PropertiesLiteral, PropertyMatchValue};

use super::*;
use crate::catalog::{PEOPLE_EMAIL, PEOPLE_ID, PEOPLE_NAME, PEOPLE_TABLE};
use crate::fold::Cell;
use crate::resolve::{Value, column_key};
use crate::run::{OutcomeKind, RowSource, RowWriter, SourceError, WriteError, run};
use crate::test_support::{catalog, *};

const FIX_LOGIN: Uuid = Uuid::from_u128(0xe1);
const WRITE_DOCS: Uuid = Uuid::from_u128(0xe2);
const SHIP_IT: Uuid = Uuid::from_u128(0xe3);
const IDLE: Uuid = Uuid::from_u128(0xe4);
const ACME: Uuid = Uuid::from_u128(0xa1);
const GLOBEX: Uuid = Uuid::from_u128(0xa2);
const SAM: &str = "macro|sam@example.com";
const ANA: &str = "macro|ana@example.com";
const KIM: &str = "macro|kim@example.com";

fn task(id: Uuid, title: &str, priority: Uuid, assignees: &[&str], deal: Option<Uuid>) -> Row {
    let mut cells = HashMap::from([
        (TITLE, Cell::Text(title.into())),
        (PRIORITY, Cell::Options(vec![priority])),
        (
            ASSIGNEES,
            Cell::Entities(assignees.iter().map(|id| id.to_string()).collect()),
        ),
    ]);
    if let Some(deal) = deal {
        cells.insert(DEAL, Cell::Entities(vec![deal.to_string()]));
    }
    Row { id, cells }
}

/// Sam and Ana share the high-priority login fix; Kim has a low one; the
/// idle task has no one and no deal.
fn tasks() -> Vec<Row> {
    vec![
        task(FIX_LOGIN, "Fix login", HIGH, &[SAM, ANA], Some(ACME)),
        task(WRITE_DOCS, "Write docs", LOW, &[KIM], None),
        task(SHIP_IT, "Ship it", HIGH, &[SAM], Some(GLOBEX)),
        task(IDLE, "Idle", HIGH, &[], None),
    ]
}

/// A person as the driver would key them for the second relation.
fn person(id: &str, name: &str, email: &str) -> Row {
    Row {
        id: Uuid::new_v5(&Uuid::NAMESPACE_OID, id.as_bytes()),
        cells: HashMap::from([
            (column_key(1, PEOPLE_ID), Cell::Entities(vec![id.into()])),
            (column_key(1, PEOPLE_NAME), Cell::Text(name.into())),
            (column_key(1, PEOPLE_EMAIL), Cell::Text(email.into())),
        ]),
    }
}

fn people() -> Vec<Row> {
    vec![
        person(SAM, "Sam", "sam@example.com"),
        person(ANA, "Ana", "ana@example.com"),
        person(KIM, "Kim", "kim@example.com"),
    ]
}

/// Deals keyed as the second relation.
fn deals() -> Vec<Row> {
    vec![
        Row {
            id: ACME,
            cells: HashMap::from([
                (column_key(1, NAME), Cell::Text("Acme".into())),
                (column_key(1, AMOUNT), Cell::Number(12000.0)),
            ]),
        },
        Row {
            id: GLOBEX,
            cells: HashMap::from([
                (column_key(1, NAME), Cell::Text("Globex".into())),
                (column_key(1, AMOUNT), Cell::Number(50.0)),
            ]),
        },
    ]
}

/// Serve every request from `serve`, one page each, noting each request.
fn drive(
    catalog: &Catalog,
    sql: &str,
    serve: impl Fn(&GqlQuery) -> Vec<Row>,
) -> (Outcome, Vec<Request>) {
    let (mut engine, mut step) = Engine::start(catalog, sql).unwrap();
    let mut requests = Vec::new();
    loop {
        step = match step {
            Step::Done(outcome) => return (outcome, requests),
            Step::Fetch(request) => {
                let rows = serve(&request.query);
                let id = request.id;
                requests.push(request);
                engine.feed_page(id, Page { rows, next: None }).unwrap()
            }
            Step::Bins(_) => panic!("no bins here"),
        };
    }
}

/// Serve each table's rows, applying the pushed-down `propf` as Soup would.
fn by_table(query: &GqlQuery) -> Vec<Row> {
    let (rows, propf) = match query {
        GqlQuery::Soup { table, propf, .. } if *table == TASKS => (tasks(), propf),
        GqlQuery::Soup { table, propf, .. } if *table == DEALS => (deals(), propf),
        GqlQuery::People { .. } => return people(),
        other => panic!("unexpected {other:?}"),
    };
    rows.into_iter()
        .filter(|row| propf.as_ref().is_none_or(|expr| soup_matches(expr, row)))
        .collect()
}

fn soup_matches(expr: &Expr<PropertiesLiteral>, row: &Row) -> bool {
    match expr {
        Expr::And(a, b) => soup_matches(a, row) && soup_matches(b, row),
        Expr::Or(a, b) => soup_matches(a, row) || soup_matches(b, row),
        Expr::Not(a) => !soup_matches(a, row),
        Expr::Literal(literal) => match (
            row.cells.get(&literal.property_definition_id),
            &literal.value,
        ) {
            (Some(Cell::Options(ids)), PropertyMatchValue::SelectOption(id)) => ids.contains(id),
            (Some(Cell::Entities(ids)), PropertyMatchValue::EntityRef(id)) => {
                ids.iter().any(|candidate| candidate == &id.to_string())
            }
            _ => false,
        },
    }
}

fn text(value: &str) -> Option<Cell> {
    Some(Cell::Text(value.into()))
}

// ---- the question that motivated joins ---------------------------------------

#[test]
fn emails_of_people_with_high_priority_tasks() {
    let (outcome, requests) = drive(
        &catalog(),
        "SELECT DISTINCT p.email FROM macro.tasks t JOIN macro.people p ON t.assignees = p.id
         WHERE t.priority = 'High' ORDER BY p.email",
        by_table,
    );

    assert_eq!(
        outcome.columns,
        vec![OutcomeColumn {
            name: "email".into(),
            column: Some(PEOPLE_EMAIL),
            kind: OutcomeKind::Text,
        }]
    );
    // Sam is on two high tasks; DISTINCT keeps one. Kim's task is low.
    assert_eq!(
        outcome.rows,
        vec![vec![text("ana@example.com")], vec![text("sam@example.com")]]
    );
    assert_eq!(outcome.row_ids, vec![FIX_LOGIN, FIX_LOGIN]);
    assert_eq!(outcome.read_tables, vec![TASKS, PEOPLE_TABLE]);
    assert!(!outcome.truncated);

    // Tasks first (Soup applied the priority), then people narrowed to the
    // assignees of the high-priority tasks.
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].id, 0);
    assert!(matches!(
        &requests[0].query,
        GqlQuery::Soup { table, propf: Some(_), key_hint: None } if *table == TASKS
    ));
    assert_eq!(requests[0].needs, vec![ASSIGNEES]);
    assert_eq!(requests[1].id, 1);
    assert_eq!(
        requests[1].query,
        GqlQuery::People {
            ids: Some(vec![SAM.into(), ANA.into()]),
        }
    );
    assert_eq!(
        requests[1].needs,
        vec![column_key(1, PEOPLE_EMAIL), column_key(1, PEOPLE_ID)]
    );
}

#[test]
fn without_distinct_each_assignment_is_a_row() {
    let (outcome, _) = drive(
        &catalog(),
        "SELECT t.title, p.name FROM macro.tasks t JOIN macro.people p ON t.assignees = p.id
         ORDER BY t.title, p.name",
        by_table,
    );
    assert_eq!(
        outcome.rows,
        vec![
            vec![text("Fix login"), text("Ana")],
            vec![text("Fix login"), text("Sam")],
            vec![text("Ship it"), text("Sam")],
            vec![text("Write docs"), text("Kim")],
        ]
    );
    assert_eq!(
        outcome.row_ids,
        vec![FIX_LOGIN, FIX_LOGIN, SHIP_IT, WRITE_DOCS]
    );
}

// ---- join kinds ---------------------------------------------------------------

#[test]
fn left_join_on_row_id_keeps_tasks_without_a_deal() {
    let (outcome, requests) = drive(
        &catalog(),
        "SELECT t.title, d.name, d.amount FROM macro.tasks t LEFT JOIN crm.deals d ON t.deal = d.row_id
         WHERE d.amount > 100 OR d.amount IS NULL ORDER BY t.title",
        by_table,
    );
    assert_eq!(
        outcome.rows,
        vec![
            vec![text("Fix login"), text("Acme"), Some(Cell::Number(12000.0))],
            vec![text("Idle"), None, None],
            vec![text("Write docs"), None, None],
        ]
    );
    // The hint names the row id (no property) with the deals the tasks link.
    assert_eq!(
        requests[1].query,
        GqlQuery::Soup {
            table: DEALS,
            propf: None,
            key_hint: Some(KeyHint {
                column: None,
                values: vec![
                    Cell::Entities(vec![ACME.to_string()]),
                    Cell::Entities(vec![GLOBEX.to_string()]),
                ],
            }),
        }
    );
}

#[test]
fn inner_join_drops_tasks_without_a_match_and_counts_joined_rows() {
    let (outcome, _) = drive(
        &catalog(),
        "SELECT t.priority, COUNT(*) FROM macro.tasks t JOIN macro.people p ON t.assignees = p.id
         GROUP BY t.priority ORDER BY 2 DESC",
        by_table,
    );
    // High: Sam and Ana on one task, Sam on another, three joined rows;
    // Low: Kim, one. The idle task has no assignee and is gone.
    assert_eq!(
        outcome.rows,
        vec![
            vec![Some(Cell::Options(vec![HIGH])), Some(Cell::Number(3.0))],
            vec![Some(Cell::Options(vec![LOW])), Some(Cell::Number(1.0))],
        ]
    );
    assert_eq!(outcome.row_ids, Vec::<Uuid>::new());
}

#[test]
fn a_joined_tables_rows_arrive_keyed_by_definition_and_the_engine_keys_them() {
    // A source serves every table the same way, cells by property
    // definition; the engine gives the joined relation its own keys, so the
    // people row's `name` cannot overwrite the deal's.
    let (outcome, requests) = drive(
        &catalog(),
        "SELECT d.name, p.name FROM crm.deals d JOIN crm.people p ON d.owner = p.row_id",
        |query| match query {
            GqlQuery::Soup { table, .. } if *table == DEALS => vec![Row {
                id: ACME,
                cells: HashMap::from([
                    (NAME, Cell::Text("Acme".into())),
                    (
                        OWNER,
                        Cell::Entities(vec![Uuid::from_u128(0x99).to_string()]),
                    ),
                ]),
            }],
            GqlQuery::Soup { table, .. } if *table == PEOPLE => vec![Row {
                id: Uuid::from_u128(0x99),
                cells: HashMap::from([(NAME, Cell::Text("Sam".into()))]),
            }],
            other => panic!("unexpected {other:?}"),
        },
    );
    assert_eq!(outcome.rows, vec![vec![text("Acme"), text("Sam")]]);
    assert_eq!(
        requests[1].needs,
        vec![column_key(1, NAME), crate::resolve::row_id_key(PEOPLE)]
    );
}

#[test]
fn a_definition_shared_by_both_tables_keeps_its_two_columns_apart() {
    // crm.deals.name and crm.people.name are one definition; joined, each
    // side's value must survive under its own key.
    let (outcome, _) = drive(
        &catalog(),
        "SELECT d.name, p.name FROM crm.deals d JOIN crm.people p ON d.owner = p.row_id",
        |query| match query {
            GqlQuery::Soup { table, .. } if *table == DEALS => vec![Row {
                id: ACME,
                cells: HashMap::from([
                    (NAME, Cell::Text("Acme".into())),
                    (
                        OWNER,
                        Cell::Entities(vec![Uuid::from_u128(0x99).to_string()]),
                    ),
                ]),
            }],
            GqlQuery::Soup { table, .. } if *table == PEOPLE => vec![Row {
                id: Uuid::from_u128(0x99),
                cells: HashMap::from([(column_key(1, NAME), Cell::Text("Sam".into()))]),
            }],
            other => panic!("unexpected {other:?}"),
        },
    );
    assert_eq!(outcome.rows, vec![vec![text("Acme"), text("Sam")]]);
    assert_eq!(
        outcome
            .columns
            .iter()
            .map(|column| column.column)
            .collect::<Vec<_>>(),
        vec![Some(NAME), Some(NAME)]
    );
}

// ---- the protocol -------------------------------------------------------------

#[test]
fn pages_continue_by_cursor_and_ids_must_match() {
    let (mut engine, step) = Engine::start(
        &catalog(),
        "SELECT t.title, p.name FROM macro.tasks t JOIN macro.people p ON t.assignees = p.id",
    )
    .unwrap();
    let Step::Fetch(first) = step else {
        panic!("expected a fetch");
    };
    assert_eq!(
        (first.id, first.cursor.clone(), first.limit),
        (0, None, PAGE_LIMIT)
    );

    // Page one of the tasks continues at its cursor.
    let step = engine
        .feed_page(
            0,
            Page {
                rows: tasks().into_iter().take(2).collect(),
                next: Some("2".into()),
            },
        )
        .unwrap();
    let Step::Fetch(second) = step else {
        panic!("expected a fetch");
    };
    assert_eq!(second.id, 1);
    assert_eq!(second.cursor, Some("2".into()));
    assert_eq!(second.query, first.query);

    // A wrong id is refused; the outstanding request is spent either way.
    assert_eq!(
        engine
            .feed_page(7, Page::default())
            .unwrap_err()
            .to_string(),
        "fed request 7, but request 1 is outstanding"
    );
    assert_eq!(
        engine
            .feed_page(1, Page::default())
            .unwrap_err()
            .to_string(),
        "fed request 1, but nothing is outstanding"
    );
}

#[test]
fn a_relation_past_the_cap_is_truncated_and_the_next_still_fetched() {
    let many: Vec<Row> = (0..ROW_CAP + 5)
        .map(|i| task(Uuid::from_u128(0x1000 + i as u128), "t", HIGH, &[SAM], None))
        .collect();
    let (mut engine, step) = Engine::start(
        &catalog(),
        "SELECT DISTINCT p.email FROM macro.tasks t JOIN macro.people p ON t.assignees = p.id",
    )
    .unwrap();
    let Step::Fetch(request) = step else {
        panic!("expected a fetch");
    };
    let step = engine
        .feed_page(
            request.id,
            Page {
                rows: many,
                next: Some("more".into()),
            },
        )
        .unwrap();
    let Step::Fetch(request) = step else {
        panic!("expected the people fetch");
    };
    assert_eq!(
        request.query,
        GqlQuery::People {
            ids: Some(vec![SAM.into()]),
        }
    );
    let Step::Done(outcome) = engine
        .feed_page(
            request.id,
            Page {
                rows: people(),
                next: None,
            },
        )
        .unwrap()
    else {
        panic!("expected the answer");
    };
    assert!(outcome.truncated);
    assert_eq!(outcome.rows, vec![vec![text("sam@example.com")]]);
}

#[test]
fn a_write_does_not_start_an_engine() {
    let error = Engine::start(
        &catalog(),
        "DELETE FROM crm.deals WHERE row_id = '00000000-0000-0000-0000-0000000000a1'",
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "the engine runs SELECT statements; writes go through run()"
    );
}

#[test]
fn steps_and_pages_cross_the_wire_as_json() {
    let (_, step) =
        Engine::start(&catalog(), "SELECT name FROM crm.deals WHERE stage = 'Won'").unwrap();
    let json = serde_json::to_value(&step).unwrap();
    assert_eq!(json["step"], "fetch");
    assert_eq!(json["query"]["type"], "soup");
    assert_eq!(json["query"]["table"], DEALS.to_string());
    assert!(json["query"]["keyHint"].is_null());
    assert!(json["query"].get("key_hint").is_none());
    assert_eq!(json["needs"], serde_json::json!([NAME.to_string()]));
    assert_eq!(json["limit"], PAGE_LIMIT);
    let back: Step = serde_json::from_value(json).unwrap();
    assert_eq!(back, step);

    let page = Page {
        rows: vec![Row {
            id: ACME,
            cells: HashMap::from([(NAME, Cell::Text("Acme".into()))]),
        }],
        next: Some("2".into()),
    };
    let json = serde_json::to_value(&page).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "rows": [{ "id": ACME.to_string(), "cells": { NAME.to_string(): { "type": "text", "value": "Acme" } } }],
            "next": "2"
        })
    );
    assert_eq!(serde_json::from_value::<Page>(json).unwrap(), page);
}

// ---- run() drives the same engine ---------------------------------------------

struct ByTable;

impl RowSource for ByTable {
    async fn page(
        &self,
        query: &GqlQuery,
        _needs: &[Uuid],
        _cursor: Option<String>,
        _limit: usize,
    ) -> Result<Page, SourceError> {
        Ok(Page {
            rows: by_table(query),
            next: None,
        })
    }

    async fn bins(&self, _: &GqlQuery) -> Result<Vec<Bin>, SourceError> {
        unreachable!()
    }
}

struct NoWrites;

impl RowWriter for NoWrites {
    async fn insert(&self, _: Uuid, _: Vec<(Uuid, Value)>) -> Result<Uuid, WriteError> {
        unreachable!()
    }

    async fn update(
        &self,
        _: Uuid,
        _: Uuid,
        _: Vec<(Uuid, Option<Value>)>,
    ) -> Result<(), WriteError> {
        unreachable!()
    }

    async fn delete(&self, _: Uuid, _: Uuid) -> Result<(), WriteError> {
        unreachable!()
    }
}

#[test]
fn run_answers_a_join_through_a_row_source() {
    let outcome = pollster::block_on(run(
        &catalog(),
        "SELECT DISTINCT p.email FROM macro.tasks t JOIN macro.people p ON t.assignees = p.id
         WHERE t.priority = 'High' ORDER BY p.email DESC",
        &ByTable,
        &NoWrites,
    ))
    .unwrap();
    assert_eq!(
        outcome.rows,
        vec![vec![text("sam@example.com")], vec![text("ana@example.com")]]
    );
}
