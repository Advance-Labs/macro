//! Typed ops through the service as hosts build it, over Postgres: a batch
//! commits whole, and a refused op leaves nothing of it behind.

use std::sync::Arc;

use entity_access::domain::models::{
    AccessLevel, EditAccessLevel, Entity, EntityAccessReceipt, EntityPermission, EntityType,
};
use entity_access::domain::service::EntityAccessServiceImpl;
use entity_access::outbound::PgAccessRepository;
use macro_db_migrator::MACRO_DB_MIGRATIONS;
use macro_event_broker::NoopMacroEventBroker;
use macro_user_id::{cowlike::CowLike, user_id::MacroUserIdStr};
use models_databases::{
    CellValue, CellWrite, ColumnKind, DatabaseOp, OpResult, OptionRef, RowChange, RowChanges,
};
use models_properties::service::property_option::PropertyOptionValue;
use models_properties::service::property_value::PropertyValue;
use models_properties::shared::{DataType, EntityReference, EntityType as PropertyEntityType};
use properties::outbound::properties_pg_repo::PropertiesPgRepo;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::models::{
    ChangeColumnType, ColumnBinding, ColumnConfig, CreateColumn, CreateDatabase, CreateTable,
    DatabaseError, DatabaseId, NewOption, OpRefusal, TableVersion, Viewer,
};
use crate::domain::ports::{CellStore, ColumnDefinitionStore, DatabasesRepo, DatabasesService};
use crate::outbound::gateway_event_publisher::NoOpTableEventPublisher;
use crate::outbound::pg_cell_store::PgCellStore;
use crate::outbound::pg_databases_repo::PgDatabasesRepo;
use crate::outbound::pg_definition_store::PgDefinitionStore;
use crate::wiring::{PgDatabasesService, build_service};

const USER: &str = "macro|apply-ops@macro.com";

pub(super) fn viewer() -> Viewer {
    Viewer {
        user_id: MacroUserIdStr::parse_from_str(USER).unwrap().into_owned(),
        acting_bot: None,
    }
}

pub(super) fn edit(database_id: DatabaseId) -> EntityAccessReceipt<EditAccessLevel> {
    EntityAccessReceipt::try_new_authenticated_user(
        MacroUserIdStr::parse_from_str(USER).unwrap().into_owned(),
        Entity {
            entity_id: database_id.to_string(),
            entity_type: EntityType::Database,
        },
        EntityPermission::AccessLevel {
            access_level: AccessLevel::Owner,
        },
    )
    .unwrap()
}

pub(super) async fn insert_user(pool: &PgPool) {
    insert_named_user(pool, USER).await;
}

pub(super) async fn insert_named_user(pool: &PgPool, user: &str) {
    let id = macro_uuid::generate_uuid_v7();
    sqlx::query!(r#"INSERT INTO macro_user (id, username, email, stripe_customer_id) VALUES ($1, $2, $2, $2)"#, id, user)
        .execute(pool).await.unwrap();
    sqlx::query!(
        r#"INSERT INTO "User" (id, email, macro_user_id) VALUES ($1, $1, $2)"#,
        user,
        id
    )
    .execute(pool)
    .await
    .unwrap();
}

/// `Guests(Name TEXT, Status SELECT[Going])` in a new database.
pub(super) struct Guests {
    pub(super) database_id: DatabaseId,
    pub(super) table_id: Uuid,
    pub(super) name: Uuid,
    pub(super) name_definition: Uuid,
    pub(super) status: Uuid,
    pub(super) status_definition: Uuid,
}

pub(super) async fn guests(pool: &PgPool) -> Guests {
    insert_user(pool).await;
    let service = service(pool);
    let database = service
        .create_database(CreateDatabase {
            name: "Offsite".into(),
            owner_id: viewer().user_id,
            acting_bot: None,
        })
        .await
        .unwrap();
    let repo = PgDatabasesRepo::new(pool.clone(), PropertiesPgRepo::new(pool.clone()));
    let table_id = repo.get_database(database.id).await.unwrap().unwrap().1[0].id;
    // The database starts with its title column, Name.
    let name = repo.columns_for_tables(&[table_id]).await.unwrap()[0].id;
    let status = service
        .create_column(
            edit(database.id),
            viewer(),
            CreateColumn {
                infer_type: false,
                table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "Status".into(),
                    data_type: DataType::SelectString,
                    is_multi_select: false,
                    options: vec!["Going".into()],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    let columns = repo.columns_for_tables(&[table_id]).await.unwrap();
    let definition_of = |column: Uuid| {
        columns
            .iter()
            .find(|placement| placement.id == column)
            .unwrap()
            .property_definition_id
    };
    Guests {
        database_id: database.id,
        table_id,
        name,
        name_definition: definition_of(name),
        status,
        status_definition: definition_of(status),
    }
}

/// The service as hosts build it, with no liveness or domain events.
pub(super) fn service(
    pool: &PgPool,
) -> PgDatabasesService<
    NoOpTableEventPublisher,
    NoopMacroEventBroker,
    EntityAccessServiceImpl<PgAccessRepository>,
> {
    build_service(
        pool.clone(),
        Arc::new(EntityAccessServiceImpl::new(PgAccessRepository::new(
            pool.clone(),
        ))),
        NoOpTableEventPublisher,
        NoopMacroEventBroker,
    )
}

pub(super) fn cells(pool: &PgPool) -> PgCellStore<PropertiesPgRepo> {
    PgCellStore::new(pool.clone(), PropertiesPgRepo::new(pool.clone()))
}

pub(super) async fn version(pool: &PgPool, table_id: Uuid) -> TableVersion {
    PgDatabasesRepo::new(pool.clone(), PropertiesPgRepo::new(pool.clone()))
        .table_versions(&[table_id])
        .await
        .unwrap()[&table_id]
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn ops_insert_update_and_delete_rows_bumping_the_table_once_per_request(pool: PgPool) {
    let guests = guests(&pool).await;
    let service = service(&pool);
    let before = version(&pool, guests.table_id).await;

    let inserted = service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::InsertRows {
                table: guests.table_id,
                rows: vec![
                    vec![CellWrite {
                        column: guests.name,
                        value: CellValue::Text("Sam".into()),
                    }],
                    vec![CellWrite {
                        column: guests.name,
                        value: CellValue::Text("Alex".into()),
                    }],
                    vec![CellWrite {
                        column: guests.name,
                        value: CellValue::Text("Robin".into()),
                    }],
                ],
                create_missing_options: false,
            }],
        )
        .await
        .unwrap();
    let [
        OpResult::RowsWritten {
            table_version,
            inserted,
            affected: 3,
        },
    ] = inserted.as_slice()
    else {
        panic!("expected one insert of three rows, got {inserted:?}");
    };
    assert_eq!(*table_version, TableVersion(before.0 + 1));
    let rows = PgDatabasesRepo::new(pool.clone(), PropertiesPgRepo::new(pool.clone()))
        .row_refs(guests.table_id)
        .await
        .unwrap();
    assert_eq!(
        rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        inserted.clone()
    );
    let (sam, alex, robin) = (inserted[0], inserted[1], inserted[2]);

    let going = PgDefinitionStore::new(pool.clone(), PropertiesPgRepo::new(pool.clone()))
        .definitions(&[guests.status_definition])
        .await
        .unwrap()[0]
        .property_options[0]
        .id;
    let results = service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![
                DatabaseOp::UpdateRows {
                    table: guests.table_id,
                    changes: RowChanges::Uniform {
                        rows: vec![sam, alex, robin],
                        cells: vec![CellWrite {
                            column: guests.status,
                            value: CellValue::Options(vec![OptionRef::Id(going)]),
                        }],
                    },
                    create_missing_options: false,
                },
                DatabaseOp::UpdateRows {
                    table: guests.table_id,
                    changes: RowChanges::PerRow {
                        rows: vec![
                            RowChange {
                                row: sam,
                                cells: vec![CellWrite {
                                    column: guests.name,
                                    value: CellValue::Text("Samantha".into()),
                                }],
                            },
                            RowChange {
                                row: alex,
                                cells: vec![CellWrite {
                                    column: guests.status,
                                    value: CellValue::Clear,
                                }],
                            },
                        ],
                    },
                    create_missing_options: false,
                },
                DatabaseOp::DeleteRows {
                    table: guests.table_id,
                    rows: vec![robin],
                },
            ],
        )
        .await
        .unwrap();

    let after = TableVersion(before.0 + 2);
    assert_eq!(
        results,
        vec![
            OpResult::RowsWritten {
                table_version: after,
                inserted: vec![],
                affected: 3,
            },
            OpResult::RowsWritten {
                table_version: after,
                inserted: vec![],
                affected: 2,
            },
            OpResult::RowsWritten {
                table_version: after,
                inserted: vec![],
                affected: 1,
            },
        ]
    );
    assert_eq!(version(&pool, guests.table_id).await, after);
    let rows = PgDatabasesRepo::new(pool.clone(), PropertiesPgRepo::new(pool.clone()))
        .row_refs(guests.table_id)
        .await
        .unwrap();
    assert_eq!(
        rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![sam, alex]
    );
    let stored = cells(&pool).cells(&[sam, alex, robin]).await.unwrap();
    assert_eq!(
        stored[&sam][&guests.name_definition],
        PropertyValue::Str("Samantha".into())
    );
    assert_eq!(
        stored[&sam][&guests.status_definition],
        PropertyValue::SelectOption(vec![going])
    );
    assert_eq!(
        stored[&alex][&guests.name_definition],
        PropertyValue::Str("Alex".into())
    );
    assert!(!stored[&alex].contains_key(&guests.status_definition));
    assert!(stored.get(&robin).is_none_or(|cells| cells.is_empty()));
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn a_new_label_becomes_a_palette_coloured_option_and_an_unknown_one_is_refused(pool: PgPool) {
    let guests = guests(&pool).await;
    let service = service(&pool);
    let definitions = PgDefinitionStore::new(pool.clone(), PropertiesPgRepo::new(pool.clone()));

    let refused = service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::InsertRows {
                table: guests.table_id,
                rows: vec![vec![CellWrite {
                    column: guests.status,
                    value: CellValue::Options(vec![OptionRef::Label("Maybe".into())]),
                }]],
                create_missing_options: false,
            }],
        )
        .await
        .unwrap_err();
    let DatabaseError::InvalidOp(refusal) = refused else {
        panic!("expected a refused op, got {refused:?}");
    };
    assert_eq!(
        refusal,
        OpRefusal {
            op: 0,
            row: Some(0),
            column: Some(guests.status),
            reason: "`Maybe` is not an option of \"Status\"".into(),
        }
    );

    let results = service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::InsertRows {
                table: guests.table_id,
                rows: vec![vec![CellWrite {
                    column: guests.status,
                    value: CellValue::Options(vec![OptionRef::Label("Maybe".into())]),
                }]],
                create_missing_options: true,
            }],
        )
        .await
        .unwrap();

    let options = definitions
        .definitions(&[guests.status_definition])
        .await
        .unwrap()
        .remove(0)
        .property_options;
    assert_eq!(options.len(), 2);
    let maybe = &options[1];
    assert_eq!(maybe.value, PropertyOptionValue::String("Maybe".into()));
    assert_eq!(maybe.display_order, 1);
    assert_eq!(
        maybe.color.as_deref(),
        Some(option_palette::OptionColor::for_position(1).hex())
    );
    let [OpResult::RowsWritten { inserted, .. }] = results.as_slice() else {
        panic!("expected one insert, got {results:?}");
    };
    let stored = cells(&pool).cells(inserted).await.unwrap();
    assert_eq!(
        stored[&inserted[0]][&guests.status_definition],
        PropertyValue::SelectOption(vec![maybe.id])
    );
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn a_refused_op_leaves_nothing_of_its_batch_behind(pool: PgPool) {
    let guests = guests(&pool).await;
    let service = service(&pool);
    let before = version(&pool, guests.table_id).await;
    let ghost = macro_uuid::generate_uuid_v7();

    let error = service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![
                DatabaseOp::InsertRows {
                    table: guests.table_id,
                    rows: vec![vec![CellWrite {
                        column: guests.status,
                        value: CellValue::Options(vec![OptionRef::Label("Maybe".into())]),
                    }]],
                    create_missing_options: true,
                },
                DatabaseOp::DeleteRows {
                    table: guests.table_id,
                    rows: vec![ghost],
                },
            ],
        )
        .await
        .unwrap_err();

    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    assert_eq!(
        refusal,
        OpRefusal {
            op: 1,
            row: Some(0),
            column: None,
            reason: format!("no row {ghost} in this table"),
        }
    );
    assert!(
        PgDatabasesRepo::new(pool.clone(), PropertiesPgRepo::new(pool.clone()))
            .row_refs(guests.table_id)
            .await
            .unwrap()
            .is_empty()
    );
    let options = PgDefinitionStore::new(pool.clone(), PropertiesPgRepo::new(pool.clone()))
        .definitions(&[guests.status_definition])
        .await
        .unwrap()
        .remove(0)
        .property_options;
    assert_eq!(options.len(), 1);
    assert_eq!(version(&pool, guests.table_id).await, before);
    let orphaned = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM entity_properties WHERE property_definition_id = $1",
        guests.status_definition
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(orphaned, Some(0));

    let elsewhere = service
        .create_database(CreateDatabase {
            name: "Elsewhere".into(),
            owner_id: viewer().user_id,
            acting_bot: None,
        })
        .await
        .unwrap();
    let elsewhere_table = PgDatabasesRepo::new(pool.clone(), PropertiesPgRepo::new(pool.clone()))
        .get_database(elsewhere.id)
        .await
        .unwrap()
        .unwrap()
        .1[0]
        .id;
    let error = service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![
                DatabaseOp::InsertRows {
                    table: guests.table_id,
                    rows: vec![vec![]],
                    create_missing_options: false,
                },
                DatabaseOp::InsertRows {
                    table: elsewhere_table,
                    rows: vec![vec![]],
                    create_missing_options: false,
                },
            ],
        )
        .await
        .unwrap_err();
    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    assert_eq!(refusal.op, 1);
    for table in [guests.table_id, elsewhere_table] {
        assert!(
            PgDatabasesRepo::new(pool.clone(), PropertiesPgRepo::new(pool.clone()))
                .row_refs(table)
                .await
                .unwrap()
                .is_empty()
        );
    }
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn a_relation_cell_holds_rows_of_its_target_table(pool: PgPool) {
    let guests = guests(&pool).await;
    let service = service(&pool);
    let sessions = service
        .create_table(
            edit(guests.database_id),
            CreateTable {
                database_id: guests.database_id,
                name: "Sessions".into(),
            },
        )
        .await
        .unwrap();
    let relation = service
        .create_column(
            edit(guests.database_id),
            viewer(),
            CreateColumn {
                infer_type: false,
                table_id: guests.table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "Sessions".into(),
                    data_type: DataType::Entity,
                    is_multi_select: true,
                    options: vec![],
                },
                config: Some(ColumnConfig::Link {
                    database_id: guests.database_id,
                    table_id: sessions.id,
                }),
            },
        )
        .await
        .unwrap();
    let relation_definition =
        PgDatabasesRepo::new(pool.clone(), PropertiesPgRepo::new(pool.clone()))
            .columns_for_tables(&[guests.table_id])
            .await
            .unwrap()
            .into_iter()
            .find(|column| column.id == relation)
            .unwrap()
            .property_definition_id;
    let keynote = service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::InsertRows {
                table: sessions.id,
                rows: vec![vec![]],
                create_missing_options: false,
            }],
        )
        .await
        .unwrap();
    let [OpResult::RowsWritten { inserted, .. }] = keynote.as_slice() else {
        panic!("expected one insert, got {keynote:?}");
    };
    let keynote = inserted[0];

    let guest = service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::InsertRows {
                table: guests.table_id,
                rows: vec![vec![CellWrite {
                    column: relation,
                    value: CellValue::Rows(vec![keynote]),
                }]],
                create_missing_options: false,
            }],
        )
        .await
        .unwrap();
    let [OpResult::RowsWritten { inserted, .. }] = guest.as_slice() else {
        panic!("expected one insert, got {guest:?}");
    };
    let guest = inserted[0];
    assert_eq!(
        cells(&pool).cells(&[guest]).await.unwrap()[&guest][&relation_definition],
        PropertyValue::EntityRef(vec![EntityReference {
            entity_id: keynote.to_string(),
            entity_type: PropertyEntityType::DatabaseRow,
            specific_message_id: None,
        }])
    );

    let error = service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::UpdateRows {
                table: guests.table_id,
                changes: RowChanges::PerRow {
                    rows: vec![RowChange {
                        row: guest,
                        cells: vec![CellWrite {
                            column: relation,
                            value: CellValue::Rows(vec![guest]),
                        }],
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
            column: Some(relation),
            reason: format!("row {guest} is not a row of the related table"),
        }
    );
    assert_eq!(
        cells(&pool).cells(&[guest]).await.unwrap()[&guest][&relation_definition],
        PropertyValue::EntityRef(vec![EntityReference {
            entity_id: keynote.to_string(),
            entity_type: PropertyEntityType::DatabaseRow,
            specific_message_id: None,
        }])
    );
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn a_type_change_through_ops_converts_like_change_column_type(pool: PgPool) {
    let guests = guests(&pool).await;
    let service = service(&pool);
    let inserted = service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::InsertRows {
                table: guests.table_id,
                rows: vec![
                    vec![CellWrite {
                        column: guests.name,
                        value: CellValue::Text("12".into()),
                    }],
                    vec![CellWrite {
                        column: guests.name,
                        value: CellValue::Text("TBD".into()),
                    }],
                ],
                create_missing_options: false,
            }],
        )
        .await
        .unwrap();
    let [OpResult::RowsWritten { inserted, .. }] = inserted.as_slice() else {
        panic!("expected one insert, got {inserted:?}");
    };

    let refused = service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::ChangeColumnType {
                table: guests.table_id,
                column: guests.name,
                to: ColumnKind::Number,
                clear_invalid: false,
            }],
        )
        .await
        .unwrap_err();
    let direct = service
        .change_column_type(
            edit(guests.database_id),
            viewer(),
            ChangeColumnType {
                table_id: guests.table_id,
                column_id: guests.name,
                data_type: DataType::Number,
                is_multi_select: false,
                specific_entity_type: None,
                relation: None,
                base_version: version(&pool, guests.table_id).await,
                clear_invalid: false,
            },
        )
        .await
        .unwrap_err();
    let (DatabaseError::InvalidOp(refusal), DatabaseError::InvalidSchemaOperation(reason)) =
        (refused, direct)
    else {
        panic!("expected both to refuse the misfit");
    };
    assert_eq!(
        refusal,
        OpRefusal {
            op: 0,
            row: None,
            column: Some(guests.name),
            reason: reason.to_string(),
        }
    );

    let results = service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::ChangeColumnType {
                table: guests.table_id,
                column: guests.name,
                to: ColumnKind::Number,
                clear_invalid: true,
            }],
        )
        .await
        .unwrap();
    assert_eq!(
        results,
        vec![OpResult::ColumnTyped {
            table_version: version(&pool, guests.table_id).await,
            cleared_cells: 1,
            trimmed_cells: 0,
        }]
    );
    let number = PgDatabasesRepo::new(pool.clone(), PropertiesPgRepo::new(pool.clone()))
        .columns_for_tables(&[guests.table_id])
        .await
        .unwrap()
        .into_iter()
        .find(|column| column.id == guests.name)
        .unwrap()
        .property_definition_id;
    let stored = cells(&pool).cells(inserted).await.unwrap();
    assert_eq!(stored[&inserted[0]][&number], PropertyValue::Num(12.0));
    assert!(
        stored
            .get(&inserted[1])
            .is_none_or(|row| !row.contains_key(&number))
    );
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn an_option_is_relabelled_and_recoloured_in_place(pool: PgPool) {
    let guests = guests(&pool).await;
    let service = service(&pool);
    let definitions = PgDefinitionStore::new(pool.clone(), PropertiesPgRepo::new(pool.clone()));
    let going = definitions
        .definitions(&[guests.status_definition])
        .await
        .unwrap()
        .remove(0)
        .property_options[0]
        .id;
    let before = version(&pool, guests.table_id).await;

    let results = service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::UpdateOption {
                table: guests.table_id,
                column: guests.status,
                option: going,
                label: Some("Attending".into()),
                color: Some(Some(models_databases::OptionColor::Pink)),
            }],
        )
        .await
        .unwrap();

    assert_eq!(
        results,
        vec![OpResult::OptionChanged {
            table_version: TableVersion(before.0 + 1),
        }]
    );
    let option = definitions
        .definitions(&[guests.status_definition])
        .await
        .unwrap()
        .remove(0)
        .property_options
        .remove(0);
    assert_eq!(
        (option.id, option.value, option.color),
        (
            going,
            PropertyOptionValue::String("Attending".into()),
            Some("#E93D82".into())
        )
    );
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn a_removed_option_leaves_its_cells_empty_and_the_others_alone(pool: PgPool) {
    let guests = guests(&pool).await;
    let service = service(&pool);
    let inserted = service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::InsertRows {
                table: guests.table_id,
                rows: vec![
                    vec![CellWrite {
                        column: guests.status,
                        value: CellValue::Options(vec![OptionRef::Label("Going".into())]),
                    }],
                    vec![CellWrite {
                        column: guests.status,
                        value: CellValue::Options(vec![OptionRef::Label("Maybe".into())]),
                    }],
                ],
                create_missing_options: true,
            }],
        )
        .await
        .unwrap();
    let [OpResult::RowsWritten { inserted, .. }] = inserted.as_slice() else {
        panic!("expected one insert, got {inserted:?}");
    };
    let definitions = PgDefinitionStore::new(pool.clone(), PropertiesPgRepo::new(pool.clone()));
    let options = definitions
        .definitions(&[guests.status_definition])
        .await
        .unwrap()
        .remove(0)
        .property_options;
    let (going, maybe) = (options[0].id, options[1].id);

    service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::DeleteOption {
                table: guests.table_id,
                column: guests.status,
                option: going,
            }],
        )
        .await
        .unwrap();

    let remaining: Vec<Uuid> = definitions
        .definitions(&[guests.status_definition])
        .await
        .unwrap()
        .remove(0)
        .property_options
        .iter()
        .map(|option| option.id)
        .collect();
    assert_eq!(remaining, vec![maybe]);
    let stored = cells(&pool).cells(inserted).await.unwrap();
    assert_eq!(
        stored
            .get(&inserted[0])
            .and_then(|row| row.get(&guests.status_definition)),
        None
    );
    assert_eq!(
        stored[&inserted[1]][&guests.status_definition],
        PropertyValue::SelectOption(vec![maybe])
    );
    let emptied = sqlx::query_scalar!(
        r#"SELECT values AS "values: serde_json::Value" FROM entity_properties
           WHERE entity_id = $1 AND property_definition_id = $2"#,
        inserted[0].to_string(),
        guests.status_definition,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(emptied, Some(serde_json::Value::Null));
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn a_shared_property_is_editable_by_its_owner_alone(pool: PgPool) {
    use properties::domain::model::PropertyDefinitionOwner;
    use properties::domain::ports::PropertiesRepo;

    insert_user(&pool).await;
    insert_named_user(&pool, "macro|someone-else@macro.com").await;
    let properties = PropertiesPgRepo::new(pool.clone());
    let owner = MacroUserIdStr::parse_from_str(USER).unwrap();
    let someone_else = MacroUserIdStr::parse_from_str("macro|someone-else@macro.com").unwrap();
    let mine = properties
        .create_property_definition(
            PropertyDefinitionOwner::User(&owner),
            "Budget",
            DataType::SelectString,
            false,
            None,
            vec![],
        )
        .await
        .unwrap();
    let theirs = properties
        .create_property_definition(
            PropertyDefinitionOwner::User(&someone_else),
            "Budget",
            DataType::SelectString,
            false,
            None,
            vec![],
        )
        .await
        .unwrap();

    let editable = PgDefinitionStore::new(pool.clone(), properties)
        .editable_definitions(&viewer(), &[mine.id, theirs.id])
        .await
        .unwrap();

    assert_eq!(editable, vec![mine.id]);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn appended_options_follow_the_existing_ones_in_place_and_colour(pool: PgPool) {
    let guests = guests(&pool).await;
    let before = version(&pool, guests.table_id).await;
    let maybe = macro_uuid::generate_uuid_v7();
    let declined = macro_uuid::generate_uuid_v7();

    let after = cells(&pool)
        .add_options(
            guests.table_id,
            &[
                NewOption {
                    definition_id: guests.status_definition,
                    id: maybe,
                    value: PropertyOptionValue::String("Maybe".into()),
                },
                NewOption {
                    definition_id: guests.status_definition,
                    id: declined,
                    value: PropertyOptionValue::String("Declined".into()),
                },
            ],
        )
        .await
        .unwrap();

    assert_eq!(after, Some(TableVersion(before.0 + 1)));
    let options = &PgDefinitionStore::new(pool.clone(), PropertiesPgRepo::new(pool.clone()))
        .definitions(&[guests.status_definition])
        .await
        .unwrap()[0]
        .property_options;
    let stored: Vec<(i32, PropertyOptionValue, Option<&str>)> = options
        .iter()
        .map(|option| {
            (
                option.display_order,
                option.value.clone(),
                option.color.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        stored,
        [
            (
                0,
                PropertyOptionValue::String("Going".into()),
                Some("#0091FF")
            ),
            (
                1,
                PropertyOptionValue::String("Maybe".into()),
                Some("#46A758")
            ),
            (
                2,
                PropertyOptionValue::String("Declined".into()),
                Some("#8E4EC6")
            ),
        ]
    );
}
