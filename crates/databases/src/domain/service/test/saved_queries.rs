use super::*;

#[test]
fn a_query_definition_is_stored_as_versioned_json() {
    let definition = QueryDefinition::V1 {
        query: "SELECT COUNT(*) FROM \"Guests\"".into(),
    };
    assert_eq!(
        serde_json::to_value(&definition).unwrap(),
        serde_json::json!({"version": 1, "query": "SELECT COUNT(*) FROM \"Guests\""})
    );
    assert_eq!(
        serde_json::from_value::<QueryDefinition>(
            serde_json::json!({"version": 1, "query": "SELECT 1"})
        )
        .unwrap(),
        QueryDefinition::V1 {
            query: "SELECT 1".into()
        }
    );
    let unknown = serde_json::from_value::<QueryDefinition>(
        serde_json::json!({"version": 2, "query": "SELECT 1"}),
    )
    .unwrap_err();
    assert!(
        unknown
            .to_string()
            .contains("unsupported query definition version 2"),
        "{unknown}"
    );
}

#[tokio::test]
async fn a_viewer_of_its_database_reads_and_runs_a_saved_query() {
    let seeded = seeded().await;
    let (world, svc, db) = (seeded.world, seeded.service, seeded.database_id);
    let saved = svc
        .save_query(
            viewer(OWNER),
            Some(db),
            QueryDefinition::V1 {
                query: "SELECT COUNT(*) AS guests FROM \"Guests\"".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(saved.database_id, Some(db));
    assert_eq!(saved.created_by, OWNER);
    assert_eq!(
        saved.definition,
        QueryDefinition::V1 {
            query: "SELECT COUNT(*) AS guests FROM \"Guests\"".into()
        }
    );
    assert_eq!(world.lock().unwrap().queries, vec![saved.clone()]);

    assert_eq!(
        svc.get_query(viewer(VIEWER), saved.id).await.unwrap(),
        saved
    );
    let shared = svc.run_query(viewer(VIEWER), saved.id).await.unwrap();
    assert_eq!(shared.results[0].columns[0].name, "guests");
    assert_eq!(shared.results[0].rows, vec![vec![SqlValue::Real(1.0)]]);
    assert_eq!(shared.changes_applied, 0);
}

#[tokio::test]
async fn a_stranger_cannot_tell_a_saved_query_exists() {
    let seeded = seeded().await;
    let (svc, db) = (seeded.service, seeded.database_id);
    let saved = svc
        .save_query(
            viewer(OWNER),
            Some(db),
            QueryDefinition::V1 {
                query: "SELECT COUNT(*) AS guests FROM \"Guests\"".into(),
            },
        )
        .await
        .unwrap();

    let read = svc.get_query(viewer(STRANGER), saved.id).await.unwrap_err();
    assert!(matches!(read, QueryError::NotFound), "{read:?}");
    let run = svc.run_query(viewer(STRANGER), saved.id).await.unwrap_err();
    assert!(matches!(run, QueryError::NotFound), "{run:?}");
}

#[tokio::test]
async fn an_unscoped_saved_query_is_its_creators_alone() {
    let seeded = seeded().await;
    let svc = seeded.service;
    let saved = svc
        .save_query(
            viewer(VIEWER),
            None,
            QueryDefinition::V1 {
                query: "SELECT COUNT(*) AS guests FROM \"Guests\"".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(saved.database_id, None);

    assert_eq!(
        svc.get_query(viewer(VIEWER), saved.id).await.unwrap(),
        saved
    );
    let answer = svc.run_query(viewer(VIEWER), saved.id).await.unwrap();
    assert_eq!(answer.results[0].rows, vec![vec![SqlValue::Real(1.0)]]);

    // The database's owner can see the table, but not this viewer's query.
    let read = svc.get_query(viewer(OWNER), saved.id).await.unwrap_err();
    assert!(matches!(read, QueryError::NotFound), "{read:?}");
    let run = svc.run_query(viewer(OWNER), saved.id).await.unwrap_err();
    assert!(matches!(run, QueryError::NotFound), "{run:?}");
}

#[tokio::test]
async fn a_saved_query_resolves_names_in_its_own_database() {
    let seeded = seeded().await;
    let (world, svc, db) = (seeded.world, seeded.service, seeded.database_id);
    let twin = svc
        .create_database(CreateDatabase {
            name: "Offsite".into(),
            owner_id: user(OWNER),
            acting_bot: None,
        })
        .await
        .unwrap();
    {
        let mut w = world.lock().unwrap();
        let starter = w
            .tables
            .iter_mut()
            .find(|table| table.database_id == twin.id)
            .unwrap();
        starter.name = "Guests".into();
    }
    let in_twin = svc
        .save_query(
            viewer(OWNER),
            Some(twin.id),
            QueryDefinition::V1 {
                query: "SELECT COUNT(*) FROM \"Offsite\".\"Guests\"".into(),
            },
        )
        .await
        .unwrap();
    let in_original = svc
        .save_query(
            viewer(OWNER),
            Some(db),
            QueryDefinition::V1 {
                query: "SELECT COUNT(*) FROM \"Offsite\".\"Guests\"".into(),
            },
        )
        .await
        .unwrap();

    let twin_answer = svc.run_query(viewer(OWNER), in_twin.id).await.unwrap();
    assert_eq!(twin_answer.results[0].rows, vec![vec![SqlValue::Real(0.0)]]);
    assert_eq!(twin_answer.read_database_ids, vec![twin.id]);
    let original_answer = svc.run_query(viewer(OWNER), in_original.id).await.unwrap();
    assert_eq!(
        original_answer.results[0].rows,
        vec![vec![SqlValue::Real(1.0)]]
    );
    assert_eq!(original_answer.read_database_ids, vec![db]);
}

#[tokio::test]
async fn only_a_compiling_select_over_a_visible_database_is_saved() {
    let seeded = seeded().await;
    let (world, svc, db) = (seeded.world, seeded.service, seeded.database_id);

    let write = svc
        .save_query(
            viewer(OWNER),
            Some(db),
            QueryDefinition::V1 {
                query: "INSERT INTO \"Guests\" (\"Name\") VALUES ('Ada')".into(),
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(write, QueryError::ReadOnly(_)), "{write:?}");

    let misspelled = svc
        .save_query(
            viewer(OWNER),
            Some(db),
            QueryDefinition::V1 {
                query: "SELECT statuz FROM \"Guests\"".into(),
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&misspelled, QueryError::Sql(message) if message.contains("statuz")),
        "{misspelled:?}"
    );

    let invisible = svc
        .save_query(
            viewer(STRANGER),
            Some(db),
            QueryDefinition::V1 {
                query: "SELECT COUNT(*) FROM \"Guests\"".into(),
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(invisible, QueryError::NotFound), "{invisible:?}");

    world.lock().unwrap().databases[0].trashed_at = Some(Utc::now());
    let trashed = svc
        .save_query(
            viewer(OWNER),
            Some(db),
            QueryDefinition::V1 {
                query: "SELECT COUNT(*) FROM \"Guests\"".into(),
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(trashed, QueryError::NotFound), "{trashed:?}");

    assert!(world.lock().unwrap().queries.is_empty());
}

#[tokio::test]
async fn running_an_unknown_saved_query_is_not_found() {
    let seeded = seeded().await;
    let missing = seeded
        .service
        .run_query(viewer(OWNER), Uuid::nil())
        .await
        .unwrap_err();
    assert!(matches!(missing, QueryError::NotFound), "{missing:?}");
    let unread = seeded
        .service
        .get_query(viewer(OWNER), Uuid::nil())
        .await
        .unwrap_err();
    assert!(matches!(unread, QueryError::NotFound), "{unread:?}");
}
