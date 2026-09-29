use std::collections::HashMap;

use chrono::{TimeZone, Utc};

use super::*;
use crate::resolve::{Query, compile};
use crate::split::split;
use crate::test_support::{catalog, *};

const ACME: Uuid = Uuid::from_u128(0xa1);
const GLOBEX: Uuid = Uuid::from_u128(0xa2);
const HOOLI: Uuid = Uuid::from_u128(0xa3);
const INITECH: Uuid = Uuid::from_u128(0xa4);

/// Four deals as the server would return them for a `SELECT *`:
/// Acme (Won, 12000, Sam, vip, done), Globex (Lead, 3000, Sam), Hooli (Won,
/// no amount, Ana, closed), Initech (no stage, 7000, no owner, not done).
fn deals() -> Vec<Row> {
    vec![
        Row {
            id: ACME,
            cells: HashMap::from([
                (NAME, Cell::Text("Acme".into())),
                (AMOUNT, Cell::Number(12000.0)),
                (STAGE, Cell::Options(vec![WON])),
                (OWNER, Cell::Entities(vec!["macro|sam@example.com".into()])),
                (TAGS, Cell::Options(vec![VIP])),
                (DONE, Cell::Bool(true)),
            ]),
        },
        Row {
            id: GLOBEX,
            cells: HashMap::from([
                (NAME, Cell::Text("Globex".into())),
                (AMOUNT, Cell::Number(3000.0)),
                (STAGE, Cell::Options(vec![LEAD])),
                (OWNER, Cell::Entities(vec!["macro|sam@example.com".into()])),
                (TAGS, Cell::Options(vec![])),
            ]),
        },
        Row {
            id: HOOLI,
            cells: HashMap::from([
                (NAME, Cell::Text("hooli".into())),
                (STAGE, Cell::Options(vec![WON])),
                (
                    CLOSED_AT,
                    Cell::Date(Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap()),
                ),
                (OWNER, Cell::Entities(vec!["macro|ana@example.com".into()])),
            ]),
        },
        Row {
            id: INITECH,
            cells: HashMap::from([
                (NAME, Cell::Text("Initech".into())),
                (AMOUNT, Cell::Number(7000.0)),
                (DONE, Cell::Bool(false)),
            ]),
        },
    ]
}

fn plan(sql: &str) -> Plan {
    match compile(&catalog(), sql).unwrap() {
        Query::Select(select) => split(&catalog(), select),
        Query::Insert(_) | Query::Update(_) | Query::Delete(_) => panic!("not a SELECT"),
    }
}

// ---- results: full literals -------------------------------------------------

#[test]
fn rows_are_filtered_projected_and_sorted() {
    let plan = plan(
        "SELECT name, amount, \"closed at\" FROM crm.deals
         WHERE amount > 5000 OR \"closed at\" IS NOT NULL
         ORDER BY amount DESC",
    );

    assert_eq!(
        fold_rows(&catalog(), &plan, deals()),
        vec![
            vec![
                Some(Cell::Text("Acme".into())),
                Some(Cell::Number(12000.0)),
                None,
            ],
            vec![
                Some(Cell::Text("Initech".into())),
                Some(Cell::Number(7000.0)),
                None,
            ],
            // No amount: sorts last even though the order is descending.
            vec![
                Some(Cell::Text("hooli".into())),
                None,
                Some(Cell::Date(
                    Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap()
                )),
            ],
        ]
    );
}

#[test]
fn aggregates_per_group_with_a_residual_filter() {
    let plan = plan(
        "SELECT owner, SUM(amount), COUNT(*), COUNT(amount), AVG(amount)
         FROM crm.deals
         WHERE stage IN ('Won', 'Lead')
         GROUP BY owner
         ORDER BY 2 DESC",
    );
    // The server would have applied `stage IN (...)`; hand the fold only
    // what it would have returned.
    let won_or_lead: Vec<Row> = deals().into_iter().take(3).collect();

    assert_eq!(
        fold_rows(&catalog(), &plan, won_or_lead),
        vec![
            vec![
                Some(Cell::Entities(vec!["macro|sam@example.com".into()])),
                Some(Cell::Number(15000.0)),
                Some(Cell::Number(2.0)),
                Some(Cell::Number(2.0)),
                Some(Cell::Number(7500.0)),
            ],
            // Hooli has no amount: SUM and AVG of nothing are NULL, COUNT(*)
            // still counts the row, COUNT(amount) does not.
            vec![
                Some(Cell::Entities(vec!["macro|ana@example.com".into()])),
                None,
                Some(Cell::Number(1.0)),
                Some(Cell::Number(0.0)),
                None,
            ],
        ]
    );
}

#[test]
fn aggregates_without_group_by_are_one_row_even_over_nothing() {
    let plan = plan("SELECT COUNT(*), SUM(amount), MIN(amount), MAX(\"closed at\") FROM crm.deals");

    assert_eq!(
        fold_rows(&catalog(), &plan, deals()),
        vec![vec![
            Some(Cell::Number(4.0)),
            Some(Cell::Number(22000.0)),
            Some(Cell::Number(3000.0)),
            Some(Cell::Date(
                Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap()
            )),
        ]]
    );
    assert_eq!(
        fold_rows(&catalog(), &plan, vec![]),
        vec![vec![Some(Cell::Number(0.0)), None, None, None]]
    );
}

#[test]
fn groups_sort_by_option_order_and_the_empty_group_last() {
    // Option order in the catalog is Lead, Won; a plain sort would put Won
    // first by id.
    let plan = plan(
        "SELECT stage, COUNT(*) FROM crm.deals WHERE amount > 0 GROUP BY stage ORDER BY stage",
    );

    assert_eq!(
        fold_rows(&catalog(), &plan, deals()),
        vec![
            vec![Some(Cell::Options(vec![LEAD])), Some(Cell::Number(1.0))],
            vec![Some(Cell::Options(vec![WON])), Some(Cell::Number(1.0))],
            vec![None, Some(Cell::Number(1.0))],
        ]
    );
}

#[test]
fn bins_answer_a_count_only_group() {
    let plan = plan("SELECT stage, COUNT(*) FROM crm.deals GROUP BY stage ORDER BY 2 DESC, stage");
    let bins = vec![
        Bin {
            key: Some(Cell::Options(vec![LEAD])),
            count: 4,
        },
        Bin {
            key: None,
            count: 4,
        },
        Bin {
            key: Some(Cell::Options(vec![WON])),
            count: 9,
        },
    ];

    assert_eq!(
        fold_bins(&catalog(), &plan, bins),
        vec![
            vec![Some(Cell::Options(vec![WON])), Some(Cell::Number(9.0))],
            vec![Some(Cell::Options(vec![LEAD])), Some(Cell::Number(4.0))],
            vec![None, Some(Cell::Number(4.0))],
        ]
    );
}

// ---- residual predicate semantics: which rows each WHERE keeps ---------------

#[test]
fn residual_predicates_follow_sql_null_rules_and_macro_matching() {
    let cases: &[(&str, &[Uuid])] = &[
        // number comparisons; an empty cell never compares true
        ("amount > 5000", &[ACME, INITECH]),
        ("amount <= 7000", &[GLOBEX, INITECH]),
        ("amount != 7000", &[ACME, GLOBEX]),
        // != and NOT IN drop empty cells, as in SQL
        ("stage != 'Won'", &[GLOBEX]),
        ("stage NOT IN ('Won', 'Lead')", &[]),
        ("owner NOT IN ('macro|sam@example.com')", &[HOOLI]),
        // IS NULL is how you ask for empty cells
        ("stage IS NULL", &[INITECH]),
        ("stage IS NOT NULL", &[ACME, GLOBEX, HOOLI]),
        // multi-valued: an absent cell is the empty set
        ("tags HAS 'vip'", &[ACME]),
        ("tags NOT HAS 'vip'", &[GLOBEX, HOOLI, INITECH]),
        // text: LIKE ignores case, = does not
        ("name LIKE 'h%'", &[HOOLI]),
        ("name LIKE '%o%'", &[GLOBEX, HOOLI]),
        ("name LIKE '_cme'", &[ACME]),
        ("name NOT LIKE '%e%'", &[HOOLI]),
        ("name = 'Hooli'", &[]),
        ("name < 'H'", &[ACME, GLOBEX]),
        // checkbox: an unset checkbox is not FALSE
        ("done = FALSE", &[INITECH]),
        ("done != TRUE", &[INITECH]),
        // dates
        ("\"closed at\" >= '2026-09-15'", &[HOOLI]),
        ("\"closed at\" < '2026-09-15T00:00:01Z'", &[HOOLI]),
        // combinations
        ("amount > 5000 AND done = TRUE", &[ACME]),
        ("amount > 5000 OR stage = 'Lead'", &[ACME, GLOBEX, INITECH]),
        (
            "(amount > 5000 OR stage = 'Lead') AND owner = 'macro|sam@example.com'",
            &[ACME, GLOBEX],
        ),
    ];

    // Split would push some of these to the server; test the evaluator on the
    // whole resolved WHERE so every form is covered as the fold would apply
    // it when it is residual.
    for (where_, kept) in cases {
        let Query::Select(select) = compile(
            &catalog(),
            &format!("SELECT name FROM crm.deals WHERE {where_}"),
        )
        .unwrap() else {
            panic!("not a SELECT");
        };
        let filter = select.where_.unwrap();
        let held: Vec<Uuid> = deals()
            .iter()
            .filter(|row| super::predicate::holds(&filter, row))
            .map(|row| row.id)
            .collect();
        assert_eq!(held, *kept, "\nWHERE {where_}");
    }
}
