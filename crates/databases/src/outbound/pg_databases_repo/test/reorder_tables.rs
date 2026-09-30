use super::*;
use crate::domain::models::TableOrderOutcome;

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn reordering_three_tables_rewrites_every_position_and_reads_back_in_order(pool: PgPool) {
    let (repo, guests, _) = fixture(&pool).await;
    let budget = applied_table(
        repo.create_table(&CreateTable {
            database_id: guests.database_id,
            name: "Budget".into(),
        })
        .await
        .unwrap(),
    );
    let (_, before) = repo
        .get_database(guests.database_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        before.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        vec!["Table 1", "Guests", "Budget"]
    );
    let starter = before[0].clone();

    let TableOrderOutcome::Applied(reordered) = repo
        .reorder_tables(guests.database_id, &[budget.id, starter.id, guests.id])
        .await
        .unwrap()
    else {
        panic!("a complete order should apply");
    };
    assert_eq!(
        reordered
            .iter()
            .map(|t| (t.name.as_str(), t.position.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("Budget", "000000000001"),
            ("Table 1", "000000000002"),
            ("Guests", "000000000003"),
        ]
    );

    let (_, after) = repo
        .get_database(guests.database_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        after.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        vec!["Budget", "Table 1", "Guests"]
    );
    assert_eq!(after[0].version.0, budget.version.0 + 1);
    assert_eq!(after[1].version.0, starter.version.0 + 1);
    assert_eq!(after[2].version.0, before[1].version.0 + 1);

    // A table created afterwards still lands at the end.
    let notes = applied_table(
        repo.create_table(&CreateTable {
            database_id: guests.database_id,
            name: "Notes".into(),
        })
        .await
        .unwrap(),
    );
    assert_eq!(notes.position, "000000000004");
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn an_order_missing_a_table_is_a_conflict_and_changes_nothing(pool: PgPool) {
    let (repo, guests, _) = fixture(&pool).await;
    let (_, before) = repo
        .get_database(guests.database_id)
        .await
        .unwrap()
        .unwrap();

    assert!(matches!(
        repo.reorder_tables(guests.database_id, &[guests.id])
            .await
            .unwrap(),
        TableOrderOutcome::Conflict
    ));
    assert!(matches!(
        repo.reorder_tables(guests.database_id, &[guests.id, guests.id])
            .await
            .unwrap(),
        TableOrderOutcome::Conflict
    ));

    let (_, after) = repo
        .get_database(guests.database_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        after
            .iter()
            .map(|t| (t.id, t.position.as_str(), t.version))
            .collect::<Vec<_>>(),
        before
            .iter()
            .map(|t| (t.id, t.position.as_str(), t.version))
            .collect::<Vec<_>>()
    );
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn an_order_naming_another_databases_table_is_a_conflict_and_changes_neither(pool: PgPool) {
    let (repo, guests, _) = fixture(&pool).await;
    let (_, before) = repo
        .get_database(guests.database_id)
        .await
        .unwrap()
        .unwrap();
    let starter = before[0].clone();
    let other = repo
        .create_database(
            &CreateDatabase {
                name: "Hiring".into(),
                owner_id: user(),
                acting_bot: None,
            },
            "Candidates",
        )
        .await
        .unwrap();
    let (_, other_tables) = repo.get_database(other.id).await.unwrap().unwrap();
    let candidates = other_tables[0].clone();

    assert!(matches!(
        repo.reorder_tables(guests.database_id, &[guests.id, candidates.id])
            .await
            .unwrap(),
        TableOrderOutcome::Conflict
    ));
    assert!(matches!(
        repo.reorder_tables(guests.database_id, &[guests.id, starter.id, candidates.id])
            .await
            .unwrap(),
        TableOrderOutcome::Conflict
    ));

    let (_, after) = repo
        .get_database(guests.database_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        after
            .iter()
            .map(|t| (t.id, t.position.as_str(), t.version))
            .collect::<Vec<_>>(),
        before
            .iter()
            .map(|t| (t.id, t.position.as_str(), t.version))
            .collect::<Vec<_>>()
    );
    let (_, other_after) = repo.get_database(other.id).await.unwrap().unwrap();
    assert_eq!(other_after[0].position, candidates.position);
    assert_eq!(other_after[0].version, candidates.version);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn reordering_a_trashed_database_is_not_found(pool: PgPool) {
    let (repo, guests, _) = fixture(&pool).await;
    let (_, before) = repo
        .get_database(guests.database_id)
        .await
        .unwrap()
        .unwrap();
    repo.trash_database(guests.database_id, chrono::Utc::now())
        .await
        .unwrap();

    assert!(matches!(
        repo.reorder_tables(guests.database_id, &[guests.id, before[0].id])
            .await
            .unwrap(),
        TableOrderOutcome::NotFound
    ));
}
