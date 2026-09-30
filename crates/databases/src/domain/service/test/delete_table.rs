use super::*;

#[tokio::test]
async fn deleting_a_table_removes_its_rows_cells_and_columns() {
    let seeded = seeded().await;
    let (world, svc, db, guests, sam) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.row_id,
    );
    let tickets = svc
        .create_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            CreateTable {
                database_id: db,
                name: "Tickets".into(),
            },
        )
        .await
        .unwrap();
    let guests_version = {
        let mut w = world.lock().unwrap();
        w.published.clear();
        w.broker_events.clear();
        w.tables.iter().find(|t| t.id == guests).unwrap().version
    };

    svc.delete_table(
        receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
        guests,
    )
    .await
    .unwrap();

    {
        let w = world.lock().unwrap();
        assert_eq!(
            w.tables.iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![tickets.id]
        );
        assert!(!w.columns.iter().any(|c| c.table_id == guests));
        assert!(!w.rows.contains_key(&guests));
        assert!(!w.cells.contains_key(&sam), "the row's cells are cleared");
        assert_eq!(w.published, [(guests, guests_version)]);
        assert_eq!(w.broker_events.len(), 1);
        assert_eq!(w.broker_events[0]["event_type"], "database.tables_changed");
        assert_eq!(
            w.broker_events[0]["metadata"]["tables"],
            serde_json::json!([{ "table_id": guests, "version": guests_version.0 }])
        );
    }

    let last = svc
        .delete_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            tickets.id,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&last, DatabaseError::InvalidSchemaOperation(message) if message.contains("only table")),
        "{last:?}"
    );
    assert_eq!(world.lock().unwrap().tables.len(), 1);

    let gone = svc
        .delete_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            guests,
        )
        .await
        .unwrap_err();
    assert!(matches!(gone, DatabaseError::NotFound), "{gone:?}");
}

#[tokio::test]
async fn a_table_another_table_relates_to_is_not_deleted() {
    let seeded = seeded().await;
    let (world, svc, db, guests) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
    );
    let invites = svc
        .create_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            CreateTable {
                database_id: db,
                name: "Invites".into(),
            },
        )
        .await
        .unwrap();
    svc.create_column(
        receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
        viewer(OWNER),
        CreateColumn {
            infer_type: false,
            table_id: invites.id,
            binding: ColumnBinding::NewDefinition {
                name: "Guest".into(),
                data_type: DataType::Entity,
                is_multi_select: true,
                options: vec![],
            },
            config: Some(ColumnConfig::Link {
                database_id: db,
                table_id: guests,
            }),
        },
    )
    .await
    .unwrap();

    let error = svc
        .delete_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            guests,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            &error,
            DatabaseError::InvalidSchemaOperation(message)
                if message == "Column `Guest` of table `Invites` relates to rows of `Guests`. Delete that column first."
        ),
        "{error:?}"
    );
    let w = world.lock().unwrap();
    assert!(w.tables.iter().any(|t| t.id == guests));
    assert!(w.cells.contains_key(&seeded.row_id));
}
