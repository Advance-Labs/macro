use filter_ast::Expr;
use item_filters::ast::properties::{EntityRefId, PropertiesLiteral, PropertyMatchValue};

use super::*;
use crate::catalog::{PEOPLE_EMAIL, PEOPLE_ID, PEOPLE_NAME, PEOPLE_TABLE};
use crate::resolve::{
    AggFn, CmpOp, Dir, JoinKind, Order, OrderKey, Query, Relation, Value, column_key, compile,
    row_id_key,
};
use crate::test_support::{catalog, *};

/// A one-table plan with the bindings the query produced.
fn single(
    plan: &Plan,
    table: Uuid,
    gql: GqlQuery,
    needs: Vec<Uuid>,
    residual: Option<Filter>,
    shape: Shape,
    order_by: Vec<Order>,
) -> Plan {
    let name = catalog()
        .tables
        .iter()
        .find(|candidate| candidate.id == table)
        .map(|table| table.name.clone())
        .unwrap();
    Plan {
        relations: vec![RelationPlan {
            relation: Relation { table, alias: name },
            gql,
            needs,
        }],
        joins: vec![],
        residual,
        distinct: false,
        shape,
        order_by,
        limit: None,
        offset: None,
        bindings: plan.bindings.clone(),
    }
}

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
        single(
            &plan,
            DEALS,
            GqlQuery::Soup {
                table: DEALS,
                propf: Some(option(STAGE, WON)),
                key_hint: None,
            },
            vec![NAME, AMOUNT],
            Some(Filter::Cmp {
                column: AMOUNT,
                op: CmpOp::Gt,
                value: Value::Number(5000.0),
            }),
            Shape::Rows(vec![NAME, AMOUNT]),
            vec![Order {
                key: OrderKey::Column(AMOUNT),
                dir: Dir::Desc,
            }],
        )
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
        single(
            &plan,
            DEALS,
            GqlQuery::Soup {
                table: DEALS,
                propf: None,
                key_hint: None,
            },
            vec![NAME, STAGE, AMOUNT],
            Some(Filter::Or(vec![
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
            Shape::Rows(vec![NAME]),
            vec![],
        )
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
        single(
            &plan,
            DEALS,
            GqlQuery::Soup {
                table: DEALS,
                propf: Some(Expr::and(
                    Expr::or(
                        Expr::or(option(STAGE, WON), option(STAGE, LEAD)),
                        entity(OWNER, "macro|sam@example.com"),
                    ),
                    option(TAGS, VIP),
                )),
                key_hint: None,
            },
            vec![NAME],
            Some(Filter::Like {
                column: NAME,
                pattern: "A%".into(),
                negated: false,
            }),
            Shape::Rows(vec![NAME]),
            vec![],
        )
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
        single(
            &plan,
            DEALS,
            GqlQuery::Soup {
                table: DEALS,
                propf: None,
                key_hint: None,
            },
            vec![NAME, STAGE, TAGS, OWNER, DONE],
            Some(Filter::And(vec![
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
            Shape::Rows(vec![NAME]),
            vec![],
        )
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
        single(
            &plan,
            DEALS,
            GqlQuery::GroupSoup {
                table: DEALS,
                propf: Some(entity(OWNER, "macro|sam@example.com")),
                group_by: STAGE,
            },
            vec![],
            None,
            Shape::Aggregate {
                group_by: Some(STAGE),
                items: vec![
                    SelectItem::Column(STAGE),
                    SelectItem::Agg {
                        func: AggFn::Count,
                        column: None,
                    },
                ],
            },
            vec![Order {
                key: OrderKey::Item(1),
                dir: Dir::Desc,
            }],
        )
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
        single(
            &plan,
            DEALS,
            GqlQuery::Soup {
                table: DEALS,
                propf: Some(Expr::or(option(STAGE, WON), option(STAGE, LEAD))),
                key_hint: None,
            },
            vec![OWNER, AMOUNT, CLOSED_AT],
            Some(Filter::And(vec![
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
            Shape::Aggregate {
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
            vec![
                Order {
                    key: OrderKey::Item(1),
                    dir: Dir::Desc,
                },
                Order {
                    key: OrderKey::Column(OWNER),
                    dir: Dir::Asc,
                },
            ],
        )
    );

    // The same COUNT-only shape over a residual filter must fetch rows too:
    // bins are counted before we could apply `amount > 5000`.
    let plan = split(
        &catalog(),
        select("SELECT stage, COUNT(*) FROM crm.deals WHERE amount > 5000 GROUP BY stage"),
    );
    assert_eq!(
        plan.relations[0].gql,
        GqlQuery::Soup {
            table: DEALS,
            propf: None,
            key_hint: None,
        }
    );
    assert_eq!(plan.relations[0].needs, vec![STAGE, AMOUNT]);
}

#[test]
fn a_whole_table_read_pushes_nothing_and_needs_every_column() {
    let plan = split(&catalog(), select("SELECT * FROM crm.people"));

    assert_eq!(
        plan,
        single(
            &plan,
            PEOPLE,
            GqlQuery::Soup {
                table: PEOPLE,
                propf: None,
                key_hint: None,
            },
            vec![NAME],
            None,
            Shape::Rows(vec![NAME]),
            vec![],
        )
    );
}

// ---- joins: one fetch per relation ------------------------------------------

#[test]
fn each_relation_gets_its_own_pushdown_and_needs() {
    let plan = split(
        &catalog(),
        select(
            "SELECT DISTINCT p.email
             FROM macro.tasks t
             JOIN macro.people p ON t.assignees = p.id
             LEFT JOIN crm.deals d ON t.deal = d.row_id
             WHERE t.priority = 'High' AND d.stage = 'Won' AND d.amount > 100 AND p.name LIKE 'A%'",
        ),
    );
    let people_id = column_key(1, PEOPLE_ID);
    let people_email = column_key(1, PEOPLE_EMAIL);
    let people_name = column_key(1, PEOPLE_NAME);
    let deals_stage = column_key(2, STAGE);
    let deals_amount = column_key(2, AMOUNT);

    assert_eq!(
        plan.relations,
        vec![
            RelationPlan {
                relation: Relation {
                    table: TASKS,
                    alias: "t".into(),
                },
                gql: GqlQuery::Soup {
                    table: TASKS,
                    propf: Some(option(PRIORITY, HIGH)),
                    key_hint: None,
                },
                needs: vec![ASSIGNEES, DEAL],
            },
            RelationPlan {
                relation: Relation {
                    table: PEOPLE_TABLE,
                    alias: "p".into(),
                },
                gql: GqlQuery::People { ids: None },
                needs: vec![people_email, people_name, people_id],
            },
            RelationPlan {
                relation: Relation {
                    table: DEALS,
                    alias: "d".into(),
                },
                // The pushed literal names the property, not the key.
                gql: GqlQuery::Soup {
                    table: DEALS,
                    propf: Some(option(STAGE, WON)),
                    key_hint: None,
                },
                needs: vec![deals_amount, row_id_key(DEALS)],
            },
        ]
    );
    assert_eq!(
        plan.joins,
        vec![
            JoinPlan {
                relation: 1,
                kind: JoinKind::Inner,
                on: vec![(ASSIGNEES, people_id)],
            },
            JoinPlan {
                relation: 2,
                kind: JoinKind::Left,
                on: vec![(DEAL, row_id_key(DEALS))],
            },
        ]
    );
    assert_eq!(
        plan.residual,
        Some(Filter::And(vec![
            Filter::Cmp {
                column: deals_amount,
                op: CmpOp::Gt,
                value: Value::Number(100.0),
            },
            Filter::Like {
                column: people_name,
                pattern: "A%".into(),
                negated: false,
            },
        ]))
    );
    assert!(plan.distinct);
    assert_eq!(plan.shape, Shape::Rows(vec![people_email]));
    assert_eq!(
        plan.column(&catalog(), deals_amount).map(|c| c.id),
        Some(AMOUNT)
    );
    assert_eq!(plan.column(&catalog(), row_id_key(DEALS)), None);
    assert_eq!(plan.table(), TASKS);
    assert_eq!(deals_stage, column_key(2, STAGE));
}

#[test]
fn a_condition_spanning_relations_stays_residual_and_bins_need_one_relation() {
    let plan = split(
        &catalog(),
        select(
            "SELECT t.priority, COUNT(*) FROM macro.tasks t JOIN macro.people p ON t.assignees = p.id
             WHERE t.priority = 'High' OR p.name = 'Sam' GROUP BY t.priority",
        ),
    );
    // COUNT per select group would be bins over one table; over a join the
    // rows are needed.
    assert!(matches!(
        plan.relations[0].gql,
        GqlQuery::Soup { propf: None, .. }
    ));
    assert!(plan.residual.is_some());
    assert_eq!(plan.relations[0].needs, vec![PRIORITY, ASSIGNEES]);

    let plan = split(
        &catalog(),
        select("SELECT DISTINCT stage, COUNT(*) FROM crm.deals GROUP BY stage"),
    );
    assert!(matches!(plan.relations[0].gql, GqlQuery::Soup { .. }));
}

#[test]
fn a_row_id_condition_never_pushes_down() {
    let plan = split(
        &catalog(),
        select("SELECT name FROM crm.deals WHERE row_id = '00000000-0000-0000-0000-0000000000a1'"),
    );
    assert!(matches!(
        plan.relations[0].gql,
        GqlQuery::Soup { propf: None, .. }
    ));
    assert_eq!(plan.relations[0].needs, vec![NAME, row_id_key(DEALS)]);
}

#[test]
fn propf_serializes_to_the_soup_wire_form() {
    let plan = split(
        &catalog(),
        select(
            "SELECT name FROM crm.deals WHERE stage = 'Won' AND owner = 'macro|sam@example.com'",
        ),
    );
    let GqlQuery::Soup { propf, .. } = plan.relations.into_iter().next().unwrap().gql else {
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
