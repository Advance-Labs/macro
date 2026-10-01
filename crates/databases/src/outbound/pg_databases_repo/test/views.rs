//! Positions and views over a real Postgres: the migration that turned the
//! old counters into fractional keys, the byte order every position column
//! compares in, and views and card places through the ops as hosts build
//! the service.

use macro_event_broker::NoopMacroEventBroker;
use models_databases::position::{key_between, keys_between};
use models_databases::views::{CardPosition, Lane, NewView, ViewLayout, ViewQuery};
use models_databases::{CellValue, CellWrite, DatabaseOp, OpResult, OptionRef};
use models_properties::service::property_value::PropertyValue;
use properties::outbound::properties_pg_repo::PropertiesPgRepo;

use super::apply_ops::{Guests, cells, edit, guests, viewer};
use super::*;
use crate::domain::ports::{CellStore, ColumnDefinitionStore, DatabasesService};
use crate::outbound::build::build_service;
use crate::outbound::gateway_event_publisher::NoOpTableEventPublisher;
use crate::outbound::pg_definition_store::PgDefinitionStore;

/// The migration that rewrote the zero-padded counters.
const FRACTIONAL_POSITIONS: &str = include_str!(
    "../../../../../macro_db_client/migrations/20261001031610_fractional_positions.up.sql"
);

/// Insert one row per status into the guests table, answering their ids.
async fn insert_statuses(pool: &PgPool, guests: &Guests, statuses: &[&str]) -> Vec<Uuid> {
    let results = build_service(pool.clone(), NoOpTableEventPublisher, NoopMacroEventBroker)
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::InsertRows {
                table: guests.table_id,
                rows: statuses
                    .iter()
                    .map(|status| {
                        vec![CellWrite {
                            column: guests.status,
                            value: CellValue::Options(vec![OptionRef::Label((*status).into())]),
                        }]
                    })
                    .collect(),
                create_missing_options: true,
            }],
        )
        .await
        .unwrap();
    let [OpResult::RowsWritten { inserted, .. }] = results.as_slice() else {
        panic!("expected one insert, got {results:?}");
    };
    inserted.clone()
}

async fn row_positions(pool: &PgPool, table_id: Uuid) -> Vec<(Uuid, String)> {
    PgDatabasesRepo::new(pool.clone())
        .row_refs(table_id)
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.id, row.position))
        .collect()
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn old_counters_become_fractional_keys_in_their_order(pool: PgPool) {
    let guests = guests(&pool).await;
    let rows = insert_statuses(&pool, &guests, &["Going", "Going", "Going"]).await;
    for (row, counter) in rows
        .iter()
        .zip(["000000000003", "000000000001", "000000000002"])
    {
        sqlx::query!(
            "UPDATE database_rows SET position = $2 WHERE id = $1",
            row,
            counter
        )
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query!(
        "UPDATE database_columns SET position = CASE WHEN id = $1 THEN '000000000002' ELSE '000000000001' END WHERE table_id = $2",
        guests.name,
        guests.table_id,
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::raw_sql(FRACTIONAL_POSITIONS)
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(
        row_positions(&pool, guests.table_id).await,
        vec![
            (rows[1], "8180".to_string()),
            (rows[2], "8280".to_string()),
            (rows[0], "8380".to_string()),
        ]
    );
    let columns: Vec<(Uuid, String)> = PgDatabasesRepo::new(pool.clone())
        .columns_for_tables(&[guests.table_id])
        .await
        .unwrap()
        .into_iter()
        .map(|column| (column.id, column.position))
        .collect();
    assert_eq!(
        columns,
        vec![
            (guests.status, "8180".to_string()),
            (guests.name, "8280".to_string()),
        ]
    );
    let appended = insert_statuses(&pool, &guests, &["Going"]).await;
    assert_eq!(
        row_positions(&pool, guests.table_id).await.last(),
        Some(&(appended[0], "8480".to_string()))
    );
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn a_long_list_takes_wider_keys_in_the_same_order(pool: PgPool) {
    let guests = guests(&pool).await;
    let rows = insert_statuses(&pool, &guests, &["Going"; 130]).await;
    for (index, row) in rows.iter().enumerate() {
        sqlx::query!(
            "UPDATE database_rows SET position = $2 WHERE id = $1",
            row,
            format!("{:012}", index + 1)
        )
        .execute(&pool)
        .await
        .unwrap();
    }

    sqlx::raw_sql(FRACTIONAL_POSITIONS)
        .execute(&pool)
        .await
        .unwrap();

    let rewritten = row_positions(&pool, guests.table_id).await;
    let order: Vec<Uuid> = rewritten.iter().map(|(row, _)| *row).collect();
    assert_eq!(order, rows);
    assert_eq!(rewritten[0].1, "818180");
    assert_eq!(rewritten[126].1, "81ff80");
    assert_eq!(rewritten[127].1, "828180");
    assert_eq!(rewritten[129].1, "828380");
    for pair in rewritten.windows(2) {
        assert!(
            key_between(Some(&pair[0].1), Some(&pair[1].1)).is_ok(),
            "{} < {}",
            pair[0].1,
            pair[1].1
        );
    }
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn positions_compare_as_bytes_as_the_keys_sort(pool: PgPool) {
    let collations = sqlx::query!(
        r#"SELECT c.relname AS "table!", co.collname AS "collation!"
           FROM pg_attribute a
           JOIN pg_class c ON c.oid = a.attrelid
           JOIN pg_collation co ON co.oid = a.attcollation
           WHERE a.attname = 'position'
             AND c.relname IN ('database_tables', 'database_columns', 'database_rows',
                               'database_views', 'database_view_positions')
           ORDER BY c.relname"#
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        collations
            .iter()
            .map(|row| (row.table.as_str(), row.collation.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("database_columns", "C"),
            ("database_rows", "C"),
            ("database_tables", "C"),
            ("database_view_positions", "C"),
            ("database_views", "C"),
        ]
    );

    let guests = guests(&pool).await;
    let rows = insert_statuses(&pool, &guests, &["Going"; 6]).await;
    let mut keys = keys_between(None, None, 3).unwrap();
    keys.push(key_between(None, Some(&keys[0])).unwrap());
    keys.push(key_between(Some(&keys[1]), Some(&keys[2])).unwrap());
    keys.push(key_between(Some("ff80"), None).unwrap());
    for (row, key) in rows.iter().zip(&keys) {
        sqlx::query!(
            "UPDATE database_rows SET position = $2 WHERE id = $1",
            row,
            key
        )
        .execute(&pool)
        .await
        .unwrap();
    }

    let stored: Vec<String> = row_positions(&pool, guests.table_id)
        .await
        .into_iter()
        .map(|(_, position)| position)
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(stored, sorted);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn a_board_and_its_card_places_round_trip_and_go_with_their_rows(pool: PgPool) {
    let guests = guests(&pool).await;
    let rows = insert_statuses(&pool, &guests, &["Going", "Maybe", "Going"]).await;
    let service = build_service(pool.clone(), NoOpTableEventPublisher, NoopMacroEventBroker);
    let options = PgDefinitionStore::new(PropertiesPgRepo::new(pool.clone()))
        .definitions(&[guests.status_definition])
        .await
        .unwrap()
        .remove(0)
        .property_options;
    let (going, maybe) = (options[0].id, options[1].id);
    let results = service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::CreateView {
                table: guests.table_id,
                view: NewView {
                    name: "Stages".into(),
                    query: ViewQuery::default(),
                    layout: ViewLayout::Board {
                        group_by: guests.status,
                        lanes: vec![
                            Lane {
                                option: Some(maybe),
                                hidden: false,
                            },
                            Lane {
                                option: Some(going),
                                hidden: false,
                            },
                        ],
                        card_fields: vec![guests.name],
                        hide_empty_lanes: false,
                    },
                },
            }],
        )
        .await
        .unwrap();
    let [OpResult::ViewWritten { view: board, .. }] = results.as_slice() else {
        panic!("expected a view, got {results:?}");
    };
    let repo = PgDatabasesRepo::new(pool.clone());
    assert_eq!(
        repo.views_for_tables(&[guests.table_id]).await.unwrap(),
        vec![*board.clone()]
    );

    service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::MoveCard {
                table: guests.table_id,
                view: board.id,
                row: rows[2],
                lane: Some(maybe),
                before: Some(rows[1]),
                after: None,
            }],
        )
        .await
        .unwrap();

    assert_eq!(
        repo.view_positions(board.id).await.unwrap(),
        vec![
            CardPosition {
                row: rows[1],
                lane: Some(maybe),
                position: "7f80".into(),
            },
            CardPosition {
                row: rows[2],
                lane: Some(maybe),
                position: "80".into(),
            },
        ]
    );
    let stored = cells(&pool).cells(&rows).await.unwrap();
    assert_eq!(
        stored[&rows[2]][&guests.status_definition],
        PropertyValue::SelectOption(vec![maybe])
    );

    service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::DeleteRows {
                table: guests.table_id,
                rows: vec![rows[1]],
            }],
        )
        .await
        .unwrap();
    assert_eq!(
        repo.view_positions(board.id).await.unwrap(),
        vec![CardPosition {
            row: rows[2],
            lane: Some(maybe),
            position: "80".into(),
        }]
    );

    service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::DeleteOption {
                table: guests.table_id,
                column: guests.status,
                option: maybe,
            }],
        )
        .await
        .unwrap();
    assert_eq!(repo.view_positions(board.id).await.unwrap(), vec![]);
    assert_eq!(
        repo.views_for_tables(&[guests.table_id]).await.unwrap()[0].layout,
        ViewLayout::Board {
            group_by: guests.status,
            lanes: vec![Lane {
                option: Some(going),
                hidden: false,
            }],
            card_fields: vec![guests.name],
            hide_empty_lanes: false,
        }
    );

    service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::DeleteView {
                table: guests.table_id,
                view: board.id,
            }],
        )
        .await
        .unwrap();
    assert_eq!(
        repo.views_for_tables(&[guests.table_id]).await.unwrap(),
        vec![]
    );
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn removing_a_column_rewrites_the_views_that_named_it(pool: PgPool) {
    let guests = guests(&pool).await;
    let service = build_service(pool.clone(), NoOpTableEventPublisher, NoopMacroEventBroker);
    let results = service
        .apply_ops(
            edit(guests.database_id),
            viewer(),
            vec![DatabaseOp::CreateView {
                table: guests.table_id,
                view: NewView {
                    name: "Stages".into(),
                    query: ViewQuery::default(),
                    layout: ViewLayout::Board {
                        group_by: guests.status,
                        lanes: vec![],
                        card_fields: vec![guests.name],
                        hide_empty_lanes: false,
                    },
                },
            }],
        )
        .await
        .unwrap();
    let [OpResult::ViewWritten { view: board, .. }] = results.as_slice() else {
        panic!("expected a view, got {results:?}");
    };

    service
        .delete_column(
            edit(guests.database_id),
            guests.table_id,
            guests.name,
            version_of(&pool, guests.table_id).await,
        )
        .await
        .unwrap();

    let stored = PgDatabasesRepo::new(pool.clone())
        .views_for_tables(&[guests.table_id])
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].id, board.id);
    assert_eq!(
        stored[0].layout,
        ViewLayout::Board {
            group_by: guests.status,
            lanes: vec![],
            card_fields: vec![],
            hide_empty_lanes: false,
        }
    );
}

async fn version_of(pool: &PgPool, table_id: Uuid) -> TableVersion {
    PgDatabasesRepo::new(pool.clone())
        .table_versions(&[table_id])
        .await
        .unwrap()[&table_id]
}
