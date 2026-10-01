//! Service tests: the domain service over in-memory fakes for every port, so
//! every write path (schema changes, typed ops, lifecycle) runs without
//! Postgres, and every allow/deny decision is asserted at the service
//! boundary.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use entity_access::domain::models::{
    AccessLevel, Entity, EntityAccessReceipt, EntityPermission, EntityType, RequiredPermission,
};
use macro_event_broker::{EventBrokerError, MacroEvent, MacroEventBroker};
use macro_user_id::user_id::MacroUserIdStr;
use models_databases::{CellValue, CellWrite, DatabaseOp, OpResult, OptionRef, RowChanges};
use models_properties::service::property_definition::PropertyDefinition;
use models_properties::service::property_definition_with_options::PropertyDefinitionWithOptions;
use models_properties::service::property_option::{PropertyOption, PropertyOptionValue};
use models_properties::service::property_value::PropertyValue;
use models_properties::shared::{DataType, EntityType as PropertyEntityType, PropertyOwner};
use uuid::Uuid;

use super::*;
use crate::domain::models::{
    CardPosition, Column, ColumnBinding, ColumnConfig, DatabaseView, NewOption, OpRefusal,
    PropertyDefinitionId, RowId, RowRef, TableDeletion, TableOrderOutcome, TableVersion, ViewId,
    Write, Writes, WritesOutcome,
};

mod casts;
mod columns;
mod delete_table;
mod discovery;
mod fakes;
mod infer_column_type;
mod ops;
mod options;
mod relations;
mod rename_column;
mod saved_queries;
mod sharing;
mod tables;
mod views;

use fakes::*;

const OWNER: &str = "macro|owner@macro.com";
const VIEWER: &str = "macro|viewer@macro.com";
const STRANGER: &str = "macro|stranger@macro.com";

fn user(id: &'static str) -> MacroUserIdStr<'static> {
    MacroUserIdStr::parse_from_str(id).expect("valid user id")
}

fn viewer(id: &'static str) -> Viewer {
    Viewer {
        user_id: user(id),
        acting_bot: None,
    }
}

type Service =
    DatabasesServiceImpl<FakeRepo, FakeDefs, FakeCells, FakeEvents, FakeAccess, RecordingBroker>;

fn service(world: &Shared) -> Service {
    DatabasesServiceImpl::new(
        FakeRepo(world.clone()),
        FakeDefs(world.clone()),
        FakeCells(world.clone()),
        FakeEvents(world.clone()),
        FakeAccess(world.clone()),
        RecordingBroker(world.clone()),
    )
}

fn definition(
    name: &str,
    data_type: DataType,
    multi: bool,
    owner: PropertyOwner,
) -> PropertyDefinitionWithOptions {
    PropertyDefinitionWithOptions {
        definition: PropertyDefinition {
            id: Uuid::new_v4(),
            owner,
            display_name: name.into(),
            data_type,
            is_multi_select: multi,
            specific_entity_type: (data_type == DataType::Entity)
                .then_some(PropertyEntityType::User),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            is_system: false,
            is_metadata: false,
        },
        property_options: vec![],
    }
}

fn receipt<T: RequiredPermission>(
    database_id: DatabaseId,
    user: &'static str,
    level: AccessLevel,
) -> EntityAccessReceipt<T> {
    EntityAccessReceipt::try_new_authenticated_user(
        self::user(user),
        Entity {
            entity_id: database_id.to_string(),
            entity_type: EntityType::Database,
        },
        EntityPermission::AccessLevel {
            access_level: level,
        },
    )
    .expect("level satisfies the requirement")
}

fn edit(database_id: DatabaseId) -> EntityAccessReceipt<EditAccessLevel> {
    receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner)
}

fn table_version(world: &Shared, table_id: TableId) -> TableVersion {
    world
        .lock()
        .unwrap()
        .tables
        .iter()
        .find(|table| table.id == table_id)
        .unwrap()
        .version
}

fn option_id(world: &Shared, definition_id: PropertyDefinitionId, label: &str) -> Uuid {
    world.lock().unwrap().definitions[&definition_id]
        .property_options
        .iter()
        .find(|option| option.value == PropertyOptionValue::String(label.into()))
        .unwrap()
        .id
}

fn row_ids(world: &Shared, table_id: TableId) -> Vec<RowId> {
    world.lock().unwrap().rows[&table_id]
        .iter()
        .map(|row| row.id)
        .collect()
}

fn cell(world: &Shared, row: RowId, definition_id: PropertyDefinitionId) -> Option<PropertyValue> {
    world
        .lock()
        .unwrap()
        .cells
        .get(&row)
        .and_then(|cells| cells.get(&definition_id))
        .cloned()
}

/// Insert one row per name into the seeded Guests table, answering their ids.
async fn insert_names(seeded: &Seeded, names: &[&str]) -> Vec<RowId> {
    let results = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![DatabaseOp::InsertRows {
                table: seeded.table_id,
                rows: names
                    .iter()
                    .map(|name| {
                        vec![CellWrite {
                            column: seeded.name_column.id,
                            value: CellValue::Text((*name).into()),
                        }]
                    })
                    .collect(),
                create_missing_options: false,
            }],
        )
        .await
        .unwrap();
    let [OpResult::RowsWritten { inserted, .. }] = results.as_slice() else {
        panic!("expected one insert, got {results:?}");
    };
    inserted.clone()
}

/// The seeded world: database `Offsite` with one table
/// `Guests(Name TEXT, Status SELECT[Going|Declined], Plus ones NUMBER)`
/// holding one row (`Sam`, `Going`, `2`), owned by OWNER and shared
/// View-only with VIEWER.
struct Seeded {
    world: Shared,
    service: Service,
    database_id: DatabaseId,
    table_id: TableId,
    /// Sam's row.
    row_id: RowId,
    name_column: Column,
    status_column: Column,
    plus_ones_column: Column,
}

async fn seeded() -> Seeded {
    let world: Shared = Arc::default();
    let service = self::service(&world);
    let database = service
        .create_database(CreateDatabase {
            name: "Offsite".into(),
            owner_id: user(OWNER),
            acting_bot: None,
        })
        .await
        .unwrap();
    let table_id = {
        let mut w = world.lock().unwrap();
        w.tables[0].name = "Guests".into();
        w.tables[0].id
    };
    let name_column = service
        .create_column(
            receipt::<EditAccessLevel>(database.id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "Name".into(),
                    data_type: DataType::String,
                    is_multi_select: false,
                    options: vec![],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    let status_column = service
        .create_column(
            receipt::<EditAccessLevel>(database.id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "Status".into(),
                    data_type: DataType::SelectString,
                    is_multi_select: false,
                    options: vec!["Going".into(), "Declined".into()],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    let plus_ones_column = service
        .create_column(
            receipt::<EditAccessLevel>(database.id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "Plus ones".into(),
                    data_type: DataType::Number,
                    is_multi_select: false,
                    options: vec![],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    world
        .lock()
        .unwrap()
        .grants
        .entry(VIEWER.into())
        .or_default()
        .push((database.id, AccessLevel::View));
    let inserted = service
        .apply_ops(
            receipt::<EditAccessLevel>(database.id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            vec![DatabaseOp::InsertRows {
                table: table_id,
                rows: vec![vec![
                    CellWrite {
                        column: name_column,
                        value: CellValue::Text("Sam".into()),
                    },
                    CellWrite {
                        column: status_column,
                        value: CellValue::Options(vec![OptionRef::Label("Going".into())]),
                    },
                    CellWrite {
                        column: plus_ones_column,
                        value: CellValue::Number(2.0),
                    },
                ]],
                create_missing_options: false,
            }],
        )
        .await
        .unwrap();
    let [OpResult::RowsWritten { inserted, .. }] = inserted.as_slice() else {
        panic!("expected one insert, got {inserted:?}");
    };
    // Tests count the batches their own writes make, not the seed row's.
    world.lock().unwrap().write_batches = 0;
    let column = |id: ColumnId| {
        world
            .lock()
            .unwrap()
            .columns
            .iter()
            .find(|column| column.id == id)
            .unwrap()
            .clone()
    };
    Seeded {
        database_id: database.id,
        table_id,
        row_id: inserted[0],
        name_column: column(name_column),
        status_column: column(status_column),
        plus_ones_column: column(plus_ones_column),
        world,
        service,
    }
}

// ===== Databases =====

#[tokio::test]
async fn create_database_grants_owner_and_starter_table() {
    let world: Shared = Arc::default();
    let svc = service(&world);
    let db = svc
        .create_database(CreateDatabase {
            name: "  Offsite ".into(),
            owner_id: user(OWNER),
            acting_bot: None,
        })
        .await
        .unwrap();
    assert_eq!(db.name, "Offsite");
    let listed = svc.list_databases(viewer(OWNER)).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].grant, AccessLevel::Owner);
    assert_eq!(listed[0].tables.len(), 1);
    assert_eq!(listed[0].tables[0].name, "Table 1");
    assert!(
        svc.list_databases(viewer(STRANGER))
            .await
            .unwrap()
            .is_empty()
    );
    {
        let w = world.lock().unwrap();
        assert_eq!(w.tables.len(), 1);
        assert_eq!(w.tables[0].name, "Table 1");
        assert_eq!(w.tables[0].database_id, db.id);
    }

    let err = svc
        .create_database(CreateDatabase {
            name: "   ".into(),
            owner_id: user(OWNER),
            acting_bot: None,
        })
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::InvalidSchemaOperation(_)));
}

#[tokio::test]
async fn database_details_answer_every_live_database_the_viewer_holds_a_grant_on() {
    let seeded = seeded().await;
    let (svc, offsite, guests) = (seeded.service, seeded.database_id, seeded.table_id);
    let sessions = svc
        .create_table(
            receipt::<EditAccessLevel>(offsite, OWNER, AccessLevel::Owner),
            CreateTable {
                database_id: offsite,
                name: "Sessions".into(),
            },
        )
        .await
        .unwrap();
    svc.reorder_tables(
        receipt::<EditAccessLevel>(offsite, OWNER, AccessLevel::Owner),
        vec![sessions.id, guests],
    )
    .await
    .unwrap();
    let venue = svc
        .create_database(CreateDatabase {
            name: "Venue".into(),
            owner_id: user(OWNER),
            acting_bot: None,
        })
        .await
        .unwrap();
    let archive = svc
        .create_database(CreateDatabase {
            name: "Archive".into(),
            owner_id: user(OWNER),
            acting_bot: None,
        })
        .await
        .unwrap();
    svc.trash_database(receipt::<OwnerAccessLevel>(
        archive.id,
        OWNER,
        AccessLevel::Owner,
    ))
    .await
    .unwrap();

    let details = svc.database_details(viewer(OWNER)).await.unwrap();
    assert_eq!(
        details
            .iter()
            .map(|detail| (detail.database.name.as_str(), detail.grant))
            .collect::<Vec<_>>(),
        vec![
            ("Offsite", AccessLevel::Owner),
            ("Venue", AccessLevel::Owner)
        ],
        "the trashed Archive is left out"
    );
    assert_eq!(
        details[0]
            .tables
            .iter()
            .map(|table| table.table.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Sessions", "Guests"]
    );
    assert!(details[0].tables[0].columns.is_empty());
    assert_eq!(
        details[0].tables[1]
            .columns
            .iter()
            .map(|column| column.definition.definition.display_name.as_str())
            .collect::<Vec<_>>(),
        vec!["Name", "Status", "Plus ones"]
    );
    assert_eq!(details[1].database.id, venue.id);
    assert_eq!(details[1].tables[0].table.name, "Table 1");

    let shared = svc.database_details(viewer(VIEWER)).await.unwrap();
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].database.id, offsite);
    assert_eq!(shared[0].grant, AccessLevel::View);
    assert_eq!(
        shared[0]
            .tables
            .iter()
            .map(|table| table.table.id)
            .collect::<Vec<_>>(),
        vec![sessions.id, guests]
    );

    assert!(
        svc.database_details(viewer(STRANGER))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn rename_validates_the_name_and_writes_it() {
    let seeded = seeded().await;
    let (world, svc, db) = (seeded.world, seeded.service, seeded.database_id);

    let renamed = svc
        .rename_database(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            "  Winter Offsite  ".into(),
        )
        .await
        .unwrap();
    assert_eq!(renamed.name, "Winter Offsite");
    assert_eq!(world.lock().unwrap().databases[0].name, "Winter Offsite");

    let err = svc
        .rename_database(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            "   ".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::InvalidSchemaOperation(_)));

    let err = svc
        .rename_database(
            receipt::<EditAccessLevel>(Uuid::new_v4(), OWNER, AccessLevel::Owner),
            "Elsewhere".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));
}

#[tokio::test]
async fn table_rename_moves_the_sql_name_and_retries_without_overwriting_a_new_name() {
    let seeded = seeded().await;
    let (world, svc, db, table_id) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
    );
    let renamed = svc
        .rename_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Edit),
            table_id,
            "  Attendees  ".into(),
            "Guests".into(),
        )
        .await
        .unwrap();
    assert_eq!(renamed.name, "Attendees");
    assert_eq!(renamed.version, TableVersion(2));
    assert_eq!(
        world.lock().unwrap().published.last(),
        Some(&(table_id, TableVersion(2)))
    );

    let detail = svc
        .get_database(
            receipt::<ViewAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
        )
        .await
        .unwrap();
    assert_eq!(detail.tables[0].sql_name, "\"Offsite\".\"Attendees\"");

    let retried = svc
        .rename_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Edit),
            table_id,
            "Attendees".into(),
            "Guests".into(),
        )
        .await
        .unwrap();
    assert_eq!(retried.version, renamed.version);
    let error = svc
        .rename_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Edit),
            table_id,
            "People".into(),
            "Guests".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DatabaseError::InvalidSchemaOperation(_)));
    assert_eq!(
        world
            .lock()
            .unwrap()
            .tables
            .iter()
            .find(|t| t.id == table_id)
            .unwrap()
            .name,
        "Attendees"
    );
}

#[tokio::test]
async fn table_rename_rejects_invalid_names_foreign_tables_and_trashed_databases() {
    let seeded = seeded().await;
    let (svc, db, table_id) = (seeded.service, seeded.database_id, seeded.table_id);
    svc.create_table(
        receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
        CreateTable {
            database_id: db,
            name: "People".into(),
        },
    )
    .await
    .unwrap();
    for name in [" ", " people "] {
        let error = svc
            .rename_table(
                receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
                table_id,
                name.into(),
                "Guests".into(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, DatabaseError::InvalidSchemaOperation(_)));
    }
    let other = svc
        .create_database(CreateDatabase {
            name: "Elsewhere".into(),
            owner_id: user(OWNER),
            acting_bot: None,
        })
        .await
        .unwrap();
    let error = svc
        .rename_table(
            receipt::<EditAccessLevel>(other.id, OWNER, AccessLevel::Owner),
            table_id,
            "People".into(),
            "Guests".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DatabaseError::NotFound));
    svc.trash_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    let error = svc
        .rename_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            table_id,
            "People".into(),
            "Guests".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DatabaseError::NotFound));
}

#[tokio::test]
async fn trash_hides_the_database_and_restore_brings_it_back() {
    let seeded = seeded().await;
    let (world, svc, db) = (seeded.world, seeded.service, seeded.database_id);

    svc.trash_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    let trashed_at = world.lock().unwrap().databases[0].trashed_at;
    assert!(trashed_at.is_some());

    // A trashed database is invisible to listing, reads, ops, and renames.
    assert!(svc.list_databases(viewer(OWNER)).await.unwrap().is_empty());
    let err = svc
        .get_database(
            receipt::<ViewAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));
    let err = svc
        .apply_ops(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            vec![DatabaseOp::DeleteRows {
                table: seeded.table_id,
                rows: vec![seeded.row_id],
            }],
        )
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound), "{err:?}");
    let err = svc
        .rename_database(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            "Renamed".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));

    // Trashing again keeps the original timestamp.
    svc.trash_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    assert_eq!(world.lock().unwrap().databases[0].trashed_at, trashed_at);

    svc.restore_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    assert!(world.lock().unwrap().databases[0].trashed_at.is_none());
    assert_eq!(svc.list_databases(viewer(OWNER)).await.unwrap().len(), 1);

    // Restoring a live database is a no-op, not an error.
    svc.restore_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
}

#[tokio::test]
async fn permanent_delete_removes_the_database_its_rows_and_its_grants() {
    let seeded = seeded().await;
    let (world, svc, db, row_id) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.row_id,
    );

    svc.delete_database_permanently(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();

    {
        let w = world.lock().unwrap();
        assert!(w.databases.is_empty());
        assert!(w.tables.is_empty());
        assert!(w.rows.is_empty());
        assert!(!w.cells.contains_key(&row_id));
        assert!(w.grants.values().all(|grants| grants.is_empty()));
    }
    assert!(svc.list_databases(viewer(VIEWER)).await.unwrap().is_empty());

    let err = svc
        .delete_database_permanently(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));
}

#[tokio::test]
async fn lifecycle_operations_act_on_trashed_databases() {
    let seeded = seeded().await;
    let (world, svc, db) = (seeded.world, seeded.service, seeded.database_id);
    svc.trash_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();

    svc.delete_database_permanently(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();

    assert!(world.lock().unwrap().databases.is_empty());
}

#[tokio::test]
async fn schema_operations_respect_receipts() {
    let seeded = seeded().await;
    let (svc, db, table_id) = (seeded.service, seeded.database_id, seeded.table_id);
    let other = Uuid::new_v4();
    let err = svc
        .create_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            CreateTable {
                database_id: other,
                name: "Nope".into(),
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::Unauthorized));

    let err = svc
        .create_column(
            receipt::<EditAccessLevel>(other, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "X".into(),
                    data_type: DataType::String,
                    is_multi_select: false,
                    options: vec![],
                },
                config: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, DatabaseError::NotFound),
        "a table outside the receipted database looks missing"
    );

    let detail = svc
        .get_database(
            receipt::<ViewAccessLevel>(db, VIEWER, AccessLevel::View),
            viewer(VIEWER),
        )
        .await
        .unwrap();
    assert_eq!(detail.grant, AccessLevel::View);
    assert_eq!(detail.tables.len(), 1);
    assert_eq!(detail.tables[0].sql_name, "\"Offsite\".\"Guests\"");
    assert_eq!(detail.tables[0].read_sql_name, "\"Offsite\".\"Guests\"");
    assert_eq!(
        detail.tables[0]
            .columns
            .iter()
            .map(|column| column.sql_name.as_str())
            .collect::<Vec<_>>(),
        vec!["\"Name\"", "\"Status\"", "\"Plus ones\""]
    );
    assert!(detail.tables[0].columns.iter().all(|c| !c.writable));

    let detail = svc
        .get_database(
            receipt::<ViewAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
        )
        .await
        .unwrap();
    assert_eq!(detail.grant, AccessLevel::Owner);
    assert!(detail.tables[0].columns.iter().all(|c| c.writable));
}

// ===== Row writes =====

#[tokio::test]
async fn a_write_to_a_row_of_another_table_is_refused() {
    let seeded = seeded().await;
    let (world, svc, database_id, table_id, row_id) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.row_id,
    );
    let sessions = svc
        .create_table(
            receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner),
            CreateTable {
                database_id,
                name: "Sessions".into(),
            },
        )
        .await
        .unwrap();
    let title = svc
        .create_column(
            receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: sessions.id,
                binding: ColumnBinding::NewDefinition {
                    name: "Title".into(),
                    data_type: DataType::String,
                    is_multi_select: false,
                    options: vec![],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    let cells_before = world.lock().unwrap().cells.clone();
    let sessions_version = table_version(&world, sessions.id);

    let error = svc
        .apply_ops(
            receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            vec![DatabaseOp::UpdateRows {
                table: sessions.id,
                changes: RowChanges::Uniform {
                    rows: vec![row_id],
                    cells: vec![CellWrite {
                        column: title,
                        value: CellValue::Text("Hijacked".into()),
                    }],
                },
                create_missing_options: false,
            }],
        )
        .await
        .unwrap_err();
    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    assert_eq!(
        refusal,
        OpRefusal {
            op: 0,
            row: Some(0),
            column: None,
            reason: format!("no row {row_id} in this table"),
        }
    );
    let error = svc
        .apply_ops(
            receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            vec![DatabaseOp::DeleteRows {
                table: sessions.id,
                rows: vec![row_id],
            }],
        )
        .await
        .unwrap_err();
    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    assert_eq!(
        refusal,
        OpRefusal {
            op: 0,
            row: Some(0),
            column: None,
            reason: format!("no row {row_id} in this table"),
        }
    );

    assert_eq!(world.lock().unwrap().cells, cells_before);
    assert_eq!(row_ids(&world, table_id), vec![row_id]);
    assert_eq!(table_version(&world, sessions.id), sessions_version);
}

#[tokio::test]
async fn a_view_grant_reads_but_cannot_be_receipted_for_ops() {
    let seeded = seeded().await;

    let edit = EntityAccessReceipt::<EditAccessLevel>::try_new_authenticated_user(
        user(VIEWER),
        Entity {
            entity_id: seeded.database_id.to_string(),
            entity_type: EntityType::Database,
        },
        EntityPermission::AccessLevel {
            access_level: AccessLevel::View,
        },
    );
    assert!(edit.is_err(), "ops need an edit receipt");

    let detail = seeded
        .service
        .get_database(
            receipt::<ViewAccessLevel>(seeded.database_id, VIEWER, AccessLevel::View),
            viewer(VIEWER),
        )
        .await
        .unwrap();
    assert_eq!(detail.grant, AccessLevel::View);
    assert!(
        detail.tables[0]
            .columns
            .iter()
            .all(|column| !column.writable)
    );
}

#[tokio::test]
async fn grants_scope_writes_per_database() {
    let seeded = seeded().await;
    let (world, svc) = (seeded.world, seeded.service);
    let venue = svc
        .create_database(CreateDatabase {
            name: "Venue".into(),
            owner_id: user(VIEWER),
            acting_bot: None,
        })
        .await
        .unwrap();
    let rooms = {
        let mut w = world.lock().unwrap();
        let table = w
            .tables
            .iter_mut()
            .find(|t| t.database_id == venue.id)
            .unwrap();
        table.name = "Rooms".into();
        table.id
    };
    let room_name = svc
        .create_column(
            receipt::<EditAccessLevel>(venue.id, VIEWER, AccessLevel::Owner),
            viewer(VIEWER),
            CreateColumn {
                infer_type: false,
                table_id: rooms,
                binding: ColumnBinding::NewDefinition {
                    name: "Name".into(),
                    data_type: DataType::String,
                    is_multi_select: false,
                    options: vec![],
                },
                config: None,
            },
        )
        .await
        .unwrap();

    // VIEWER writes their own database...
    let written = svc
        .apply_ops(
            receipt::<EditAccessLevel>(venue.id, VIEWER, AccessLevel::Owner),
            viewer(VIEWER),
            vec![DatabaseOp::InsertRows {
                table: rooms,
                rows: vec![vec![CellWrite {
                    column: room_name,
                    value: CellValue::Text("Main Hall".into()),
                }]],
                create_missing_options: false,
            }],
        )
        .await
        .unwrap();
    assert!(matches!(
        written.as_slice(),
        [OpResult::RowsWritten {
            table_version: TableVersion(1),
            affected: 1,
            ..
        }]
    ));

    // ...but their receipt on it reaches no table of OWNER's.
    let error = svc
        .apply_ops(
            receipt::<EditAccessLevel>(venue.id, VIEWER, AccessLevel::Owner),
            viewer(VIEWER),
            vec![DatabaseOp::InsertRows {
                table: seeded.table_id,
                rows: vec![vec![]],
                create_missing_options: false,
            }],
        )
        .await
        .unwrap_err();
    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    assert_eq!(
        refusal.reason,
        format!("table {} is not in this database", seeded.table_id)
    );

    // OWNER has no grant on Venue: it is not theirs to list.
    let listed = svc.list_databases(viewer(OWNER)).await.unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|listed| listed.database.id)
            .collect::<Vec<_>>(),
        vec![seeded.database_id]
    );
    let w = world.lock().unwrap();
    assert_eq!(w.rows[&rooms].len(), 1);
    assert_eq!(w.rows[&seeded.table_id].len(), 1);
}

// ===== Select options are explicit schema =====

#[tokio::test]
async fn a_select_column_with_no_options_accepts_nothing() {
    let seeded = seeded().await;
    let (svc, database_id, table_id) = (seeded.service, seeded.database_id, seeded.table_id);
    let stage = svc
        .create_column(
            receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "Stage".into(),
                    data_type: DataType::SelectString,
                    is_multi_select: false,
                    options: vec![],
                },
                config: None,
            },
        )
        .await
        .unwrap();

    let error = svc
        .apply_ops(
            receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            vec![DatabaseOp::InsertRows {
                table: table_id,
                rows: vec![vec![CellWrite {
                    column: stage,
                    value: CellValue::Options(vec![OptionRef::Label("Main".into())]),
                }]],
                create_missing_options: false,
            }],
        )
        .await
        .unwrap_err();
    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    assert_eq!(refusal.reason, "`Main` is not an option of \"Stage\"");
}

/// The point of the operation: the write that failed succeeds once the option
/// exists, and the table's version moves because its schema did.
#[tokio::test]
async fn add_column_options_extends_what_ops_accept_and_bumps_the_version() {
    let seeded = seeded().await;
    let (world, svc, db, guests, status) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.status_column.id,
    );
    let published_before = world.lock().unwrap().published.len();

    let column = svc
        .add_column_options(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            AddColumnOptions {
                table_id: guests,
                column_id: status,
                labels: vec!["Waitlisted".into()],
            },
        )
        .await
        .expect("edit access may extend a select column");

    assert_eq!(column.sql_name, "\"Status\"");
    assert_eq!(
        catalog::option_labels(&column.definition)
            .into_iter()
            .map(|(_, label)| label)
            .collect::<Vec<_>>(),
        vec!["Going", "Declined", "Waitlisted"],
        "new options are appended, so existing labels do not move"
    );

    {
        let w = world.lock().unwrap();
        assert_eq!(w.tables[0].version, TableVersion(2));
        assert_eq!(
            w.published.len(),
            published_before + 1,
            "the schema change is announced for liveness"
        );
    }

    let inserted = svc
        .apply_ops(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            vec![DatabaseOp::InsertRows {
                table: guests,
                rows: vec![vec![CellWrite {
                    column: status,
                    value: CellValue::Options(vec![OptionRef::Label("Waitlisted".into())]),
                }]],
                create_missing_options: false,
            }],
        )
        .await
        .expect("the option now resolves");
    let [OpResult::RowsWritten { inserted, .. }] = inserted.as_slice() else {
        panic!("expected one insert, got {inserted:?}");
    };
    let waitlisted = option_id(&world, column.definition.definition.id, "Waitlisted");
    assert_eq!(
        cell(&world, inserted[0], column.definition.definition.id),
        Some(PropertyValue::SelectOption(vec![waitlisted]))
    );
}

/// Re-sending a label the column already has changes nothing: no duplicate
/// option, no version bump, no event — and no error either.
#[tokio::test]
async fn adding_an_existing_option_is_a_no_op() {
    let seeded = seeded().await;
    let (world, svc, db, guests, status) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.status_column.id,
    );
    let published_before = world.lock().unwrap().published.len();

    let column = svc
        .add_column_options(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            AddColumnOptions {
                table_id: guests,
                column_id: status,
                labels: vec!["going".into(), "  Declined  ".into()],
            },
        )
        .await
        .expect("an option that is already there is not an error");

    assert_eq!(column.definition.property_options.len(), 2);
    let w = world.lock().unwrap();
    assert_eq!(w.tables[0].version, TableVersion(1));
    assert_eq!(w.published.len(), published_before);
}

#[tokio::test]
async fn options_are_refused_on_a_column_that_cannot_hold_them() {
    let seeded = seeded().await;
    let (svc, db, guests, name_column) = (
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.name_column.id,
    );

    let err = svc
        .create_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: guests,
                binding: ColumnBinding::NewDefinition {
                    name: "Notes".into(),
                    data_type: DataType::String,
                    is_multi_select: false,
                    options: vec!["Main".into()],
                },
                config: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            DatabaseError::InvalidSchemaOperation(SchemaError::OptionsOnPlainColumn)
        ),
        "{err:?}"
    );

    // …and the same on the standalone operation, against the text column the
    // seeded table already has.
    let err = svc
        .add_column_options(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            AddColumnOptions {
                table_id: guests,
                column_id: name_column,
                labels: vec!["Main".into()],
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            DatabaseError::InvalidSchemaOperation(SchemaError::ColumnTakesNoOptions)
        ),
        "{err:?}"
    );
}

/// A numeric select stores numbers, so its labels have to be numbers, and
/// labels naming the same number are one option.
#[tokio::test]
async fn numeric_select_options_are_parsed_as_numbers() {
    let seeded = seeded().await;
    let (world, svc, db, guests) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
    );

    let err = svc
        .create_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: guests,
                binding: ColumnBinding::NewDefinition {
                    name: "Priority".into(),
                    data_type: DataType::SelectNumber,
                    is_multi_select: false,
                    options: vec!["soon".into()],
                },
                config: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, DatabaseError::InvalidSchemaOperation(SchemaError::OptionNotNumber { label }) if label == "soon"),
        "{err:?}"
    );

    let priority = svc
        .create_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: guests,
                binding: ColumnBinding::NewDefinition {
                    name: "Priority".into(),
                    data_type: DataType::SelectNumber,
                    is_multi_select: false,
                    options: vec!["1".into(), "2.0".into(), "2".into()],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    let w = world.lock().unwrap();
    let definition = w
        .columns
        .iter()
        .find(|column| column.id == priority)
        .unwrap()
        .property_definition_id;
    assert_eq!(
        w.definitions[&definition]
            .property_options
            .iter()
            .map(|option| option.value.clone())
            .collect::<Vec<_>>(),
        vec![
            PropertyOptionValue::Number(1.0),
            PropertyOptionValue::Number(2.0)
        ]
    );
}

#[tokio::test]
async fn option_labels_are_validated() {
    let seeded = seeded().await;
    let (svc, db, guests) = (seeded.service, seeded.database_id, seeded.table_id);

    for bad in ["   ", ""] {
        let err = svc
            .create_column(
                receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
                viewer(OWNER),
                CreateColumn {
                    infer_type: false,
                    table_id: guests,
                    binding: ColumnBinding::NewDefinition {
                        name: "Stage".into(),
                        data_type: DataType::SelectString,
                        is_multi_select: false,
                        options: vec![bad.into()],
                    },
                    config: None,
                },
            )
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                DatabaseError::InvalidSchemaOperation(SchemaError::EmptyOptionLabel)
            ),
            "{err:?}"
        );
    }

    let err = svc
        .create_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: guests,
                binding: ColumnBinding::NewDefinition {
                    name: "Stage".into(),
                    data_type: DataType::SelectString,
                    is_multi_select: false,
                    options: vec!["x".repeat(MAX_OPTION_LABEL_LEN + 1)],
                },
                config: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            DatabaseError::InvalidSchemaOperation(SchemaError::OptionLabelTooLong {
                max: MAX_OPTION_LABEL_LEN
            })
        ),
        "{err:?}"
    );
}

#[tokio::test]
async fn add_column_options_respects_receipts() {
    let seeded = seeded().await;
    let (world, svc, db, guests, status) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.status_column.id,
    );
    let elsewhere = Uuid::new_v4();

    let err = svc
        .add_column_options(
            receipt::<EditAccessLevel>(elsewhere, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            AddColumnOptions {
                table_id: guests,
                column_id: status,
                labels: vec!["Waitlisted".into()],
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, DatabaseError::NotFound),
        "a receipt for another database reaches nothing: {err:?}"
    );

    let err = svc
        .add_column_options(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            AddColumnOptions {
                table_id: guests,
                column_id: Uuid::new_v4(),
                labels: vec!["Waitlisted".into()],
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound), "{err:?}");

    // Nothing was written on the way to either refusal.
    assert_eq!(world.lock().unwrap().definitions.len(), 3);
}

// ===== Domain events =====

#[tokio::test]
async fn lifecycle_and_writes_publish_domain_events() {
    let seeded = seeded().await;
    let (world, svc, db, row_id) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.row_id,
    );

    // Seeding created the database and then shaped its table.
    {
        let w = world.lock().unwrap();
        assert_eq!(w.broker_events[0]["event_type"], "database.created");
        assert!(
            w.broker_events[1..]
                .iter()
                .all(|event| event["event_type"] == "database.tables_changed"),
            "{:?}",
            w.broker_events
        );
    }
    world.lock().unwrap().broker_events.clear();

    svc.rename_database(
        receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
        "Winter Offsite".into(),
    )
    .await
    .unwrap();
    svc.apply_ops(
        receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
        viewer(OWNER),
        vec![DatabaseOp::UpdateRows {
            table: seeded.table_id,
            changes: RowChanges::Uniform {
                rows: vec![row_id],
                cells: vec![CellWrite {
                    column: seeded.status_column.id,
                    value: CellValue::Options(vec![OptionRef::Label("Declined".into())]),
                }],
            },
            create_missing_options: false,
        }],
    )
    .await
    .unwrap();
    svc.trash_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    svc.restore_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    svc.delete_database_permanently(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();

    let events = world.lock().unwrap().broker_events.clone();
    assert_eq!(
        events
            .iter()
            .map(|event| event["event_type"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "database.renamed",
            "database.tables_changed",
            "database.trashed",
            "database.restored",
            "database.purged",
        ]
    );
    let renamed = &events[0]["metadata"];
    assert_eq!(renamed["database_id"], db.to_string());
    assert_eq!(renamed["name"], "Winter Offsite");
    assert_eq!(renamed["attribution"]["actor"], OWNER);
    let changed = &events[1]["metadata"];
    assert_eq!(changed["database_id"], db.to_string());
    assert_eq!(changed["attribution"]["actor"], OWNER);
    assert_eq!(
        changed["tables"],
        serde_json::json!([{ "table_id": seeded.table_id, "version": 2 }])
    );
    assert_eq!(events[4]["metadata"]["database_id"], db.to_string());
}

#[tokio::test]
async fn an_agent_is_attributed_as_acting_for_the_user() {
    let seeded = seeded().await;
    let (world, svc, db, row_id) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.row_id,
    );
    world.lock().unwrap().broker_events.clear();
    let agent = bot_id::MACRO_AI_BOT_ID;

    svc.create_database(CreateDatabase {
        name: "Agent Offsite".into(),
        owner_id: user(OWNER),
        acting_bot: Some(agent),
    })
    .await
    .unwrap();
    svc.rename_database(
        EntityAccessReceipt::try_new_bot(
            agent.into_storage_id(),
            (&entity_access::domain::models::BotAccessScope::user(user(OWNER))).into(),
            Entity {
                entity_id: db.to_string(),
                entity_type: EntityType::Database,
            },
            EntityPermission::AccessLevel {
                access_level: AccessLevel::Owner,
            },
        )
        .unwrap(),
        "Winter Offsite".into(),
    )
    .await
    .unwrap();
    svc.apply_ops(
        receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
        Viewer {
            user_id: user(OWNER),
            acting_bot: Some(agent),
        },
        vec![DatabaseOp::UpdateRows {
            table: seeded.table_id,
            changes: RowChanges::Uniform {
                rows: vec![row_id],
                cells: vec![CellWrite {
                    column: seeded.status_column.id,
                    value: CellValue::Options(vec![OptionRef::Label("Declined".into())]),
                }],
            },
            create_missing_options: false,
        }],
    )
    .await
    .unwrap();

    let events = world.lock().unwrap().broker_events.clone();
    assert_eq!(
        events
            .iter()
            .map(|event| event["event_type"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "database.created",
            "database.renamed",
            "database.tables_changed"
        ]
    );
    for event in &events {
        assert_eq!(
            event["metadata"]["attribution"],
            serde_json::json!({
                "actor": "bot|00000000-0000-0000-0000-00000000a1a1",
                "on_behalf_of": OWNER,
            }),
            "{event}"
        );
    }
}

#[tokio::test]
async fn no_op_lifecycle_calls_publish_nothing() {
    let seeded = seeded().await;
    let (world, svc, db) = (seeded.world, seeded.service, seeded.database_id);
    svc.trash_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    world.lock().unwrap().broker_events.clear();

    // Trashing twice and restoring what is not trashed change nothing, so
    // nothing is announced.
    svc.trash_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    svc.restore_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    svc.restore_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    let events = world.lock().unwrap().broker_events.clone();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["event_type"], "database.restored");
}
