use chrono::{TimeZone, Utc};

use uuid::Uuid;

use super::*;
use crate::catalog::{Catalog, PEOPLE_EMAIL, PEOPLE_ID, PEOPLE_NAME, PEOPLE_TABLE};
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
        distinct: false,
        relations: vec![Relation {
            table: DEALS,
            alias: "deals".into(),
        }],
        joins: vec![],
        bindings: vec![],
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
        limit: None,
        offset: None,
    });

    assert_eq!(
        without_bindings(resolve(&catalog(), parse(sql).unwrap()).unwrap()),
        expected
    );
}

/// The bindings are the scope's bookkeeping; the literal tests check the
/// rest.
fn without_bindings(query: Query) -> Query {
    match query {
        Query::Select(select) => Query::Select(SelectQuery {
            bindings: vec![],
            ..select
        }),
        other => other,
    }
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
        distinct: false,
        relations: vec![Relation {
            table: DEALS,
            alias: "deals".into(),
        }],
        joins: vec![],
        bindings: vec![],
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
        limit: None,
        offset: None,
    });

    assert_eq!(
        without_bindings(
            resolve(
                &Catalog {
                    tables: catalog().tables.into_iter().take(2).collect(),
                },
                parse(sql).unwrap()
            )
            .unwrap()
        ),
        expected
    );
}

#[test]
fn order_by_aggregate_resolves_to_its_select_item() {
    let sql = "SELECT stage, MAX(\"closed at\") FROM crm.deals GROUP BY stage ORDER BY MAX(\"closed at\") DESC, stage";

    let expected = Query::Select(SelectQuery {
        distinct: false,
        relations: vec![Relation {
            table: DEALS,
            alias: "deals".into(),
        }],
        joins: vec![],
        bindings: vec![],
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
        limit: None,
        offset: None,
    });

    assert_eq!(
        without_bindings(resolve(&catalog(), parse(sql).unwrap()).unwrap()),
        expected
    );
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

#[test]
fn update_types_cells_and_null_clears() {
    let sql = "UPDATE crm.deals SET stage = 'won', amount = NULL WHERE row_id = '00000000-0000-0000-0000-0000000000a1'";

    let expected = Query::Update(UpdateQuery {
        table: DEALS,
        row_id: Uuid::from_u128(0xa1),
        cells: vec![(STAGE, Some(Value::Option(WON))), (AMOUNT, None)],
    });
    assert_eq!(resolve(&catalog(), parse(sql).unwrap()).unwrap(), expected);

    let sql = "DELETE FROM crm.deals WHERE row_id = '00000000-0000-0000-0000-0000000000a1'";
    let expected = Query::Delete(DeleteQuery {
        table: DEALS,
        row_id: Uuid::from_u128(0xa1),
    });
    assert_eq!(resolve(&catalog(), parse(sql).unwrap()).unwrap(), expected);
}

#[test]
fn joins_key_each_relation_and_record_bindings() {
    let sql = "
        SELECT DISTINCT p.email, t.row_id
        FROM macro.tasks t
        JOIN macro.people p ON t.assignees = p.id
        LEFT JOIN crm.deals ON t.deal = deals.row_id
        WHERE priority = 'High' AND deals.amount > 100
        ORDER BY email
    ";
    let people_id = column_key(1, PEOPLE_ID);
    let people_email = column_key(1, PEOPLE_EMAIL);
    let deals_amount = column_key(2, AMOUNT);

    let expected = Query::Select(SelectQuery {
        distinct: true,
        relations: vec![
            Relation {
                table: TASKS,
                alias: "t".into(),
            },
            Relation {
                table: PEOPLE_TABLE,
                alias: "p".into(),
            },
            Relation {
                table: DEALS,
                alias: "deals".into(),
            },
        ],
        joins: vec![
            ResolvedJoin {
                relation: 1,
                kind: JoinKind::Inner,
                on: vec![(ASSIGNEES, people_id)],
            },
            ResolvedJoin {
                relation: 2,
                kind: JoinKind::Left,
                on: vec![(DEAL, row_id_key(DEALS))],
            },
        ],
        items: vec![
            SelectItem::Column(people_email),
            SelectItem::Column(row_id_key(TASKS)),
        ],
        where_: Some(Filter::And(vec![
            Filter::Cmp {
                column: PRIORITY,
                op: CmpOp::Eq,
                value: Value::Option(HIGH),
            },
            Filter::Cmp {
                column: deals_amount,
                op: CmpOp::Gt,
                value: Value::Number(100.0),
            },
        ])),
        group_by: None,
        order_by: vec![Order {
            key: OrderKey::Column(people_email),
            dir: Dir::Asc,
        }],
        limit: None,
        offset: None,
        bindings: vec![
            Binding {
                key: ASSIGNEES,
                relation: 0,
                column: Some(ASSIGNEES),
            },
            Binding {
                key: people_id,
                relation: 1,
                column: Some(PEOPLE_ID),
            },
            Binding {
                key: DEAL,
                relation: 0,
                column: Some(DEAL),
            },
            Binding {
                key: row_id_key(DEALS),
                relation: 2,
                column: None,
            },
            Binding {
                key: people_email,
                relation: 1,
                column: Some(PEOPLE_EMAIL),
            },
            Binding {
                key: row_id_key(TASKS),
                relation: 0,
                column: None,
            },
            Binding {
                key: PRIORITY,
                relation: 0,
                column: Some(PRIORITY),
            },
            Binding {
                key: deals_amount,
                relation: 2,
                column: Some(AMOUNT),
            },
        ],
    });

    assert_eq!(resolve(&catalog(), parse(sql).unwrap()).unwrap(), expected);
}

#[test]
fn a_definition_bound_to_both_joined_tables_gets_two_keys() {
    // `crm.deals.name` and `crm.people.name` are the same definition.
    let query = match resolve(
        &catalog(),
        parse("SELECT d.name, p.name FROM crm.deals d JOIN crm.people p ON d.owner = p.row_id")
            .unwrap(),
    )
    .unwrap()
    {
        Query::Select(query) => query,
        _ => unreachable!(),
    };
    assert_eq!(
        query.items,
        vec![
            SelectItem::Column(NAME),
            SelectItem::Column(column_key(1, NAME)),
        ]
    );
    assert_ne!(NAME, column_key(1, NAME));
    assert_eq!(
        query.binding(column_key(1, NAME)),
        Some(&Binding {
            key: column_key(1, NAME),
            relation: 1,
            column: Some(NAME),
        })
    );
}

#[test]
fn star_over_a_join_lists_every_relation_in_order() {
    let query = match resolve(
        &catalog(),
        parse("SELECT * FROM macro.tasks t JOIN macro.people p ON t.assignees = p.id").unwrap(),
    )
    .unwrap()
    {
        Query::Select(query) => query,
        _ => unreachable!(),
    };
    assert_eq!(
        query.items,
        vec![
            SelectItem::Column(TITLE),
            SelectItem::Column(PRIORITY),
            SelectItem::Column(ASSIGNEES),
            SelectItem::Column(DEAL),
            SelectItem::Column(column_key(1, PEOPLE_ID)),
            SelectItem::Column(column_key(1, PEOPLE_NAME)),
            SelectItem::Column(column_key(1, PEOPLE_EMAIL)),
        ]
    );
}

#[test]
fn join_rejections_quote_what_the_agent_wrote() {
    let cases: &[(&str, &str)] = &[
        (
            "SELECT name FROM crm.deals d JOIN crm.people p ON d.owner = p.row_id",
            "\"name\" is ambiguous — qualify it as d.name or p.name",
        ),
        (
            "SELECT x.name FROM crm.deals d JOIN crm.people p ON d.owner = p.row_id",
            "unknown table \"x\" in x.name — the query reads crm.deals as d and crm.people as p",
        ),
        (
            "SELECT d.name FROM crm.deals d JOIN crm.people p ON d.amount = p.row_id",
            "cannot join d.amount (number) to p.row_id (entity): join columns must hold the same kind of value",
        ),
        (
            "SELECT d.name FROM crm.deals d JOIN crm.people p ON d.owner = d.owner",
            "ON d.owner = d.owner must compare a column of p with a column of an earlier table",
        ),
        (
            "SELECT d.name FROM crm.deals d JOIN crm.people d ON d.owner = d.row_id",
            "\"d\" already names crm.deals; give the other table an alias, like JOIN crm.people p",
        ),
        (
            "SELECT d.emial FROM crm.deals d JOIN macro.people p ON d.owner = p.id",
            "unknown column \"emial\" in crm.deals",
        ),
        (
            "SELECT emai FROM crm.deals d JOIN macro.people p ON d.owner = p.id",
            "unknown column \"emai\" in crm.deals or macro.people — did you mean \"email\"?",
        ),
        (
            "SELECT d.name FROM crm.deals d JOIN macro.people p ON d.owner = p.id GROUP BY p.email",
            "\"name\" must appear in GROUP BY or inside an aggregate",
        ),
    ];

    for (sql, message) in cases {
        let error = resolve(&catalog(), parse(sql).unwrap()).unwrap_err();
        assert_eq!(error.to_string(), *message, "\n{sql}");
    }
}

#[test]
fn lists_type_multi_valued_cells_and_bare_values_become_one_element_lists() {
    let sql = "UPDATE crm.deals SET tags = ['vip'], owner = 'macro|sam@example.com', stage = ['Won'] WHERE row_id = '00000000-0000-0000-0000-0000000000a1'";
    let expected = Query::Update(UpdateQuery {
        table: DEALS,
        row_id: Uuid::from_u128(0xa1),
        cells: vec![
            (TAGS, Some(Value::Options(vec![VIP]))),
            (OWNER, Some(Value::Entity("macro|sam@example.com".into()))),
            (STAGE, Some(Value::Option(WON))),
        ],
    });
    assert_eq!(resolve(&catalog(), parse(sql).unwrap()).unwrap(), expected);

    let sql = "INSERT INTO crm.deals (name, tags) VALUES ('Acme', 'vip'), ('Globex', ['vip'])";
    let expected = Query::Insert(InsertQuery {
        table: DEALS,
        rows: vec![
            vec![
                (NAME, Value::Text("Acme".into())),
                (TAGS, Value::Options(vec![VIP])),
            ],
            vec![
                (NAME, Value::Text("Globex".into())),
                (TAGS, Value::Options(vec![VIP])),
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
            "UPDATE crm.deals SET stage = 'Won' WHERE row_id = 'first'",
            "'first' is not a row id; row ids are the UUIDs a SELECT returns",
        ),
        (
            "UPDATE crm.deals SET stage = 'Won', stage = 'Lead' WHERE row_id = '00000000-0000-0000-0000-0000000000a1'",
            "\"stage\" is listed twice in the column list",
        ),
        (
            "UPDATE crm.deals SET amount = 'lots' WHERE row_id = '00000000-0000-0000-0000-0000000000a1'",
            "\"amount\" is a number column; compare it to a number",
        ),
        (
            "UPDATE crm.deals SET stage = ['Won', 'Lead'] WHERE row_id = '00000000-0000-0000-0000-0000000000a1'",
            "\"stage\" holds one value; a list of 2 was given",
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
