use super::*;

fn col(name: &str) -> ColumnRef {
    ColumnRef {
        table: None,
        column: Ident(name.into()),
    }
}

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
        distinct: false,
        aliases: vec![],
        items: vec![
            Item::Column(col("owner")),
            Item::Agg(Agg {
                func: AggFn::Sum,
                arg: Some(col("amount")),
            }),
            Item::Agg(Agg {
                func: AggFn::Count,
                arg: None,
            }),
        ],
        from: FromItem {
            table: TableName {
                database: Some(Ident("crm".into())),
                table: Ident("deals".into()),
            },
            alias: None,
        },
        joins: vec![],
        where_: Some(Cond::And(vec![
            Cond::In {
                column: col("stage"),
                values: vec![Lit::Str("Won".into()), Lit::Str("Lead".into())],
                negated: false,
            },
            Cond::Cmp {
                column: col("amount"),
                op: CmpOp::Gt,
                value: Lit::Num(5000.0),
            },
            Cond::IsNull {
                column: col("closed_at"),
                negated: true,
            },
        ])),
        group_by: Some(col("owner")),
        order_by: vec![
            OrderBy {
                key: OrderKey::Position(2),
                dir: Dir::Desc,
            },
            OrderBy {
                key: OrderKey::Column(col("owner")),
                dir: Dir::Asc,
            },
        ],
        limit: None,
        offset: None,
    });

    assert_eq!(parse(sql).unwrap(), expected);
}

#[test]
fn or_binds_looser_than_and_and_parens_override() {
    let sql = "SELECT * FROM deals WHERE a = 1 OR b = 2 AND (c = 3 OR d = 4)";

    let expected = Statement::Select(Select {
        distinct: false,
        aliases: vec![],
        items: vec![Item::Star],
        from: FromItem {
            table: TableName {
                database: None,
                table: Ident("deals".into()),
            },
            alias: None,
        },
        joins: vec![],
        where_: Some(Cond::Or(vec![
            Cond::Cmp {
                column: col("a"),
                op: CmpOp::Eq,
                value: Lit::Num(1.0),
            },
            Cond::And(vec![
                Cond::Cmp {
                    column: col("b"),
                    op: CmpOp::Eq,
                    value: Lit::Num(2.0),
                },
                Cond::Or(vec![
                    Cond::Cmp {
                        column: col("c"),
                        op: CmpOp::Eq,
                        value: Lit::Num(3.0),
                    },
                    Cond::Cmp {
                        column: col("d"),
                        op: CmpOp::Eq,
                        value: Lit::Num(4.0),
                    },
                ]),
            ]),
        ])),
        group_by: None,
        order_by: vec![],
        limit: None,
        offset: None,
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
        distinct: false,
        aliases: vec![],
        items: vec![Item::Column(col("name"))],
        from: FromItem {
            table: TableName {
                database: Some(Ident("crm".into())),
                table: Ident("deals".into()),
            },
            alias: None,
        },
        joins: vec![],
        where_: Some(Cond::And(vec![
            Cond::In {
                column: col("stage"),
                values: vec![Lit::Str("Lost".into())],
                negated: true,
            },
            Cond::Has {
                column: col("tags"),
                value: Lit::Str("vip".into()),
                negated: false,
            },
            Cond::Has {
                column: col("assignees"),
                value: Lit::Str("macro|a@b.com".into()),
                negated: true,
            },
            Cond::IsNull {
                column: col("closed_at"),
                negated: false,
            },
            Cond::Like {
                column: col("name"),
                pattern: "A%".into(),
                negated: false,
            },
            Cond::Like {
                column: col("notes"),
                pattern: "%draft%".into(),
                negated: true,
            },
            Cond::Cmp {
                column: col("done"),
                op: CmpOp::Eq,
                value: Lit::Bool(true),
            },
            Cond::Cmp {
                column: col("score"),
                op: CmpOp::Ge,
                value: Lit::Num(-1500.0),
            },
            Cond::Cmp {
                column: col("Plus ones"),
                op: CmpOp::Ne,
                value: Lit::Num(0.5),
            },
            Cond::Cmp {
                column: col("note"),
                op: CmpOp::Ne,
                value: Lit::Str("it's".into()),
            },
        ])),
        group_by: None,
        order_by: vec![],
        limit: None,
        offset: None,
    });

    assert_eq!(parse(sql).unwrap(), expected);
}

#[test]
fn order_by_column_aggregate_and_position() {
    let sql = "SELECT stage, MAX(amount) FROM \"My CRM\".\"Big Deals\" GROUP BY stage ORDER BY stage, MAX(amount) DESC, 1 ASC";

    let expected = Statement::Select(Select {
        distinct: false,
        aliases: vec![],
        items: vec![
            Item::Column(col("stage")),
            Item::Agg(Agg {
                func: AggFn::Max,
                arg: Some(col("amount")),
            }),
        ],
        from: FromItem {
            table: TableName {
                database: Some(Ident("My CRM".into())),
                table: Ident("Big Deals".into()),
            },
            alias: None,
        },
        joins: vec![],
        where_: None,
        group_by: Some(col("stage")),
        order_by: vec![
            OrderBy {
                key: OrderKey::Column(col("stage")),
                dir: Dir::Asc,
            },
            OrderBy {
                key: OrderKey::Agg(Agg {
                    func: AggFn::Max,
                    arg: Some(col("amount")),
                }),
                dir: Dir::Desc,
            },
            OrderBy {
                key: OrderKey::Position(1),
                dir: Dir::Asc,
            },
        ],
        limit: None,
        offset: None,
    });

    assert_eq!(parse(sql).unwrap(), expected);
}

#[test]
fn distinct_aliases_and_joins() {
    let sql = "
        SELECT DISTINCT p.email, t.row_id
        FROM macro.tasks AS t
        INNER JOIN macro.people p ON t.assignees = p.id
        LEFT OUTER JOIN crm.deals ON deals.owner = p.id AND deals.name = t.name
        WHERE t.priority = 'High'
        GROUP BY p.email
        ORDER BY p.email
    ";
    let qualified = |table: &str, column: &str| ColumnRef {
        table: Some(Ident(table.into())),
        column: Ident(column.into()),
    };

    let expected = Statement::Select(Select {
        distinct: true,
        aliases: vec![],
        items: vec![
            Item::Column(qualified("p", "email")),
            Item::Column(qualified("t", "row_id")),
        ],
        from: FromItem {
            table: TableName {
                database: Some(Ident("macro".into())),
                table: Ident("tasks".into()),
            },
            alias: Some(Ident("t".into())),
        },
        joins: vec![
            Join {
                kind: JoinKind::Inner,
                table: FromItem {
                    table: TableName {
                        database: Some(Ident("macro".into())),
                        table: Ident("people".into()),
                    },
                    alias: Some(Ident("p".into())),
                },
                on: vec![(qualified("t", "assignees"), qualified("p", "id"))],
            },
            Join {
                kind: JoinKind::Left,
                table: FromItem {
                    table: TableName {
                        database: Some(Ident("crm".into())),
                        table: Ident("deals".into()),
                    },
                    alias: None,
                },
                on: vec![
                    (qualified("deals", "owner"), qualified("p", "id")),
                    (qualified("deals", "name"), qualified("t", "name")),
                ],
            },
        ],
        where_: Some(Cond::Cmp {
            column: qualified("t", "priority"),
            op: CmpOp::Eq,
            value: Lit::Str("High".into()),
        }),
        group_by: Some(qualified("p", "email")),
        order_by: vec![OrderBy {
            key: OrderKey::Column(qualified("p", "email")),
            dir: Dir::Asc,
        }],
        limit: None,
        offset: None,
    });

    assert_eq!(parse(sql).unwrap(), expected);
}

#[test]
fn item_aliases_and_membership_joins() {
    let sql = "
        SELECT p.email AS person, COUNT(*) deals
        FROM crm.deals d
        JOIN crm.people p ON d.owner HAS p.id
        GROUP BY p.email
        ORDER BY deals DESC
    ";
    let qualified = |table: &str, column: &str| ColumnRef {
        table: Some(Ident(table.into())),
        column: Ident(column.into()),
    };

    let expected = Statement::Select(Select {
        distinct: false,
        items: vec![
            Item::Column(qualified("p", "email")),
            Item::Agg(Agg {
                func: AggFn::Count,
                arg: None,
            }),
        ],
        aliases: vec![(0, Ident("person".into())), (1, Ident("deals".into()))],
        from: FromItem {
            table: TableName {
                database: Some(Ident("crm".into())),
                table: Ident("deals".into()),
            },
            alias: Some(Ident("d".into())),
        },
        joins: vec![Join {
            kind: JoinKind::Inner,
            table: FromItem {
                table: TableName {
                    database: Some(Ident("crm".into())),
                    table: Ident("people".into()),
                },
                alias: Some(Ident("p".into())),
            },
            on: vec![(qualified("d", "owner"), qualified("p", "id"))],
        }],
        where_: None,
        group_by: Some(qualified("p", "email")),
        order_by: vec![OrderBy {
            key: OrderKey::Column(col("deals")),
            dir: Dir::Desc,
        }],
        limit: None,
        offset: None,
    });

    assert_eq!(parse(sql).unwrap(), expected);
}

#[test]
fn a_keyword_after_as_is_the_alias() {
    let sql = "SELECT stage AS count, SUM(amount) AS sum FROM crm.deals GROUP BY stage";

    let Statement::Select(select) = parse(sql).unwrap() else {
        panic!("a select");
    };
    assert_eq!(
        select.aliases,
        vec![(0, Ident("count".into())), (1, Ident("sum".into()))]
    );
}

#[test]
fn keywords_are_usable_as_column_names_when_quoted() {
    // `count` unquoted is the aggregate keyword; quoted it is a column.
    let sql = "SELECT \"count\", COUNT(\"order\") FROM stats WHERE \"from\" = 'x'";

    let expected = Statement::Select(Select {
        distinct: false,
        aliases: vec![],
        items: vec![
            Item::Column(col("count")),
            Item::Agg(Agg {
                func: AggFn::Count,
                arg: Some(col("order")),
            }),
        ],
        from: FromItem {
            table: TableName {
                database: None,
                table: Ident("stats".into()),
            },
            alias: None,
        },
        joins: vec![],
        where_: Some(Cond::Cmp {
            column: col("from"),
            op: CmpOp::Eq,
            value: Lit::Str("x".into()),
        }),
        group_by: None,
        order_by: vec![],
        limit: None,
        offset: None,
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

#[test]
fn update_and_delete_one_row_by_id() {
    let sql = "UPDATE crm.deals SET stage = 'Won', amount = 12000, \"closed at\" = NULL WHERE ROW_ID = '00000000-0000-0000-0000-0000000000a1'";

    let expected = Statement::Update(Update {
        table: TableName {
            database: Some(Ident("crm".into())),
            table: Ident("deals".into()),
        },
        assignments: vec![
            (Ident("stage".into()), Lit::Str("Won".into())),
            (Ident("amount".into()), Lit::Num(12000.0)),
            (Ident("closed at".into()), Lit::Null),
        ],
        row_id: "00000000-0000-0000-0000-0000000000a1".into(),
    });
    assert_eq!(parse(sql).unwrap(), expected);

    let sql = "delete from deals where row_id = '00000000-0000-0000-0000-0000000000a1';";
    let expected = Statement::Delete(Delete {
        table: TableName {
            database: None,
            table: Ident("deals".into()),
        },
        row_id: "00000000-0000-0000-0000-0000000000a1".into(),
    });
    assert_eq!(parse(sql).unwrap(), expected);
}

#[test]
fn list_values_default_values_and_limit_offset() {
    let sql = "UPDATE crm.deals SET tags = ['vip', 'renewal'], owner = ['macro|sam@example.com'] WHERE row_id = '00000000-0000-0000-0000-0000000000a1'";
    let expected = Statement::Update(Update {
        table: TableName {
            database: Some(Ident("crm".into())),
            table: Ident("deals".into()),
        },
        assignments: vec![
            (
                Ident("tags".into()),
                Lit::List(vec![Lit::Str("vip".into()), Lit::Str("renewal".into())]),
            ),
            (
                Ident("owner".into()),
                Lit::List(vec![Lit::Str("macro|sam@example.com".into())]),
            ),
        ],
        row_id: "00000000-0000-0000-0000-0000000000a1".into(),
    });
    assert_eq!(parse(sql).unwrap(), expected);

    let sql = "INSERT INTO crm.deals DEFAULT VALUES";
    let expected = Statement::Insert(Insert {
        table: TableName {
            database: Some(Ident("crm".into())),
            table: Ident("deals".into()),
        },
        columns: vec![],
        rows: vec![vec![]],
    });
    assert_eq!(parse(sql).unwrap(), expected);

    let sql = "SELECT name FROM crm.deals ORDER BY name LIMIT 10 OFFSET 20";
    let expected = Statement::Select(Select {
        distinct: false,
        aliases: vec![],
        items: vec![Item::Column(ColumnRef {
            table: None,
            column: Ident("name".into()),
        })],
        from: FromItem {
            table: TableName {
                database: Some(Ident("crm".into())),
                table: Ident("deals".into()),
            },
            alias: None,
        },
        joins: vec![],
        where_: None,
        group_by: None,
        order_by: vec![OrderBy {
            key: OrderKey::Column(ColumnRef {
                table: None,
                column: Ident("name".into()),
            }),
            dir: Dir::Asc,
        }],
        limit: Some(10),
        offset: Some(20),
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
            "SELECT d.name FROM crm.deals d JOIN crm.people p WHERE p.id = d.owner",
            49..54,
            "expected ON after the joined table, found WHERE",
        ),
        (
            "SELECT d.name FROM crm.deals d JOIN crm.people p ON p.id LIKE d.owner",
            57..61,
            "expected = between the two join columns, found LIKE",
        ),
        (
            "SELECT d.name FROM crm.deals d LEFT crm.people p ON p.id = d.owner",
            36..39,
            "expected JOIN after LEFT, found \"crm\"",
        ),
        (
            "SELECT name AS FROM crm.deals",
            15..19,
            "expected a name for the column after AS, found FROM",
        ),
        (
            "SELECT d.name FROM crm.deals d, crm.people p",
            30..31,
            "tables are combined with JOIN … ON a.column = b.row_id, not a comma",
        ),
        (
            "SELECT stage, SUM(amount) FROM crm.deals GROUP BY stage HAVING SUM(amount) > 1",
            56..62,
            "expected end of statement, found \"HAVING\"",
        ),
        (
            "SELECT name FROM crm.deals WHERE owner IN (SELECT id FROM crm.people)",
            43..49,
            "subqueries are not supported: run the inner SELECT on its own first and use the values it returns",
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
            "expected ( and the column list, or DEFAULT VALUES, after the table name, found VALUES",
        ),
        (
            "INSERT INTO crm.deals (name, stage) VALUES ('Acme', 'Won'), ('Globex')",
            60..70,
            "row 2 has 1 values but 2 columns were listed",
        ),
        (
            "UPDATE crm.deals SET stage = 'Won'",
            34..34,
            "expected WHERE row_id = '<id>' (UPDATE changes one row at a time), found end of statement",
        ),
        (
            "UPDATE crm.deals SET stage = 'Won' WHERE stage = 'Lead'",
            41..46,
            "expected row_id (UPDATE changes one row at a time), found \"stage\"",
        ),
        (
            "DELETE FROM crm.deals",
            21..21,
            "expected WHERE row_id = '<id>' (DELETE changes one row at a time), found end of statement",
        ),
        (
            "DELETE FROM crm.deals WHERE row_id = 'a' AND stage = 'Won'",
            41..44,
            "expected end of statement, found AND",
        ),
        (
            "MERGE INTO crm.deals USING x",
            0..5,
            "expected SELECT, INSERT, UPDATE or DELETE, found \"MERGE\"",
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
