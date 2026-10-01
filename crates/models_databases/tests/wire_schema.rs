//! The OpenAPI schema names the fields serde actually writes. utoipa reads
//! some serde attributes and not others (it ignores `rename_all_fields`), so
//! a sample of every op, result and view is serialized and walked against
//! the schema it claims.

use chrono::{TimeZone, Utc};
use models_databases::views::{
    CardPosition, Conjunction, DatabaseView, DateOperator, FilterCondition, FilterGroup,
    FilterNode, FilterTest, Lane, NewView, NumberOperator, PresenceOperator, SetOperator,
    SortDirection, SortKey, TextOperator, ViewColumn, ViewLayout, ViewPosition, ViewQuery,
};
use models_databases::{
    CellValue, CellWrite, ColumnId, ColumnKind, DatabaseId, DatabaseOp, EntityKind, EntityRef,
    NewColumn, NewOption, OpResult, OptionId, OptionRef, PropertyId, RowChange, RowChanges, RowId,
    TableId, TableVersion, VersionedTable, ViewId,
};
use serde_json::{Map, Value};
use utoipa::OpenApi;
use uuid::Uuid;

#[derive(OpenApi)]
#[openapi(components(schemas(DatabaseOp, OpResult)))]
struct Schemas;

const DATABASE: DatabaseId = DatabaseId::from_uuid(Uuid::from_u128(0xdb));
const TABLE: TableId = TableId::from_uuid(Uuid::from_u128(0x7ab1));
const VIEW: ViewId = ViewId::from_uuid(Uuid::from_u128(0x71e3));
const STATUS: ColumnId = ColumnId::from_uuid(Uuid::from_u128(0xc01b));
const NAME: ColumnId = ColumnId::from_uuid(Uuid::from_u128(0xc01a));
const ROW: RowId = RowId::from_uuid(Uuid::from_u128(0x5a11));
const OTHER_ROW: RowId = RowId::from_uuid(Uuid::from_u128(0xa1e8));
const DONE: OptionId = OptionId::from_uuid(Uuid::from_u128(0xd0e));
const PROPERTY: PropertyId = PropertyId::from_uuid(Uuid::from_u128(0x9e0));

fn every_filter_test() -> FilterGroup {
    let tests = vec![
        FilterTest::Presence {
            operator: PresenceOperator::IsEmpty,
        },
        FilterTest::Text {
            operator: TextOperator::Contains,
            value: "Sam".into(),
        },
        FilterTest::Number {
            operator: NumberOperator::GreaterThan,
            value: 2.5,
        },
        FilterTest::Date {
            operator: DateOperator::Before,
            value: Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap(),
        },
        FilterTest::Checkbox { checked: true },
        FilterTest::Options {
            operator: SetOperator::IsAnyOf,
            options: vec![DONE],
        },
        FilterTest::Entities {
            operator: SetOperator::HasAll,
            entities: vec!["doc".into()],
        },
    ];
    FilterGroup {
        conjunction: Conjunction::And,
        conditions: vec![FilterNode::Group(FilterGroup {
            conjunction: Conjunction::Or,
            conditions: tests
                .into_iter()
                .map(|test| FilterNode::Condition(FilterCondition { column: NAME, test }))
                .collect(),
        })],
    }
}

fn board() -> ViewLayout {
    ViewLayout::Board {
        group_by: STATUS,
        lanes: vec![
            Lane {
                option: Some(DONE),
                hidden: false,
            },
            Lane {
                option: None,
                hidden: true,
            },
        ],
        card_fields: vec![NAME],
        hide_empty_lanes: true,
    }
}

fn table() -> ViewLayout {
    ViewLayout::Table {
        columns: vec![ViewColumn {
            column: NAME,
            width: Some(120),
            hidden: false,
        }],
    }
}

fn query() -> ViewQuery {
    ViewQuery {
        filter: Some(every_filter_test()),
        sort: vec![SortKey {
            column: NAME,
            direction: SortDirection::Descending,
        }],
    }
}

fn every_cell_value() -> Vec<CellWrite> {
    [
        CellValue::Text("Sam".into()),
        CellValue::Number(2.5),
        CellValue::Boolean(true),
        CellValue::Date(Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap()),
        CellValue::Link(vec!["https://macro.com".into()]),
        CellValue::Options(vec![OptionRef::Id(DONE), OptionRef::Label("Going".into())]),
        CellValue::Entities(vec![EntityRef {
            entity_type: EntityKind::CallRecord,
            entity_id: "call".into(),
        }]),
        CellValue::Rows(vec![ROW]),
        CellValue::Clear,
    ]
    .into_iter()
    .map(|value| CellWrite {
        column: NAME,
        value,
    })
    .collect()
}

fn ops() -> Vec<DatabaseOp> {
    let mut ops = vec![
        DatabaseOp::CreateTable {
            id: TABLE,
            name: "Guests".into(),
        },
        DatabaseOp::RenameTable {
            table: TABLE,
            name: "People".into(),
            previous_name: Some("Guests".into()),
        },
        DatabaseOp::RenameTable {
            table: TABLE,
            name: "People".into(),
            previous_name: None,
        },
        DatabaseOp::DeleteTable { table: TABLE },
        DatabaseOp::ReorderTables { order: vec![TABLE] },
        DatabaseOp::CreateColumn {
            table: TABLE,
            id: STATUS,
            definition: NewColumn::New {
                name: "Status".into(),
                kind: ColumnKind::Select { multi: false },
                options: vec![NewOption {
                    id: DONE,
                    label: "Done".into(),
                }],
                infer_type: false,
            },
            after: Some(NAME),
        },
        DatabaseOp::CreateColumn {
            table: TABLE,
            id: NAME,
            definition: NewColumn::New {
                name: "Name".into(),
                kind: ColumnKind::Text,
                options: vec![],
                infer_type: true,
            },
            after: None,
        },
        DatabaseOp::CreateColumn {
            table: TABLE,
            id: STATUS,
            definition: NewColumn::Existing { property: PROPERTY },
            after: None,
        },
        DatabaseOp::RenameColumn {
            table: TABLE,
            column: NAME,
            name: "Title".into(),
            previous_name: Some("Name".into()),
        },
        DatabaseOp::DeleteColumn {
            table: TABLE,
            column: NAME,
        },
        DatabaseOp::ReorderColumns {
            table: TABLE,
            order: vec![STATUS, NAME],
        },
        DatabaseOp::AddOptions {
            table: TABLE,
            column: STATUS,
            options: vec![NewOption {
                id: DONE,
                label: "Done".into(),
            }],
        },
        DatabaseOp::InsertRows {
            table: TABLE,
            rows: vec![every_cell_value()],
            create_missing_options: true,
        },
        DatabaseOp::UpdateRows {
            table: TABLE,
            changes: RowChanges::Uniform {
                rows: vec![ROW],
                cells: every_cell_value(),
            },
            create_missing_options: false,
        },
        DatabaseOp::UpdateRows {
            table: TABLE,
            changes: RowChanges::PerRow {
                rows: vec![RowChange {
                    row: ROW,
                    cells: every_cell_value(),
                }],
            },
            create_missing_options: false,
        },
        DatabaseOp::DeleteRows {
            table: TABLE,
            rows: vec![ROW],
        },
        DatabaseOp::UpdateOption {
            table: TABLE,
            column: STATUS,
            option: DONE,
            label: Some("Done".into()),
            color: Some(Some("#0091FF".into())),
        },
        DatabaseOp::UpdateOption {
            table: TABLE,
            column: STATUS,
            option: DONE,
            label: None,
            color: Some(None),
        },
        DatabaseOp::DeleteOption {
            table: TABLE,
            column: STATUS,
            option: DONE,
        },
        DatabaseOp::CreateView {
            table: TABLE,
            view: NewView {
                name: "Board".into(),
                query: query(),
                layout: board(),
            },
        },
        DatabaseOp::UpdateView {
            table: TABLE,
            view: VIEW,
            name: Some("Grid".into()),
            query: Some(query()),
            layout: Some(table()),
        },
        DatabaseOp::UpdateView {
            table: TABLE,
            view: VIEW,
            name: None,
            query: None,
            layout: Some(board()),
        },
        DatabaseOp::DeleteView {
            table: TABLE,
            view: VIEW,
        },
        DatabaseOp::ReorderViews {
            table: TABLE,
            order: vec![VIEW],
        },
        DatabaseOp::MoveCard {
            table: TABLE,
            view: VIEW,
            row: ROW,
            lane: Some(DONE),
            before: Some(OTHER_ROW),
            after: None,
        },
        DatabaseOp::MoveCard {
            table: TABLE,
            view: VIEW,
            row: ROW,
            lane: None,
            before: None,
            after: None,
        },
    ];
    let kinds = [
        ColumnKind::Text,
        ColumnKind::Number,
        ColumnKind::Boolean,
        ColumnKind::Date,
        ColumnKind::Link,
        ColumnKind::Select { multi: true },
        ColumnKind::SelectNumber { multi: false },
        ColumnKind::Tag,
        ColumnKind::Entity {
            target: EntityKind::Task,
            multi: true,
        },
        ColumnKind::Relation {
            database: DATABASE,
            table: TABLE,
        },
    ];
    ops.extend(kinds.into_iter().map(|to| DatabaseOp::ChangeColumnType {
        table: TABLE,
        column: NAME,
        to,
        clear_invalid: true,
    }));
    ops
}

fn results() -> Vec<OpResult> {
    let version = TableVersion(7);
    let at = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
    vec![
        OpResult::TableCreated {
            table: TABLE,
            table_version: version,
        },
        OpResult::TableRenamed {
            table_version: version,
        },
        OpResult::TableDeleted { table: TABLE },
        OpResult::TablesReordered {
            tables: vec![VersionedTable {
                table: TABLE,
                version,
            }],
        },
        OpResult::ColumnCreated {
            column: NAME,
            table_version: version,
        },
        OpResult::ColumnRenamed {
            table_version: version,
        },
        OpResult::ColumnDeleted {
            table_version: version,
        },
        OpResult::ColumnsReordered {
            table_version: version,
        },
        OpResult::OptionsAdded {
            table_version: version,
            added: vec![DONE],
        },
        OpResult::RowsWritten {
            table_version: version,
            inserted: vec![ROW],
            affected: 1,
        },
        OpResult::ColumnTyped {
            table_version: version,
            cleared_cells: 1,
            trimmed_cells: 2,
        },
        OpResult::OptionChanged {
            table_version: version,
        },
        OpResult::ViewWritten {
            table_version: version,
            view: Box::new(DatabaseView {
                id: VIEW,
                database_id: DATABASE,
                table_id: TABLE,
                name: "Board".into(),
                position: "80".parse().unwrap(),
                query: query(),
                layout: board(),
                created_at: at,
                updated_at: at,
            }),
        },
        OpResult::ViewDeleted {
            table_version: version,
        },
        OpResult::ViewsReordered {
            table_version: version,
            positions: vec![ViewPosition {
                view: VIEW,
                position: "80".parse().unwrap(),
            }],
        },
        OpResult::CardMoved {
            table_version: version,
            positions: vec![
                CardPosition {
                    row: ROW,
                    lane: Some(DONE),
                    position: "80".parse().unwrap(),
                },
                CardPosition {
                    row: OTHER_ROW,
                    lane: None,
                    position: "8180".parse().unwrap(),
                },
            ],
        },
    ]
}

/// Why `value` is not what `schema` describes, by field name: an object's
/// keys must be the schema's properties, its required ones all present.
fn mismatch(
    value: &Value,
    schema: &Value,
    components: &Map<String, Value>,
    path: &str,
) -> Option<String> {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let name = reference.trim_start_matches("#/components/schemas/");
        return mismatch(value, &components[name], components, path);
    }
    for combinator in ["oneOf", "anyOf"] {
        if let Some(alternatives) = schema.get(combinator).and_then(Value::as_array) {
            let reasons: Vec<String> = alternatives
                .iter()
                .filter_map(|alternative| mismatch(value, alternative, components, path))
                .collect();
            return (reasons.len() == alternatives.len())
                .then(|| format!("{path}: no alternative fits: {}", reasons.join("; ")));
        }
    }
    if let Some(parts) = schema.get("allOf").and_then(Value::as_array) {
        return parts
            .iter()
            .find_map(|part| mismatch(value, part, components, path));
    }
    match value {
        Value::Null => {
            let nullable = schema["type"] == "null"
                || schema["type"]
                    .as_array()
                    .is_some_and(|types| types.contains(&Value::from("null")));
            (!nullable).then(|| format!("{path}: null where the schema allows none"))
        }
        Value::Array(items) => items.iter().enumerate().find_map(|(index, item)| {
            mismatch(
                item,
                &schema["items"],
                components,
                &format!("{path}[{index}]"),
            )
        }),
        Value::Object(fields) => {
            let properties = schema["properties"].as_object()?;
            if let Some(unknown) = fields.keys().find(|key| !properties.contains_key(*key)) {
                return Some(format!(
                    "{path}: `{unknown}` is not among the schema's {:?}",
                    properties.keys().collect::<Vec<_>>()
                ));
            }
            let required = schema["required"].as_array().cloned().unwrap_or_default();
            if let Some(missing) = required
                .iter()
                .filter_map(Value::as_str)
                .find(|key| !fields.contains_key(*key))
            {
                return Some(format!("{path}: required `{missing}` is not written"));
            }
            fields.iter().find_map(|(key, field)| {
                mismatch(
                    field,
                    &properties[key],
                    components,
                    &format!("{path}.{key}"),
                )
            })
        }
        _ => None,
    }
}

fn components() -> Map<String, Value> {
    let openapi = serde_json::to_value(Schemas::openapi()).unwrap();
    openapi["components"]["schemas"]
        .as_object()
        .unwrap()
        .clone()
}

#[test]
fn every_op_writes_the_fields_its_schema_names() {
    let components = components();
    for op in ops() {
        let value = serde_json::to_value(&op).unwrap();
        if let Some(reason) = mismatch(&value, &components["DatabaseOp"], &components, "op") {
            panic!("{reason}\nin {value:#}");
        }
    }
}

#[test]
fn every_result_writes_the_fields_its_schema_names() {
    let components = components();
    for result in results() {
        let value = serde_json::to_value(&result).unwrap();
        if let Some(reason) = mismatch(&value, &components["OpResult"], &components, "result") {
            panic!("{reason}\nin {value:#}");
        }
    }
}

#[test]
fn a_board_names_its_fields_in_camel_case() {
    let components = components();
    let board = serde_json::to_value(board()).unwrap();
    assert_eq!(board["groupBy"], Value::from(STATUS.to_string()));
    assert_eq!(board["hideEmptyLanes"], Value::from(true));
    assert!(mismatch(&board, &components["ViewLayout"], &components, "board").is_none());
}

#[test]
fn a_card_move_may_leave_out_where_it_lands() {
    let components = components();
    let move_card = components["DatabaseOp"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|variant| variant["properties"]["kind"]["enum"][0] == "move_card")
        .unwrap();
    let required: Vec<&str> = move_card["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(!required.contains(&"before"), "{required:?}");
    assert!(!required.contains(&"after"), "{required:?}");
    assert!(required.contains(&"lane"), "{required:?}");
}
