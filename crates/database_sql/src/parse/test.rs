use super::*;

// ---- accepted statements: one full literal per grammar area ---------------

#[test]
fn grouped_aggregate_with_mixed_where() {
    let sql = "
        SELECT owner, SUM(amount), COUNT(*)
        FROM crm.deals
        WHERE stage IN ('Won', 'Lead') AND amount > 5000 AND closed_at IS NOT NULL
        GROUP BY owner
        ORDER BY 2 DESC, owner ASC
    ";

    let expected = Statement::Select(Select {
        items: vec![
            Item::Column(Ident("owner".into())),
            Item::Agg(Agg {
                func: AggFn::Sum,
                arg: Some(Ident("amount".into())),
            }),
            Item::Agg(Agg {
                func: AggFn::Count,
                arg: None,
            }),
        ],
        table: TableName {
            database: Some(Ident("crm".into())),
            table: Ident("deals".into()),
        },
        where_: Some(Cond::And(vec![
            Cond::In {
                column: Ident("stage".into()),
                values: vec![Lit::Str("Won".into()), Lit::Str("Lead".into())],
                negated: false,
            },
            Cond::Cmp {
                column: Ident("amount".into()),
                op: CmpOp::Gt,
                value: Lit::Num(5000.0),
            },
            Cond::IsNull {
                column: Ident("closed_at".into()),
                negated: true,
            },
        ])),
        group_by: Some(Ident("owner".into())),
        order_by: vec![
            OrderBy {
                key: OrderKey::Position(2),
                dir: Dir::Desc,
            },
            OrderBy {
                key: OrderKey::Column(Ident("owner".into())),
                dir: Dir::Asc,
            },
        ],
    });

    assert_eq!(parse(sql).unwrap(), expected);
}

#[test]
fn or_binds_looser_than_and_and_parens_override() {
    let sql = "SELECT * FROM deals WHERE a = 1 OR b = 2 AND (c = 3 OR d = 4)";

    let expected = Statement::Select(Select {
        items: vec![Item::Star],
        table: TableName {
            database: None,
            table: Ident("deals".into()),
        },
        where_: Some(Cond::Or(vec![
            Cond::Cmp {
                column: Ident("a".into()),
                op: CmpOp::Eq,
                value: Lit::Num(1.0),
            },
            Cond::And(vec![
                Cond::Cmp {
                    column: Ident("b".into()),
                    op: CmpOp::Eq,
                    value: Lit::Num(2.0),
                },
                Cond::Or(vec![
                    Cond::Cmp {
                        column: Ident("c".into()),
                        op: CmpOp::Eq,
                        value: Lit::Num(3.0),
                    },
                    Cond::Cmp {
                        column: Ident("d".into()),
                        op: CmpOp::Eq,
                        value: Lit::Num(4.0),
                    },
                ]),
            ]),
        ])),
        group_by: None,
        order_by: vec![],
    });

    assert_eq!(parse(sql).unwrap(), expected);
}

#[test]
fn every_atom_form_and_literal_kind() {
    let sql = "
        select name from crm.deals
        where stage not in ('Lost')
          and tags has 'vip'
          and assignees not has 'macro|a@b.com'
          and closed_at is null
          and name like 'A%'
          and notes not like '%draft%'
          and done = true
          and score >= -1.5e3
          and \"Plus ones\" <> .5
          and note != 'it''s';
    ";

    let expected = Statement::Select(Select {
        items: vec![Item::Column(Ident("name".into()))],
        table: TableName {
            database: Some(Ident("crm".into())),
            table: Ident("deals".into()),
        },
        where_: Some(Cond::And(vec![
            Cond::In {
                column: Ident("stage".into()),
                values: vec![Lit::Str("Lost".into())],
                negated: true,
            },
            Cond::Has {
                column: Ident("tags".into()),
                value: Lit::Str("vip".into()),
                negated: false,
            },
            Cond::Has {
                column: Ident("assignees".into()),
                value: Lit::Str("macro|a@b.com".into()),
                negated: true,
            },
            Cond::IsNull {
                column: Ident("closed_at".into()),
                negated: false,
            },
            Cond::Like {
                column: Ident("name".into()),
                pattern: "A%".into(),
                negated: false,
            },
            Cond::Like {
                column: Ident("notes".into()),
                pattern: "%draft%".into(),
                negated: true,
            },
            Cond::Cmp {
                column: Ident("done".into()),
                op: CmpOp::Eq,
                value: Lit::Bool(true),
            },
            Cond::Cmp {
                column: Ident("score".into()),
                op: CmpOp::Ge,
                value: Lit::Num(-1500.0),
            },
            Cond::Cmp {
                column: Ident("Plus ones".into()),
                op: CmpOp::Ne,
                value: Lit::Num(0.5),
            },
            Cond::Cmp {
                column: Ident("note".into()),
                op: CmpOp::Ne,
                value: Lit::Str("it's".into()),
            },
        ])),
        group_by: None,
        order_by: vec![],
    });

    assert_eq!(parse(sql).unwrap(), expected);
}

#[test]
fn order_by_column_aggregate_and_position() {
    let sql = "SELECT stage, MAX(amount) FROM \"My CRM\".\"Big Deals\" GROUP BY stage ORDER BY stage, MAX(amount) DESC, 1 ASC";

    let expected = Statement::Select(Select {
        items: vec![
            Item::Column(Ident("stage".into())),
            Item::Agg(Agg {
                func: AggFn::Max,
                arg: Some(Ident("amount".into())),
            }),
        ],
        table: TableName {
            database: Some(Ident("My CRM".into())),
            table: Ident("Big Deals".into()),
        },
        where_: None,
        group_by: Some(Ident("stage".into())),
        order_by: vec![
            OrderBy {
                key: OrderKey::Column(Ident("stage".into())),
                dir: Dir::Asc,
            },
            OrderBy {
                key: OrderKey::Agg(Agg {
                    func: AggFn::Max,
                    arg: Some(Ident("amount".into())),
                }),
                dir: Dir::Desc,
            },
            OrderBy {
                key: OrderKey::Position(1),
                dir: Dir::Asc,
            },
        ],
    });

    assert_eq!(parse(sql).unwrap(), expected);
}

#[test]
fn keywords_are_usable_as_column_names_when_quoted() {
    // `count` unquoted is the aggregate keyword; quoted it is a column.
    let sql = "SELECT \"count\", COUNT(\"order\") FROM stats WHERE \"from\" = 'x'";

    let expected = Statement::Select(Select {
        items: vec![
            Item::Column(Ident("count".into())),
            Item::Agg(Agg {
                func: AggFn::Count,
                arg: Some(Ident("order".into())),
            }),
        ],
        table: TableName {
            database: None,
            table: Ident("stats".into()),
        },
        where_: Some(Cond::Cmp {
            column: Ident("from".into()),
            op: CmpOp::Eq,
            value: Lit::Str("x".into()),
        }),
        group_by: None,
        order_by: vec![],
    });

    assert_eq!(parse(sql).unwrap(), expected);
}

#[test]
fn insert_several_rows() {
    let sql = "
        INSERT INTO crm.deals (name, stage, amount, \"closed at\")
        VALUES ('Acme', 'Won', 12000, '2026-09-01'),
               ('Globex', 'Lead', NULL, NULL),
               ('Initech ''24', 'Lead', -0, FALSE)
    ";

    let expected = Statement::Insert(Insert {
        table: TableName {
            database: Some(Ident("crm".into())),
            table: Ident("deals".into()),
        },
        columns: vec![
            Ident("name".into()),
            Ident("stage".into()),
            Ident("amount".into()),
            Ident("closed at".into()),
        ],
        rows: vec![
            vec![
                Lit::Str("Acme".into()),
                Lit::Str("Won".into()),
                Lit::Num(12000.0),
                Lit::Str("2026-09-01".into()),
            ],
            vec![
                Lit::Str("Globex".into()),
                Lit::Str("Lead".into()),
                Lit::Null,
                Lit::Null,
            ],
            vec![
                Lit::Str("Initech '24".into()),
                Lit::Str("Lead".into()),
                Lit::Num(-0.0),
                Lit::Bool(false),
            ],
        ],
    });

    assert_eq!(parse(sql).unwrap(), expected);
}

// ---- rejected statements: the span and the exact message the agent reads ---

#[test]
fn rejections_point_at_the_offending_token() {
    let cases: &[(&str, std::ops::Range<usize>, &str)] = &[
        (
            "SELECT name FROM crm.deals WHERE amount * 1.2 > 5000",
            40..41,
            "expected a comparison operator, IN, HAS, IS or LIKE after \"amount\", found *",
        ),
        (
            "SELECT d.name FROM crm.deals d JOIN crm.people p ON p.id = d.owner",
            8..9,
            "expected FROM, found .",
        ),
        (
            "SELECT name AS n FROM crm.deals",
            12..14,
            "expected FROM, found \"AS\"",
        ),
        (
            "SELECT name FROM crm.deals LIMIT 10",
            27..32,
            "expected end of statement, found \"LIMIT\"",
        ),
        (
            "SELECT stage, SUM(amount) FROM crm.deals GROUP BY stage HAVING SUM(amount) > 1",
            56..62,
            "expected end of statement, found \"HAVING\"",
        ),
        (
            "SELECT name FROM crm.deals WHERE owner IN (SELECT id FROM crm.people)",
            43..49,
            "expected a value: 'text', a number, TRUE, FALSE or NULL, found SELECT",
        ),
        (
            "SELECT LOWER(name) FROM crm.deals",
            12..13,
            "expected FROM, found (",
        ),
        (
            "SELECT name FROM crm.deals WHERE stage IN ()",
            43..44,
            "expected a value: 'text', a number, TRUE, FALSE or NULL, found )",
        ),
        (
            "SELECT name FROM crm.deals WHERE NOT stage = 'Won'",
            33..36,
            "expected a column name to compare, found NOT",
        ),
        (
            "SELECT name FROM crm.deals WHERE stage NOT = 'Won'",
            43..44,
            "expected IN, HAS or LIKE after \"stage\" NOT, found =",
        ),
        (
            "SELECT name FROM crm.deals ORDER BY 0",
            36..37,
            "expected a column name or a 1-based select-list position after ORDER BY, found 0",
        ),
        (
            "SELECT name FROM crm.deals WHERE (stage = 'Won'",
            47..47,
            "expected ) to close the condition, found end of statement",
        ),
        (
            "SELECT name FROM crm.deals; SELECT name FROM crm.deals",
            28..34,
            "expected end of statement, found SELECT",
        ),
        (
            "SELECT FROM crm.deals",
            7..11,
            "expected a column name, an aggregate like COUNT(*) or SUM(column), or *, found FROM",
        ),
        (
            "SELECT name FROM crm.deals WHERE name = 'unterminated",
            40..53,
            "unterminated quote starting at '",
        ),
        (
            "SELECT name FROM crm.deals WHERE name = #1",
            40..41,
            "unexpected character \"#\"",
        ),
        (
            "INSERT INTO crm.deals VALUES ('Acme')",
            22..28,
            "expected ( and the column list after the table name, found VALUES",
        ),
        (
            "INSERT INTO crm.deals (name, stage) VALUES ('Acme', 'Won'), ('Globex')",
            60..70,
            "row 2 has 1 values but 2 columns were listed",
        ),
        (
            "UPDATE crm.deals SET stage = 'Won'",
            0..6,
            "expected SELECT or INSERT, found \"UPDATE\"",
        ),
    ];

    for (sql, span, message) in cases {
        let error = parse(sql).unwrap_err();
        assert_eq!(
            (error.span.clone(), error.message.as_str()),
            (span.clone(), *message),
            "\n{sql}\n{}^",
            " ".repeat(error.span.start)
        );
    }
}
