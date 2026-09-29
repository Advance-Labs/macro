use super::*;

#[tokio::test]
async fn create_table_announces_only_the_committed_table() {
    let seeded = seeded().await;
    let (world, svc, db) = (seeded.world, seeded.service, seeded.database_id);
    {
        let mut world = world.lock().unwrap();
        world.published.clear();
        world.broker_events.clear();
    }
    let table = svc
        .create_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            CreateTable {
                database_id: db,
                name: "Tickets".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(table.name, "Tickets");
    assert_eq!(table.version, TableVersion(0));
    {
        let w = world.lock().unwrap();
        assert_eq!(w.published, [(table.id, TableVersion(0))]);
        assert_eq!(w.broker_events.len(), 1);
        let event = &w.broker_events[0];
        assert_eq!(event["event_type"], "database.tables_changed");
        assert_eq!(event["metadata"]["database_id"], db.to_string());
        assert_eq!(event["metadata"]["attribution"]["actor"], OWNER);
        assert_eq!(
            event["metadata"]["tables"],
            serde_json::json!([{ "table_id": table.id, "version": 0 }])
        );
    }

    let error = svc
        .create_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            CreateTable {
                database_id: db,
                name: "tickets".into(),
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DatabaseError::InvalidSchemaOperation(_)));
    let w = world.lock().unwrap();
    assert_eq!(w.published.len(), 1);
    assert_eq!(w.broker_events.len(), 1);
    assert_eq!(w.tables.len(), 2);
}

#[tokio::test]
async fn parent_disappearing_at_table_write_stays_not_found_and_publishes_nothing() {
    let seeded = seeded().await;
    let (world, svc, db, table_id) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
    );
    {
        let mut world = world.lock().unwrap();
        world.published.clear();
        world.broker_events.clear();
        world.table_write_not_found = true;
    }
    let create = svc
        .create_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            CreateTable {
                database_id: db,
                name: "Unavailable".into(),
            },
        )
        .await;
    let rename = svc
        .rename_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            table_id,
            "Unavailable".into(),
            "Guests".into(),
        )
        .await;
    assert!(matches!(create, Err(DatabaseError::NotFound)));
    assert!(matches!(rename, Err(DatabaseError::NotFound)));
    let world = world.lock().unwrap();
    assert!(world.published.is_empty());
    assert!(world.broker_events.is_empty());
    assert!(!world.tables.iter().any(|table| table.name == "Unavailable"));
}

#[tokio::test]
async fn a_new_table_is_empty_and_queryable_by_its_quoted_name() {
    let seeded = seeded().await;
    let (svc, db) = (seeded.service, seeded.database_id);
    let table = svc
        .create_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            CreateTable {
                database_id: db,
                name: "  Ticket sales ".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(table.name, "Ticket sales");

    let detail = svc
        .get_database(
            receipt::<ViewAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
        )
        .await
        .unwrap();
    assert_eq!(
        detail
            .tables
            .iter()
            .map(|table| table.sql_name.as_str())
            .collect::<Vec<_>>(),
        vec!["\"Guests\"", "\"Ticket sales\""]
    );
    assert!(detail.tables[1].columns.is_empty());

    let answer = svc
        .query_sql(
            viewer(OWNER),
            "SELECT COUNT(*) FROM \"Ticket sales\"".into(),
        )
        .await
        .unwrap();
    assert_eq!(answer.results[0].rows, vec![vec![SqlValue::Real(0.0)]]);
    assert_eq!(answer.read_tables, vec![table.id]);
}
