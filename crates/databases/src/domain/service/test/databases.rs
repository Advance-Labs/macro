//! Database lifecycle and schema operations: create, list, rename, trash,
//! restore, delete, and the receipts they take.

use super::*;

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
        let world_state = world.lock().unwrap();
        assert_eq!(world_state.tables.len(), 1);
        assert_eq!(world_state.tables[0].name, "Table 1");
        assert_eq!(world_state.tables[0].database_id, db.id);
    }
    let detail = svc
        .get_database(
            receipt::<ViewAccessLevel>(db.id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
        )
        .await
        .unwrap();
    let columns = &detail.tables[0].columns;
    assert_eq!(columns.len(), 1, "one request yields a usable table");
    assert_eq!(columns[0].name(), "Name");
    assert_eq!(columns[0].definition.definition.data_type, DataType::String);
    assert!(!columns[0].definition.definition.is_multi_select);
    assert!(!columns[0].shared_outside_database);

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
