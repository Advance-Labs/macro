use std::sync::{Arc, Mutex};

use databases::domain::models::{QueryDefinition, TableVersion};
use entity_access::domain::models::AccessLevel;
use item_filters::ast::database_row::DatabaseRowLiteral;
use models_databases::{CellValue, CellWrite, DatabaseOp, OptionRef, RowChanges};
use models_properties::service::property_value::PropertyValue;
use models_properties::shared::{DataType, EntityReference, EntityType as PropertyEntityType};
use uuid::Uuid;

use super::*;
use crate::outcome::ResultColumn;
use crate::test_support::{
    AppliedOps, OWNER, STRANGER, Shared, VIEWER, World, agent_for, column, database,
    relation_column, row, row_literals, select_column, sql, table,
};

const OFFSITE: Uuid = Uuid::from_u128(0xdb01);
const GUESTS: Uuid = Uuid::from_u128(0x7a01);
const VENUES: Uuid = Uuid::from_u128(0xdb02);
const HALLS: Uuid = Uuid::from_u128(0x7a02);
const SECRET: Uuid = Uuid::from_u128(0xdb03);
const PLANS: Uuid = Uuid::from_u128(0x7a03);

const NAME: Uuid = Uuid::from_u128(0xc001);
const STATUS: Uuid = Uuid::from_u128(0xc002);
const HALL: Uuid = Uuid::from_u128(0xc003);
const CONTACT: Uuid = Uuid::from_u128(0xc004);
const HALL_NAME: Uuid = Uuid::from_u128(0xc005);
const PLAN_NAME: Uuid = Uuid::from_u128(0xc006);

const NAME_COLUMN: Uuid = Uuid::from_u128(0xb001);
const STATUS_COLUMN: Uuid = Uuid::from_u128(0xb002);
const HALL_COLUMN: Uuid = Uuid::from_u128(0xb003);
const CONTACT_COLUMN: Uuid = Uuid::from_u128(0xb004);
const HALL_NAME_COLUMN: Uuid = Uuid::from_u128(0xb005);
const PLAN_NAME_COLUMN: Uuid = Uuid::from_u128(0xb006);

const GOING: Uuid = Uuid::from_u128(0xa001);
const MAYBE: Uuid = Uuid::from_u128(0xa002);

const MARIA: Uuid = Uuid::from_u128(0xe001);
const SAM: Uuid = Uuid::from_u128(0xe002);
const BALLROOM: Uuid = Uuid::from_u128(0xe003);
const LAUNCH: Uuid = Uuid::from_u128(0xe004);

/// `Offsite.Guests` (relating to `Venues.Halls`) and `Venues` are the
/// owner's; the viewer can read `Offsite`; `Secret.Plans` is the
/// stranger's alone.
fn world() -> Shared {
    let reference = |id: &str| {
        PropertyValue::EntityRef(vec![EntityReference {
            entity_id: id.to_string(),
            entity_type: PropertyEntityType::User,
            specific_message_id: None,
        }])
    };
    Arc::new(Mutex::new(World {
        databases: vec![
            database(
                OFFSITE,
                "Offsite",
                vec![table(
                    GUESTS,
                    OFFSITE,
                    "Guests",
                    vec![
                        column(NAME_COLUMN, NAME, "Name", DataType::String, false),
                        select_column(
                            STATUS_COLUMN,
                            STATUS,
                            "Status",
                            &[(GOING, "Going"), (MAYBE, "Maybe")],
                        ),
                        relation_column(HALL_COLUMN, HALL, "Hall", VENUES, HALLS),
                        column(CONTACT_COLUMN, CONTACT, "Contact", DataType::Entity, false),
                    ],
                )],
            ),
            database(
                VENUES,
                "Venues",
                vec![table(
                    HALLS,
                    VENUES,
                    "Halls",
                    vec![column(
                        HALL_NAME_COLUMN,
                        HALL_NAME,
                        "Name",
                        DataType::String,
                        false,
                    )],
                )],
            ),
            database(
                SECRET,
                "Secret",
                vec![table(
                    PLANS,
                    SECRET,
                    "Plans",
                    vec![column(
                        PLAN_NAME_COLUMN,
                        PLAN_NAME,
                        "Name",
                        DataType::String,
                        false,
                    )],
                )],
            ),
        ],
        grants: vec![
            (OWNER, OFFSITE, AccessLevel::Owner),
            (OWNER, VENUES, AccessLevel::Owner),
            (VIEWER, OFFSITE, AccessLevel::View),
            (STRANGER, SECRET, AccessLevel::Owner),
        ],
        rows: vec![
            row(
                MARIA,
                GUESTS,
                OFFSITE,
                1,
                vec![
                    (NAME, PropertyValue::Str("Maria".into())),
                    (STATUS, PropertyValue::SelectOption(vec![GOING])),
                    (
                        HALL,
                        PropertyValue::EntityRef(vec![EntityReference {
                            entity_id: BALLROOM.to_string(),
                            entity_type: PropertyEntityType::DatabaseRow,
                            specific_message_id: None,
                        }]),
                    ),
                    (CONTACT, reference("macro|maria@macro.com")),
                ],
            ),
            row(
                SAM,
                GUESTS,
                OFFSITE,
                2,
                vec![
                    (NAME, PropertyValue::Str("Sam".into())),
                    (STATUS, PropertyValue::SelectOption(vec![MAYBE])),
                ],
            ),
            row(
                BALLROOM,
                HALLS,
                VENUES,
                3,
                vec![(HALL_NAME, PropertyValue::Str("Ballroom".into()))],
            ),
            row(
                LAUNCH,
                PLANS,
                SECRET,
                4,
                vec![(PLAN_NAME, PropertyValue::Str("Launch".into()))],
            ),
        ],
        contacts: vec!["macro|maria@macro.com", "macro|sam@macro.com"],
        ..World::default()
    }))
}

fn read(sql: &str) -> SqlRequest {
    SqlRequest {
        sql: sql.to_string(),
        scope: None,
        base_versions: HashMap::new(),
    }
}

fn result(outcome: &SqlOutcome) -> &crate::outcome::ResultSet {
    outcome.result.as_ref().expect("a SELECT has a result")
}

// ---- permissions -------------------------------------------------------------

#[tokio::test]
async fn a_strangers_database_is_not_in_the_catalog() {
    let world = world();
    let error = sql(&world)
        .execute(
            agent_for(OWNER),
            read("SELECT \"Name\" FROM \"Secret\".\"Plans\""),
        )
        .await
        .expect_err("the owner holds no grant on Secret");

    let SqlError::Sql(message) = error else {
        panic!("a missing table is a SQL error: {error:?}");
    };
    assert!(message.contains("unknown table"), "{message}");
    assert!(world.lock().unwrap().soup_reads.is_empty());

    let outcome = sql(&world)
        .execute(
            agent_for(STRANGER),
            read("SELECT \"Name\" FROM \"Secret\".\"Plans\""),
        )
        .await
        .expect("the stranger reads their own table");
    assert_eq!(
        result(&outcome).rows,
        vec![vec![
            serde_json::json!(LAUNCH.to_string()),
            serde_json::json!("Launch"),
        ]]
    );
}

#[tokio::test]
async fn a_view_only_database_cannot_be_written() {
    let world = world();
    let error = sql(&world)
        .execute(
            agent_for(VIEWER),
            read("UPDATE \"Offsite\".\"Guests\" SET \"Status\" = 'Going' WHERE \"Name\" = 'Sam'"),
        )
        .await
        .expect_err("a view grant does not write");

    let SqlError::ReadOnly(message) = error else {
        panic!("a view grant reads as read-only: {error:?}");
    };
    assert_eq!(message, "table Guests is read-only");
    let world = world.lock().unwrap();
    assert!(world.applied.is_empty());
    assert!(world.soup_reads.is_empty());
}

#[tokio::test]
async fn a_write_is_applied_under_an_edit_receipt_for_its_database() {
    let world = world();
    let outcome = sql(&world)
        .execute(
            agent_for(OWNER),
            read("UPDATE \"Offsite\".\"Guests\" SET \"Status\" = 'Going' WHERE \"Name\" = 'Sam'"),
        )
        .await
        .expect("the owner writes");

    assert_eq!(outcome.changes_applied, 1);
    assert_eq!(
        outcome.new_versions,
        HashMap::from([(GUESTS, TableVersion(2))])
    );
    assert_eq!(
        world.lock().unwrap().applied,
        vec![AppliedOps {
            database: OFFSITE,
            level: AccessLevel::Owner,
            acting_bot: Some(bot_id::MACRO_AI_BOT_ID),
            ops: vec![DatabaseOp::UpdateRows {
                table: GUESTS,
                changes: RowChanges::Uniform {
                    rows: vec![SAM],
                    cells: vec![CellWrite {
                        column: STATUS_COLUMN,
                        value: CellValue::Options(vec![OptionRef::Label("Going".into())]),
                    }],
                },
                create_missing_options: false,
            }],
        }]
    );
}

#[tokio::test]
async fn a_cross_database_join_only_sees_databases_the_viewer_can_reach() {
    let join = "SELECT g.\"Name\", h.\"Name\" AS hall FROM \"Offsite\".\"Guests\" g \
                JOIN \"Venues\".\"Halls\" h ON g.\"Hall\" = h.row_id";

    let world = world();
    let outcome = sql(&world)
        .execute(agent_for(OWNER), read(join))
        .await
        .expect("the owner reaches both databases");
    assert_eq!(
        result(&outcome).rows,
        vec![vec![
            serde_json::json!(MARIA.to_string()),
            serde_json::json!("Maria"),
            serde_json::json!("Ballroom"),
        ]]
    );
    let reads: Vec<Vec<DatabaseRowLiteral>> = world
        .lock()
        .unwrap()
        .soup_reads
        .iter()
        .map(row_literals)
        .collect();
    assert_eq!(
        reads,
        vec![
            vec![DatabaseRowLiteral::TableId(GUESTS)],
            vec![
                DatabaseRowLiteral::TableId(HALLS),
                DatabaseRowLiteral::Id(BALLROOM)
            ],
        ]
    );

    let world = self::world();
    let error = sql(&world)
        .execute(agent_for(VIEWER), read(join))
        .await
        .expect_err("the viewer holds no grant on Venues");
    assert!(
        matches!(&error, SqlError::Sql(message) if message.contains("unknown table")),
        "{error:?}"
    );
    assert!(world.lock().unwrap().soup_reads.is_empty());

    let error = sql(&world)
        .execute(
            agent_for(OWNER),
            read(
                "SELECT g.\"Name\" FROM \"Offsite\".\"Guests\" g \
                 JOIN \"Secret\".\"Plans\" p ON g.\"Name\" = p.\"Name\"",
            ),
        )
        .await
        .expect_err("the owner holds no grant on Secret");
    assert!(
        matches!(&error, SqlError::Sql(message) if message.contains("unknown table")),
        "{error:?}"
    );
}

// ---- reads -------------------------------------------------------------------

#[tokio::test]
async fn a_read_answers_row_ids_first_with_labels_and_entity_types() {
    let world = world();
    let outcome = sql(&world)
        .execute(
            agent_for(VIEWER),
            read("SELECT \"Name\", \"Status\", \"Contact\" FROM \"Offsite\".\"Guests\" WHERE \"Status\" = 'Going'"),
        )
        .await
        .expect("the viewer reads");

    assert_eq!(
        result(&outcome).columns,
        vec![
            ResultColumn {
                name: "row_id".into(),
                entity_type: None
            },
            ResultColumn {
                name: "Name".into(),
                entity_type: None
            },
            ResultColumn {
                name: "Status".into(),
                entity_type: None
            },
            ResultColumn {
                name: "Contact".into(),
                entity_type: Some(model_entity::EntityType::User)
            },
        ]
    );
    assert_eq!(
        result(&outcome).rows,
        vec![vec![
            serde_json::json!(MARIA.to_string()),
            serde_json::json!("Maria"),
            serde_json::json!("Going"),
            serde_json::json!("macro|maria@macro.com"),
        ]]
    );
    assert_eq!(
        outcome.read_versions,
        HashMap::from([(GUESTS, TableVersion(1))])
    );
    // The select filter went to Soup as a property filter on the table.
    let world = world.lock().unwrap();
    assert_eq!(world.soup_reads.len(), 1);
    assert!(world.soup_reads[0].properties_filter.is_some());
}

#[tokio::test]
async fn a_count_per_option_is_read_as_soup_bins() {
    let world = world();
    let outcome = sql(&world)
        .execute(
            agent_for(VIEWER),
            read("SELECT \"Status\", COUNT(*) AS guests FROM \"Offsite\".\"Guests\" GROUP BY \"Status\""),
        )
        .await
        .expect("the viewer counts");

    let mut rows = result(&outcome).rows.clone();
    rows.sort_by_key(|row| row[0].to_string());
    assert_eq!(
        rows,
        vec![
            vec![serde_json::json!("Going"), serde_json::json!(1.0)],
            vec![serde_json::json!("Maybe"), serde_json::json!(1.0)],
        ]
    );
}

#[tokio::test]
async fn people_are_the_viewers_contacts() {
    let world = world();
    let outcome = sql(&world)
        .execute(
            agent_for(VIEWER),
            read(
                "SELECT g.\"Name\", p.email FROM \"Offsite\".\"Guests\" g \
                 JOIN macro.people p ON g.\"Contact\" = p.id",
            ),
        )
        .await
        .expect("people join like any table");

    assert_eq!(
        result(&outcome).rows,
        vec![vec![
            serde_json::json!(MARIA.to_string()),
            serde_json::json!("Maria"),
            serde_json::json!("maria@macro.com"),
        ]]
    );
}

#[tokio::test]
async fn a_read_only_query_refuses_a_write_before_reading() {
    let world = world();
    let error = sql(&world)
        .query(
            agent_for(OWNER),
            "DELETE FROM \"Offsite\".\"Guests\" WHERE \"Name\" = 'Sam'".into(),
        )
        .await
        .expect_err("queries never write");

    assert!(matches!(error, SqlError::ReadOnly(_)), "{error:?}");
    let world = world.lock().unwrap();
    assert!(world.soup_reads.is_empty());
    assert!(world.applied.is_empty());
}

#[tokio::test]
async fn a_stale_base_version_refuses_the_write() {
    let world = world();
    let error = sql(&world)
        .execute(
            agent_for(OWNER),
            SqlRequest {
                sql: "DELETE FROM \"Offsite\".\"Guests\" WHERE \"Name\" = 'Sam'".into(),
                scope: Some(OFFSITE),
                base_versions: HashMap::from([(GUESTS, TableVersion(0))]),
            },
        )
        .await
        .expect_err("the table moved past version 0");

    assert!(
        matches!(error, SqlError::VersionConflict { table_id } if table_id == GUESTS),
        "{error:?}"
    );
    assert!(world.lock().unwrap().applied.is_empty());
}

#[tokio::test]
async fn an_alter_column_reports_the_column_it_changed() {
    let world = world();
    let outcome = sql(&world)
        .execute(
            agent_for(OWNER),
            read("ALTER TABLE \"Offsite\".\"Guests\" ALTER COLUMN \"Status\" TYPE text"),
        )
        .await
        .expect("the owner retypes");

    assert_eq!(
        outcome.altered_column,
        Some(crate::outcome::AlteredColumn {
            table_id: GUESTS,
            column_id: STATUS_COLUMN,
            name: "Status".into(),
            to: "text".into(),
            cleared_cells: 1,
            trimmed_cells: 0,
        })
    );
}

// ---- saved questions ---------------------------------------------------------

#[tokio::test]
async fn a_question_is_saved_once_it_compiles_as_a_select_in_its_database() {
    let world = world();
    let definition = QueryDefinition::V1 {
        query: "SELECT COUNT(*) FROM \"Guests\"".into(),
    };
    let saved = sql(&world)
        .save_query(agent_for(VIEWER), Some(OFFSITE), definition.clone())
        .await
        .expect("the viewer saves a read");

    assert_eq!(saved.database_id, Some(OFFSITE));
    assert_eq!(
        world.lock().unwrap().saved,
        vec![(Some(OFFSITE), definition)]
    );
}

#[tokio::test]
async fn a_question_that_writes_or_does_not_compile_is_not_saved() {
    let world = world();
    let writes = sql(&world)
        .save_query(
            agent_for(OWNER),
            Some(OFFSITE),
            QueryDefinition::V1 {
                query: "DELETE FROM \"Guests\" WHERE \"Name\" = 'Sam'".into(),
            },
        )
        .await
        .expect_err("a saved question never writes");
    assert!(matches!(writes, SqlError::ReadOnly(_)), "{writes:?}");

    let broken = sql(&world)
        .save_query(
            agent_for(OWNER),
            Some(OFFSITE),
            QueryDefinition::V1 {
                query: "SELECT statuz FROM \"Guests\"".into(),
            },
        )
        .await
        .expect_err("a broken question is not saved");
    assert!(matches!(broken, SqlError::Sql(_)), "{broken:?}");

    let hidden = sql(&world)
        .save_query(
            agent_for(OWNER),
            Some(SECRET),
            QueryDefinition::V1 {
                query: "SELECT COUNT(*) FROM \"Plans\"".into(),
            },
        )
        .await
        .expect_err("the owner cannot see Secret");
    assert!(matches!(hidden, SqlError::NotFound), "{hidden:?}");

    assert!(world.lock().unwrap().saved.is_empty());
}
