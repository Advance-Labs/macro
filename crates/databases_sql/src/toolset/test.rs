//! The SQL tools over the adapter, over fakes of the services it runs on.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ai_toolset::schema::generate_validated_input_schema;
use ai_toolset::{AsyncTool, RequestContext, ServiceContext};
use entity_access::domain::models::AccessLevel;
use models_databases::{CellValue, CellWrite, DatabaseOp, OptionRef, RowChanges};
use models_properties::service::property_value::PropertyValue;
use models_properties::shared::DataType;
use uuid::Uuid;

use super::*;
use crate::test_support::{
    AppliedOps, FakeAccess, FakeContacts, FakeDatabases, FakeSoup, OWNER, Shared, VIEWER, World,
    column, database, row, select_column, sql, table, user,
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

type Context = DatabasesSqlToolContext<FakeDatabases, FakeAccess, FakeSoup, FakeContacts>;

/// `Offsite.Guests` with one guest, the owner's; the viewer can read it.
fn world() -> Shared {
    Arc::new(Mutex::new(World {
        databases: vec![database(
            OFFSITE,
            "Offsite",
            vec![table(
                GUESTS,
                OFFSITE,
                "Guests",
                vec![
                    column(
                        Uuid::from_u128(0xb001),
                        NAME,
                        "Name",
                        DataType::String,
                        false,
                    ),
                    select_column(
                        STATUS_COLUMN,
                        STATUS,
                        "Status",
                        &[(GOING, "Going"), (MAYBE, "Maybe")],
                    ),
                ],
            )],
        )],
        grants: vec![
            (OWNER, OFFSITE, AccessLevel::Owner),
            (VIEWER, OFFSITE, AccessLevel::View),
        ],
        rows: vec![row(
            MARIA,
            GUESTS,
            OFFSITE,
            1,
            vec![
                (NAME, PropertyValue::Str("Maria".into())),
                (STATUS, PropertyValue::SelectOption(vec![MAYBE])),
            ],
        )],
        ..World::default()
    }))
}

fn context(world: &Shared) -> Context {
    DatabasesSqlToolContext::new(sql(world))
}

fn as_user(id: &'static str) -> RequestContext {
    RequestContext::new(user(id))
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
async fn a_read_answers_with_row_ids_labels_and_the_versions_it_read() {
    let world = world();
    let response = QueryDatabase {
        sql: "SELECT \"Name\", \"Status\" FROM \"Guests\"".into(),
        database_id: Some(OFFSITE),
        base_versions: None,
        display: None,
    }
    .call(ServiceContext(context(&world)), as_user(VIEWER))
    .await
    .expect("the viewer reads");

    assert_eq!(
        serde_json::to_value(&response).unwrap(),
        serde_json::json!({
            "results": [{
                "columns": [{"name": "row_id"}, {"name": "Name"}, {"name": "Status"}],
                "rows": [[MARIA.to_string(), "Maria", "Maybe"]],
            }],
            "changesApplied": 0,
            "readVersions": [{"tableId": GUESTS, "version": 1}],
            "summary": "Returned 1 row.",
        })
    );
}

#[tokio::test]
async fn a_write_runs_as_the_agent_for_the_user_and_guards_its_base_versions() {
    let world = world();
    let request: QueryDatabase = serde_json::from_value(serde_json::json!({
        "sql": "UPDATE \"Guests\" SET \"Status\" = 'Going' WHERE \"Name\" = 'Maria'",
        "databaseId": OFFSITE,
        "baseVersions": [{"tableId": GUESTS, "version": 1}],
    }))
    .unwrap();
    let response = request
        .call(
            ServiceContext(context(&world).with_actor(bot_id::MACRO_AI_BOT_ID)),
            as_user(OWNER),
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
        .call(ServiceContext(context(&world)), as_user(OWNER))
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
    .call(ServiceContext(context(&world)), as_user(OWNER))
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
    .call(ServiceContext(context(&world)), as_user(VIEWER))
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
    .call(ServiceContext(context(&world)), as_user(OWNER))
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
    .call(ServiceContext(context(&world)), as_user(OWNER))
    .await
    .expect("document answers read");
    assert_eq!(response.results[0].rows, vec![vec![serde_json::json!(1.0)]]);
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
