use filter_ast::Expr;
use item_filters::ast::properties::{EntityRefId, PropertiesLiteral, PropertyMatchValue};

use super::*;
use crate::resolve::{AggFn, CmpOp, Dir, Order, OrderKey, Query, Value, compile};
use crate::test_support::{catalog, *};

fn select(sql: &str) -> SelectQuery {
    match compile(&catalog(), sql).unwrap() {
        Query::Select(select) => select,
        Query::Insert(_) | Query::Update(_) | Query::Delete(_) => panic!("not a SELECT"),
    }
}

fn option(column: Uuid, option: Uuid) -> Expr<PropertiesLiteral> {
    Expr::Literal(PropertiesLiteral {
        property_definition_id: column,
        entity_type: None,
        value: PropertyMatchValue::SelectOption(option),
    })
}

fn entity(column: Uuid, id: &str) -> Expr<PropertiesLiteral> {
    Expr::Literal(PropertiesLiteral {
        property_definition_id: column,
        entity_type: None,
        value: PropertyMatchValue::EntityRef(EntityRefId::new(id.into()).unwrap()),
    })
}

// ---- the plan for each pushdown situation: full literals -------------------

#[test]
fn pushable_and_residual_conjuncts_are_divided() {
    let plan = split(
        &catalog(),
        select(
            "SELECT name, amount FROM crm.deals WHERE stage = 'Won' AND amount > 5000 ORDER BY amount DESC",
        ),
    );

    assert_eq!(
        plan,
        Plan {
            gql: GqlQuery::Soup {
                table: DEALS,
                propf: Some(option(STAGE, WON)),
            },
            needs: vec![NAME, AMOUNT],
            residual: Some(Filter::Cmp {
                column: AMOUNT,
                op: CmpOp::Gt,
                value: Value::Number(5000.0),
            }),
            shape: Shape::Rows(vec![NAME, AMOUNT]),
            order_by: vec![Order {
                key: OrderKey::Column(AMOUNT),
                dir: Dir::Desc,
            }],
        }
    );
}

#[test]
fn an_or_with_a_residual_side_pushes_nothing() {
    let plan = split(
        &catalog(),
        select("SELECT name FROM crm.deals WHERE stage = 'Won' OR amount > 5000"),
    );

    assert_eq!(
        plan,
        Plan {
            gql: GqlQuery::Soup {
                table: DEALS,
                propf: None,
            },
            needs: vec![NAME, STAGE, AMOUNT],
            residual: Some(Filter::Or(vec![
                Filter::Cmp {
                    column: STAGE,
                    op: CmpOp::Eq,
                    value: Value::Option(WON),
                },
                Filter::Cmp {
                    column: AMOUNT,
                    op: CmpOp::Gt,
                    value: Value::Number(5000.0),
                },
            ])),
            shape: Shape::Rows(vec![NAME]),
            order_by: vec![],
        }
    );
}

#[test]
fn in_lists_nested_ors_and_has_push_as_one_expression() {
    let plan = split(
        &catalog(),
        select(
            "SELECT name FROM crm.deals
             WHERE (stage IN ('Won', 'Lead') OR owner = 'macro|sam@example.com')
               AND tags HAS 'vip'
               AND name LIKE 'A%'",
        ),
    );

    assert_eq!(
        plan,
        Plan {
            gql: GqlQuery::Soup {
                table: DEALS,
                propf: Some(Expr::and(
                    Expr::or(
                        Expr::or(option(STAGE, WON), option(STAGE, LEAD)),
                        entity(OWNER, "macro|sam@example.com"),
                    ),
                    option(TAGS, VIP),
                )),
            },
            needs: vec![NAME],
            residual: Some(Filter::Like {
                column: NAME,
                pattern: "A%".into(),
                negated: false,
            }),
            shape: Shape::Rows(vec![NAME]),
            order_by: vec![],
        }
    );
}

#[test]
fn negations_stay_residual_because_soup_not_keeps_empty_cells() {
    let plan = split(
        &catalog(),
        select(
            "SELECT name FROM crm.deals
             WHERE stage != 'Won' AND tags NOT HAS 'vip' AND owner NOT IN ('macro|sam@example.com') AND done = TRUE",
        ),
    );

    assert_eq!(
        plan,
        Plan {
            gql: GqlQuery::Soup {
                table: DEALS,
                propf: None,
            },
            needs: vec![NAME, STAGE, TAGS, OWNER, DONE],
            residual: Some(Filter::And(vec![
                Filter::Cmp {
                    column: STAGE,
                    op: CmpOp::Ne,
                    value: Value::Option(WON),
                },
                Filter::Has {
                    column: TAGS,
                    value: Value::Option(VIP),
                    negated: true,
                },
                Filter::In {
                    column: OWNER,
                    values: vec![Value::Entity("macro|sam@example.com".into())],
                    negated: true,
                },
                Filter::Cmp {
                    column: DONE,
                    op: CmpOp::Eq,
                    value: Value::Bool(true),
                },
            ])),
            shape: Shape::Rows(vec![NAME]),
            order_by: vec![],
        }
    );
}

#[test]
fn count_per_select_group_needs_no_rows() {
    let plan = split(
        &catalog(),
        select(
            "SELECT stage, COUNT(*) FROM crm.deals WHERE owner = 'macro|sam@example.com' GROUP BY stage ORDER BY 2 DESC",
        ),
    );

    assert_eq!(
        plan,
        Plan {
            gql: GqlQuery::GroupSoup {
                table: DEALS,
                propf: Some(entity(OWNER, "macro|sam@example.com")),
                group_by: STAGE,
            },
            needs: vec![],
            residual: None,
            shape: Shape::Aggregate {
                group_by: Some(STAGE),
                items: vec![
                    SelectItem::Column(STAGE),
                    SelectItem::Agg {
                        func: AggFn::Count,
                        column: None,
                    },
                ],
            },
            order_by: vec![Order {
                key: OrderKey::Item(1),
                dir: Dir::Desc,
            }],
        }
    );
}

#[test]
fn any_other_aggregate_or_a_residual_filter_fetches_rows_and_folds() {
    let plan = split(
        &catalog(),
        select(
            "SELECT owner, SUM(amount), COUNT(*)
             FROM crm.deals
             WHERE stage IN ('Won', 'Lead') AND amount > 5000 AND \"closed at\" IS NOT NULL
             GROUP BY owner
             ORDER BY 2 DESC, owner ASC",
        ),
    );

    assert_eq!(
        plan,
        Plan {
            gql: GqlQuery::Soup {
                table: DEALS,
                propf: Some(Expr::or(option(STAGE, WON), option(STAGE, LEAD))),
            },
            needs: vec![OWNER, AMOUNT, CLOSED_AT],
            residual: Some(Filter::And(vec![
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
            shape: Shape::Aggregate {
                group_by: Some(OWNER),
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
            },
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
        }
    );

    // The same COUNT-only shape over a residual filter must fetch rows too:
    // bins are counted before we could apply `amount > 5000`.
    let plan = split(
        &catalog(),
        select("SELECT stage, COUNT(*) FROM crm.deals WHERE amount > 5000 GROUP BY stage"),
    );
    assert_eq!(
        plan.gql,
        GqlQuery::Soup {
            table: DEALS,
            propf: None,
        }
    );
    assert_eq!(plan.needs, vec![STAGE, AMOUNT]);
}

#[test]
fn a_whole_table_read_pushes_nothing_and_needs_every_column() {
    let plan = split(&catalog(), select("SELECT * FROM crm.people"));

    assert_eq!(
        plan,
        Plan {
            gql: GqlQuery::Soup {
                table: PEOPLE,
                propf: None,
            },
            needs: vec![NAME],
            residual: None,
            shape: Shape::Rows(vec![NAME]),
            order_by: vec![],
        }
    );
}

#[test]
fn propf_serializes_to_the_soup_wire_form() {
    let plan = split(
        &catalog(),
        select(
            "SELECT name FROM crm.deals WHERE stage = 'Won' AND owner = 'macro|sam@example.com'",
        ),
    );
    let GqlQuery::Soup { propf, .. } = plan.gql else {
        panic!("expected a soup query");
    };

    assert_eq!(
        serde_json::to_value(propf.unwrap()).unwrap(),
        serde_json::json!({
            "&": [
                { "l": { "pd": "00000000-0000-0000-0000-000000000003", "v": { "so": "00000000-0000-0000-0000-000000000030" } } },
                { "l": { "pd": "00000000-0000-0000-0000-000000000005", "v": { "er": "macro|sam@example.com" } } }
            ]
        })
    );
}
