use chrono::{TimeZone, Utc};

use super::*;
use crate::catalog::Catalog;
use crate::parse::parse;

use crate::test_support::{catalog, *};

// ---- resolved statements: full literals ------------------------------------

#[test]
fn grouped_aggregate_with_mixed_where() {
    let sql = "
        SELECT owner, SUM(amount), COUNT(*)
        FROM crm.deals
        WHERE stage IN ('won', 'Lead') AND amount > 5000 AND \"closed at\" IS NOT NULL
        GROUP BY owner
        ORDER BY 2 DESC, owner ASC
    ";

    let expected = Query::Select(SelectQuery {
        table: DEALS,
        items: vec![
            SelectItem::Column(OWNER),
            SelectItem::Agg {
                func: AggFn::Sum,
                column: Some(AMOUNT),
            },
            SelectItem::Agg {
                func: AggFn::Count,
                column: None,
            },
        ],
        where_: Some(Filter::And(vec![
            Filter::In {
                column: STAGE,
                values: vec![Value::Option(WON), Value::Option(LEAD)],
                negated: false,
            },
            Filter::Cmp {
                column: AMOUNT,
                op: CmpOp::Gt,
                value: Value::Number(5000.0),
            },
            Filter::IsNull {
                column: CLOSED_AT,
                negated: true,
            },
        ])),
        group_by: Some(OWNER),
        order_by: vec![
            Order {
                key: OrderKey::Item(1),
                dir: Dir::Desc,
            },
            Order {
                key: OrderKey::Column(OWNER),
                dir: Dir::Asc,
            },
        ],
    });

    assert_eq!(resolve(&catalog(), parse(sql).unwrap()).unwrap(), expected);
}

#[test]
fn star_expands_and_every_column_kind_types_its_literal() {
    let sql = "
        SELECT * FROM deals
        WHERE Name LIKE 'A%'
           OR amount <= 10
           OR stage != 'Won'
           OR \"closed at\" >= '2026-09-01'
           OR owner = 'macro|sam@example.com'
           OR tags NOT HAS 'VIP'
           OR done = TRUE
           OR website = 'https://acme.example'
        ORDER BY \"closed at\" DESC
    ";

    let expected = Query::Select(SelectQuery {
        table: DEALS,
        items: vec![
            SelectItem::Column(NAME),
            SelectItem::Column(AMOUNT),
            SelectItem::Column(STAGE),
            SelectItem::Column(CLOSED_AT),
            SelectItem::Column(OWNER),
            SelectItem::Column(TAGS),
            SelectItem::Column(DONE),
            SelectItem::Column(WEBSITE),
        ],
        where_: Some(Filter::Or(vec![
            Filter::Like {
                column: NAME,
                pattern: "A%".into(),
                negated: false,
            },
            Filter::Cmp {
                column: AMOUNT,
                op: CmpOp::Le,
                value: Value::Number(10.0),
            },
            Filter::Cmp {
                column: STAGE,
                op: CmpOp::Ne,
                value: Value::Option(WON),
            },
            Filter::Cmp {
                column: CLOSED_AT,
                op: CmpOp::Ge,
                value: Value::Date(Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap()),
            },
            Filter::Cmp {
                column: OWNER,
                op: CmpOp::Eq,
                value: Value::Entity("macro|sam@example.com".into()),
            },
            Filter::Has {
                column: TAGS,
                value: Value::Option(VIP),
                negated: true,
            },
            Filter::Cmp {
                column: DONE,
                op: CmpOp::Eq,
                value: Value::Bool(true),
            },
            Filter::Cmp {
                column: WEBSITE,
                op: CmpOp::Eq,
                value: Value::Text("https://acme.example".into()),
            },
        ])),
        group_by: None,
        order_by: vec![Order {
            key: OrderKey::Column(CLOSED_AT),
            dir: Dir::Desc,
        }],
    });

    assert_eq!(
        resolve(
            &Catalog {
                tables: catalog().tables.into_iter().take(2).collect(),
            },
            parse(sql).unwrap()
        )
        .unwrap(),
        expected
    );
}

#[test]
fn order_by_aggregate_resolves_to_its_select_item() {
    let sql = "SELECT stage, MAX(\"closed at\") FROM crm.deals GROUP BY stage ORDER BY MAX(\"closed at\") DESC, stage";

    let expected = Query::Select(SelectQuery {
        table: DEALS,
        items: vec![
            SelectItem::Column(STAGE),
            SelectItem::Agg {
                func: AggFn::Max,
                column: Some(CLOSED_AT),
            },
        ],
        where_: None,
        group_by: Some(STAGE),
        order_by: vec![
            Order {
                key: OrderKey::Item(1),
                dir: Dir::Desc,
            },
            Order {
                key: OrderKey::Column(STAGE),
                dir: Dir::Asc,
            },
        ],
    });

    assert_eq!(resolve(&catalog(), parse(sql).unwrap()).unwrap(), expected);
}

#[test]
fn insert_types_each_cell_and_drops_nulls() {
    let sql = "
        INSERT INTO crm.deals (name, stage, amount, \"closed at\", owner)
        VALUES ('Acme', 'Won', 12000, '2026-09-01T09:30:00Z', 'macro|sam@example.com'),
               ('Globex', 'lead', NULL, NULL, NULL)
    ";

    let expected = Query::Insert(InsertQuery {
        table: DEALS,
        rows: vec![
            vec![
                (NAME, Value::Text("Acme".into())),
                (STAGE, Value::Option(WON)),
                (AMOUNT, Value::Number(12000.0)),
                (
                    CLOSED_AT,
                    Value::Date(Utc.with_ymd_and_hms(2026, 9, 1, 9, 30, 0).unwrap()),
                ),
                (OWNER, Value::Entity("macro|sam@example.com".into())),
            ],
            vec![
                (NAME, Value::Text("Globex".into())),
                (STAGE, Value::Option(LEAD)),
            ],
        ],
    });

    assert_eq!(resolve(&catalog(), parse(sql).unwrap()).unwrap(), expected);
}

// ---- rejections: the exact message the agent reads --------------------------

#[test]
fn rejections_quote_what_the_agent_wrote() {
    let cases: &[(&str, &str)] = &[
        (
            "SELECT nam FROM crm.deals",
            "unknown column \"nam\" in crm.deals — did you mean \"name\"?",
        ),
        (
            "SELECT closed_at FROM crm.deals",
            "unknown column \"closed_at\" in crm.deals — did you mean \"closed at\"?",
        ),
        (
            "SELECT zzz FROM crm.deals",
            "unknown column \"zzz\" in crm.deals",
        ),
        (
            "SELECT * FROM crm.dealz",
            "unknown table crm.dealz — did you mean crm.deals?",
        ),
        (
            "SELECT * FROM hr.deals",
            "unknown table hr.deals — did you mean crm.deals?",
        ),
        (
            "SELECT * FROM deals",
            "table \"deals\" exists in crm and sales — qualify it as crm.deals or sales.deals",
        ),
        (
            "SELECT * FROM crm.deals WHERE stage = 'Wonn'",
            "\"Wonn\" is not an option of \"stage\" (Lead, Won)",
        ),
        (
            "SELECT * FROM crm.deals WHERE stage > 'Won'",
            "cannot use > on \"stage\": select columns support =, != and IN",
        ),
        (
            "SELECT * FROM crm.deals WHERE done < TRUE",
            "cannot use < on \"done\": checkbox columns support = and !=",
        ),
        (
            "SELECT * FROM crm.deals WHERE owner HAS 'macro|a@b.com'",
            "\"owner\" holds one value; use = instead of HAS",
        ),
        (
            "SELECT * FROM crm.deals WHERE tags = 'vip'",
            "\"tags\" holds several values; use HAS instead of =",
        ),
        (
            "SELECT * FROM crm.deals WHERE tags IN ('vip')",
            "\"tags\" holds several values; use HAS instead of =",
        ),
        (
            "SELECT * FROM crm.deals WHERE amount = 'lots'",
            "\"amount\" is a number column; compare it to a number",
        ),
        (
            "SELECT * FROM crm.deals WHERE \"closed at\" > 'yesterday'",
            "\"closed at\" is a date column; compare it to an ISO date like '2026-09-01' or '2026-09-01T09:00:00Z'",
        ),
        (
            "SELECT * FROM crm.deals WHERE done = 1",
            "\"done\" is a checkbox column; compare it to TRUE or FALSE",
        ),
        (
            "SELECT * FROM crm.deals WHERE owner = 42",
            "\"owner\" is an entity column; give an id like 'macro|sam@example.com', not a name",
        ),
        (
            "SELECT * FROM crm.deals WHERE amount LIKE '1%'",
            "cannot use LIKE on \"amount\": LIKE only applies to text columns",
        ),
        (
            "SELECT * FROM crm.deals WHERE stage = NULL",
            "use \"stage\" IS NULL or IS NOT NULL to test for an empty cell",
        ),
        (
            "SELECT SUM(name) FROM crm.deals",
            "SUM cannot apply to \"name\": it is a text column",
        ),
        (
            "SELECT MIN(stage) FROM crm.deals",
            "MIN cannot apply to \"stage\": it is a select column",
        ),
        (
            "SELECT name, COUNT(*) FROM crm.deals",
            "\"name\" must appear in GROUP BY when the select list has aggregates",
        ),
        (
            "SELECT name FROM crm.deals GROUP BY stage",
            "\"name\" must appear in GROUP BY or inside an aggregate",
        ),
        (
            "SELECT name FROM crm.deals ORDER BY 3",
            "ORDER BY 3 is out of range; the select list has 1 item",
        ),
        (
            "SELECT name FROM crm.deals ORDER BY MAX(amount)",
            "ORDER BY MAX(amount) must also appear in the select list",
        ),
        (
            "SELECT stage, COUNT(*) FROM crm.deals GROUP BY stage ORDER BY amount",
            "cannot ORDER BY \"amount\": it is neither the GROUP BY column nor aggregated",
        ),
        (
            "INSERT INTO crm.deals (name, name) VALUES ('a', 'b')",
            "\"name\" is listed twice in the column list",
        ),
        (
            "INSERT INTO crm.deals (owner) VALUES ('Sam')",
            "\"owner\" is an entity column; give an id like 'macro|sam@example.com', not a name",
        ),
    ];

    for (sql, message) in cases {
        let error = resolve(&catalog(), parse(sql).unwrap()).unwrap_err();
        assert_eq!(error.to_string(), *message, "\n{sql}");
    }
}
