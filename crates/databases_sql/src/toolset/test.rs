//! The SQL tools over the adapter, over fakes of the services it runs on.

use std::collections::HashMap;

use chrono::{TimeZone, Utc};
use databases::domain::models::{
    Column, ColumnDetail, Database, DatabaseDetail, Table, TableDetail, TableVersion,
};
use models_properties::service::property_definition::PropertyDefinition;
use models_properties::service::property_definition_with_options::PropertyDefinitionWithOptions;
use models_properties::service::property_option::{PropertyOption, PropertyOptionValue};
use models_properties::shared::PropertyOwner;
use std::sync::{Arc, Mutex};

use ai_toolset::schema::generate_validated_input_schema;
use ai_toolset::{AsyncTool, RequestContext, ServiceContext};
use entity_access::domain::models::AccessLevel;
use models_databases::{CellValue, CellWrite, DatabaseOp, OpResult, OptionRef, RowChanges};
use models_properties::service::property_value::PropertyValue;
use models_properties::shared::DataType;
use uuid::Uuid;

use super::*;
use crate::test_support::{
    AppliedOps, FakeAccess, FakeContacts, FakeDatabases, FakeSoup, OWNER, Shared, StoredRow,
    VIEWER, World, sql, user,
};

mod saved_queries;

const OFFSITE: Uuid = Uuid::from_u128(0xdb01);
const GUESTS: Uuid = Uuid::from_u128(0x7a01);
const NAME: Uuid = Uuid::from_u128(0xc001);
const STATUS: Uuid = Uuid::from_u128(0xc002);
const STATUS_COLUMN: Uuid = Uuid::from_u128(0xb002);
const GOING: Uuid = Uuid::from_u128(0xa001);
const MAYBE: Uuid = Uuid::from_u128(0xa002);
const MARIA: Uuid = Uuid::from_u128(0xe001);

/// `Offsite.Guests` with one guest, the owner's; the viewer can read it.
fn world() -> Shared {
    Arc::new(Mutex::new(World {
        databases: vec![DatabaseDetail {
            database: Database {
                id: OFFSITE,
                name: "Offsite".into(),
                owner_id: OWNER.to_string(),
                created_at: Utc::now(),
                trashed_at: None,
            },
            grant: AccessLevel::Owner,
            tables: vec![TableDetail {
                table: Table {
                    id: GUESTS,
                    database_id: OFFSITE,
                    name: "Guests".into(),
                    position: "80".into(),
                    version: TableVersion(1),
                },
                sql_name: "\"Guests\"".into(),
                columns: vec![
                    ColumnDetail {
                        column: Column {
                            id: Uuid::from_u128(0xb001),
                            table_id: GUESTS,
                            property_definition_id: NAME,
                            position: "80".into(),
                            config: None,
                            display_name: None,
                            infer_type: false,
                        },
                        sql_name: "\"Name\"".into(),
                        definition: PropertyDefinitionWithOptions {
                            definition: PropertyDefinition {
                                id: NAME,
                                owner: PropertyOwner::System,
                                display_name: "Name".into(),
                                data_type: DataType::String,
                                is_multi_select: false,
                                specific_entity_type: None,
                                created_at: Utc::now(),
                                updated_at: Utc::now(),
                                is_system: false,
                                is_metadata: false,
                            },
                            property_options: Vec::new(),
                        },
                        writable: true,
                        shared_outside_database: true,
                    },
                    ColumnDetail {
                        column: Column {
                            id: STATUS_COLUMN,
                            table_id: GUESTS,
                            property_definition_id: STATUS,
                            position: "8180".into(),
                            config: None,
                            display_name: None,
                            infer_type: false,
                        },
                        sql_name: "\"Status\"".into(),
                        definition: PropertyDefinitionWithOptions {
                            definition: PropertyDefinition {
                                id: STATUS,
                                owner: PropertyOwner::System,
                                display_name: "Status".into(),
                                data_type: DataType::SelectString,
                                is_multi_select: false,
                                specific_entity_type: None,
                                created_at: Utc::now(),
                                updated_at: Utc::now(),
                                is_system: false,
                                is_metadata: false,
                            },
                            property_options: vec![
                                PropertyOption {
                                    id: GOING,
                                    property_definition_id: STATUS,
                                    display_order: 0,
                                    value: PropertyOptionValue::String("Going".into()),
                                    color: None,
                                    created_at: Utc::now(),
                                    updated_at: Utc::now(),
                                },
                                PropertyOption {
                                    id: MAYBE,
                                    property_definition_id: STATUS,
                                    display_order: 1,
                                    value: PropertyOptionValue::String("Maybe".into()),
                                    color: None,
                                    created_at: Utc::now(),
                                    updated_at: Utc::now(),
                                },
                            ],
                        },
                        writable: true,
                        shared_outside_database: true,
                    },
                ],
                views: Vec::new(),
            }],
        }],
        grants: vec![
            (OWNER, OFFSITE, AccessLevel::Owner),
            (VIEWER, OFFSITE, AccessLevel::View),
        ],
        rows: vec![StoredRow {
            id: MARIA,
            table_id: GUESTS,
            database_id: OFFSITE,
            position: "80".into(),
            created_at: Utc.with_ymd_and_hms(2026, 9, 1, 9, 1, 0).unwrap(),
            cells: vec![
                (NAME, PropertyValue::Str("Maria".into())),
                (STATUS, PropertyValue::SelectOption(vec![MAYBE])),
            ],
        }],
        ..World::default()
    }))
}

#[test]
fn every_tool_schema_is_valid() {
    for (name, validated) in [
        (
            "QueryDatabase",
            generate_validated_input_schema::<QueryDatabase>(),
        ),
        (
            "QueryDatabase",
            generate_validated_input_schema::<ReadOnlyQueryDatabase>(),
        ),
        (
            "SaveDatabaseQuery",
            generate_validated_input_schema::<SaveDatabaseQuery>(),
        ),
    ] {
        assert_eq!(validated.expect("schema should validate").name, name);
    }
}

/// The dialect note is the whole reason a model can write correct SQL on the
/// first try, so it has to actually reach the description.
#[test]
fn query_schema_teaches_the_dialect() {
    let validated =
        generate_validated_input_schema::<QueryDatabase>().expect("schema should validate");
    for expected in ["row_id", "HAS", "DescribeDatabase", "macro.people"] {
        assert!(
            validated.description.contains(expected),
            "description is missing {expected}: {}",
            validated.description
        );
    }
}

/// Every tool has to survive being put in a collection — that is where name
/// conflicts and schema rejections actually surface.
#[test]
fn the_toolsets_build_with_their_tools() {
    let toolset = databases_sql_toolset::<FakeDatabases, FakeAccess, FakeSoup, FakeContacts>();
    assert_eq!(toolset.tools.len(), 2);
    for name in ["QueryDatabase", "SaveDatabaseQuery"] {
        assert!(toolset.tools.contains_key(name), "missing {name}");
    }
    assert!(toolset.user_tools.is_empty());

    let read_only =
        databases_sql_read_only_toolset::<FakeDatabases, FakeAccess, FakeSoup, FakeContacts>();
    assert_eq!(read_only.tools.len(), 1);
    assert!(read_only.tools.contains_key("QueryDatabase"));
}

#[tokio::test]
async fn a_read_answers_typed_cells_row_ids_and_the_versions_it_read() {
    let world = world();
    let response = QueryDatabase {
        sql: "SELECT \"Name\", \"Status\" FROM \"Guests\"".into(),
        database_id: Some(OFFSITE),
        base_versions: None,
        display: None,
    }
    .call(
        ServiceContext(DatabasesSqlToolContext::new(sql(&world))),
        RequestContext::new(user(VIEWER)),
    )
    .await
    .expect("the viewer reads");

    assert_eq!(
        serde_json::to_value(&response).unwrap(),
        serde_json::json!({
            "results": [{
                "columns": [
                    {"name": "Name", "kind": "text"},
                    {
                        "name": "Status",
                        "kind": "select",
                        "options": [
                            {"id": GOING, "label": "Going"},
                            {"id": MAYBE, "label": "Maybe"},
                        ],
                    },
                ],
                "rows": [[
                    {"type": "text", "value": "Maria"},
                    {"type": "options", "value": [MAYBE]},
                ]],
                "rowIds": [MARIA],
            }],
            "changesApplied": 0,
            "readVersions": [{"tableId": GUESTS, "version": 1}],
            "statement": {"kind": "select"},
            "summary": "Returned 1 row.",
        })
    );
}

#[tokio::test]
async fn a_write_runs_as_the_agent_for_the_user_and_guards_its_base_versions() {
    let world = world();
    world
        .lock()
        .unwrap()
        .op_answers
        .push_back(Ok(vec![OpResult::RowsWritten {
            table_version: TableVersion(2),
            inserted: vec![],
            affected: 1,
        }]));
    let request: QueryDatabase = serde_json::from_value(serde_json::json!({
        "sql": "UPDATE \"Guests\" SET \"Status\" = 'Going' WHERE \"Name\" = 'Maria'",
        "databaseId": OFFSITE,
        "baseVersions": [{"tableId": GUESTS, "version": 1}],
    }))
    .unwrap();
    let response = request
        .call(
            ServiceContext(
                DatabasesSqlToolContext::new(sql(&world)).with_actor(bot_id::MACRO_AI_BOT_ID),
            ),
            RequestContext::new(user(OWNER)),
        )
        .await
        .expect("the owner writes");

    assert_eq!(response.changes_applied, 1);
    assert_eq!(response.new_versions, HashMap::from([(GUESTS, 2)]));
    assert_eq!(response.summary, "Applied 1 row change.");
    assert_eq!(
        world.lock().unwrap().applied,
        vec![AppliedOps {
            database: OFFSITE,
            level: AccessLevel::Owner,
            acting_bot: Some(bot_id::MACRO_AI_BOT_ID),
            ops: vec![DatabaseOp::UpdateRows {
                table: GUESTS,
                changes: RowChanges::Uniform {
                    rows: vec![MARIA],
                    cells: vec![CellWrite {
                        column: STATUS_COLUMN,
                        value: CellValue::Options(vec![OptionRef::Label("Going".into())]),
                    }],
                },
                create_missing_options: false,
            }],
        }]
    );

    let stale: QueryDatabase = serde_json::from_value(serde_json::json!({
        "sql": "UPDATE \"Guests\" SET \"Status\" = 'Going' WHERE \"Name\" = 'Maria'",
        "baseVersions": [{"tableId": GUESTS, "version": 0}],
    }))
    .unwrap();
    let error = stale
        .call(
            ServiceContext(DatabasesSqlToolContext::new(sql(&world))),
            RequestContext::new(user(OWNER)),
        )
        .await
        .expect_err("version 0 is stale");
    assert!(
        error.description.contains("changed underneath"),
        "{}",
        error.description
    );
}

/// The compiler's message is the product's broken-query state: it is what lets
/// a model fix the name and retry, so it has to arrive verbatim.
#[tokio::test]
async fn a_sql_error_reaches_the_model_verbatim() {
    let world = world();
    let error = QueryDatabase {
        sql: "SELECT statuz FROM \"Guests\"".into(),
        database_id: Some(OFFSITE),
        base_versions: None,
        display: None,
    }
    .call(
        ServiceContext(DatabasesSqlToolContext::new(sql(&world))),
        RequestContext::new(user(OWNER)),
    )
    .await
    .expect_err("a bad statement is an error");

    assert!(
        error.description.contains("statuz"),
        "{}",
        error.description
    );
    assert!(
        error.description.contains("DescribeDatabase"),
        "the error should say how to recover: {}",
        error.description
    );
}

#[tokio::test]
async fn a_view_grant_reads_as_read_only() {
    let world = world();
    let error = QueryDatabase {
        sql: "DELETE FROM \"Guests\" WHERE \"Name\" = 'Maria'".into(),
        database_id: Some(OFFSITE),
        base_versions: None,
        display: None,
    }
    .call(
        ServiceContext(DatabasesSqlToolContext::new(sql(&world))),
        RequestContext::new(user(VIEWER)),
    )
    .await
    .expect_err("a view grant does not write");

    assert!(
        error.description.contains("edit access"),
        "{}",
        error.description
    );
    assert!(world.lock().unwrap().applied.is_empty());
}

#[tokio::test]
async fn the_read_only_tool_never_writes_even_for_an_owner() {
    let world = world();
    let error = ReadOnlyQueryDatabase {
        sql: "DELETE FROM \"Guests\" WHERE \"Name\" = 'Maria'".into(),
    }
    .call(
        ServiceContext(DatabasesSqlToolContext::new(sql(&world))),
        RequestContext::new(user(OWNER)),
    )
    .await
    .expect_err("document answers never write");

    assert!(
        error.description.contains("read-only"),
        "{}",
        error.description
    );
    assert!(world.lock().unwrap().applied.is_empty());

    let response = ReadOnlyQueryDatabase {
        sql: "SELECT COUNT(*) AS guests FROM \"Offsite\".\"Guests\"".into(),
    }
    .call(
        ServiceContext(DatabasesSqlToolContext::new(sql(&world))),
        RequestContext::new(user(OWNER)),
    )
    .await
    .expect("document answers read");
    assert_eq!(
        response.results[0].rows,
        vec![vec![Some(database_sql::fold::Cell::Number(1.0))]]
    );
}

#[test]
fn query_response_schema_accepts_omitted_empty_metadata() {
    let schema = serde_json::to_value(schemars::schema_for!(QueryDatabaseResponse)).unwrap();
    let required = schema["required"].as_array().unwrap();
    for omitted in ["insertedRowIds", "newVersions", "truncatedTables"] {
        assert!(
            !required.contains(&serde_json::json!(omitted)),
            "{omitted} is omitted by serialization and must be optional in the frontend schema"
        );
    }
    assert!(required.contains(&serde_json::json!("results")));
}

#[test]
fn query_display_is_optional_and_only_accepts_supported_views() {
    let query: QueryDatabase =
        serde_json::from_value(serde_json::json!({"sql": "SELECT 1"})).unwrap();
    assert!(query.display.is_none());
    for (display, expected) in [
        ("bar", QueryDatabaseDisplay::Bar),
        ("area", QueryDatabaseDisplay::Area),
        ("scatter", QueryDatabaseDisplay::Scatter),
    ] {
        let chart: QueryDatabase =
            serde_json::from_value(serde_json::json!({"sql": "SELECT 1", "display": display}))
                .unwrap();
        assert_eq!(chart.display, Some(expected));
    }
    assert!(
        serde_json::from_value::<QueryDatabase>(
            serde_json::json!({"sql": "SELECT 1", "display": "unsupported"})
        )
        .is_err()
    );
}

/// A refused write and a table that moved mid-write are not naming
/// mistakes, so the model is not sent to look names up again.
#[tokio::test]
async fn a_refused_write_says_what_was_refused_without_name_advice() {
    let world = world();
    world.lock().unwrap().op_answers.extend([
        Err(databases::domain::models::DatabaseError::InvalidOp(
            databases::domain::models::OpRefusal {
                op: 0,
                row: None,
                column: Some(STATUS_COLUMN),
                reason: "\"Status\" holds one value; 2 were given".into(),
            },
        )),
        Err(databases::domain::models::DatabaseError::VersionConflict),
    ]);
    let update = || QueryDatabase {
        sql: "UPDATE \"Guests\" SET \"Status\" = 'Going' WHERE \"Name\" = 'Maria'".into(),
        database_id: Some(OFFSITE),
        base_versions: None,
        display: None,
    };

    let refused = update()
        .call(
            ServiceContext(DatabasesSqlToolContext::new(sql(&world))),
            RequestContext::new(user(OWNER)),
        )
        .await
        .expect_err("the service refuses the write");
    assert_eq!(
        refused.description,
        "The write was refused, so nothing changed: \"Status\" holds one value; 2 were given. \
         Fix the statement and retry."
    );

    let conflicted = update()
        .call(
            ServiceContext(DatabasesSqlToolContext::new(sql(&world))),
            RequestContext::new(user(OWNER)),
        )
        .await
        .expect_err("the table moved");
    assert_eq!(
        conflicted.description,
        format!("Table {GUESTS} changed underneath this statement. Re-read it and retry.")
    );
}

/// Each kind of statement names what it wrote, so the chat need not read
/// the SQL to know.
#[tokio::test]
async fn every_statement_kind_names_what_it_wrote() {
    let world = world();
    world.lock().unwrap().op_answers.extend([
        Ok(vec![OpResult::RowsWritten {
            table_version: TableVersion(2),
            inserted: vec![Uuid::from_u128(0xe002)],
            affected: 1,
        }]),
        Ok(vec![OpResult::RowsWritten {
            table_version: TableVersion(3),
            inserted: vec![],
            affected: 1,
        }]),
        Ok(vec![OpResult::RowsWritten {
            table_version: TableVersion(4),
            inserted: vec![],
            affected: 1,
        }]),
        Ok(vec![OpResult::ColumnTyped {
            table_version: TableVersion(5),
            cleared_cells: 0,
            trimmed_cells: 2,
        }]),
    ]);
    let statement = async |statement: &str| {
        let response = QueryDatabase {
            sql: statement.into(),
            database_id: Some(OFFSITE),
            base_versions: None,
            display: None,
        }
        .call(
            ServiceContext(DatabasesSqlToolContext::new(sql(&world))),
            RequestContext::new(user(OWNER)),
        )
        .await
        .expect("the owner runs it");
        (
            serde_json::to_value(&response.statement).unwrap(),
            response.summary,
        )
    };

    assert_eq!(
        statement("SELECT \"Name\" FROM \"Guests\"").await,
        (
            serde_json::json!({"kind": "select"}),
            "Returned 1 row.".into()
        )
    );
    assert_eq!(
        statement("INSERT INTO \"Guests\" (\"Name\") VALUES ('Sam')").await,
        (
            serde_json::json!({"kind": "insert", "tableId": GUESTS, "tableName": "Guests"}),
            "Applied 1 row change.".into()
        )
    );
    assert_eq!(
        statement("UPDATE \"Guests\" SET \"Status\" = 'Going' WHERE \"Name\" = 'Maria'").await,
        (
            serde_json::json!({"kind": "update", "tableId": GUESTS, "tableName": "Guests"}),
            "Applied 1 row change.".into()
        )
    );
    assert_eq!(
        statement("DELETE FROM \"Guests\" WHERE \"Name\" = 'Maria'").await,
        (
            serde_json::json!({"kind": "delete", "tableId": GUESTS, "tableName": "Guests"}),
            "Applied 1 row change.".into()
        )
    );
    assert_eq!(
        statement("ALTER TABLE \"Guests\" ALTER COLUMN \"Status\" TYPE select[]").await,
        (
            serde_json::json!({
                "kind": "alterColumnType",
                "tableId": GUESTS,
                "tableName": "Guests",
                "columnId": STATUS_COLUMN,
                "columnName": "Status",
                "to": "select[]",
                "clearedCells": 0,
                "trimmedCells": 2,
            }),
            "Changed \"Status\" to select[]. Kept only the first value of 2 cells.".into()
        )
    );
}
