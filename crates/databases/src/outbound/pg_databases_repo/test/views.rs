//! Positions and views over a real Postgres: the byte order every position
//! column compares in, and views and card places through the ops as hosts
//! build the service.

use models_databases::position::{key_between, keys_between};
use models_databases::views::{CardPosition, Lane, NewView, ViewLayout, ViewQuery};
use models_databases::{CellValue, CellWrite, DatabaseOp, OpResult, OptionRef};
use models_properties::service::property_value::PropertyValue;
use properties::outbound::properties_pg_repo::PropertiesPgRepo;

use super::apply_ops::{Guests, cells, edit, guests, service, viewer};
use super::*;
use crate::domain::ports::{CellStore, ColumnDefinitionStore, DatabasesService};
use crate::outbound::pg_definition_store::PgDefinitionStore;

/// Insert one row per status into the guests table, answering their ids.
async fn insert_statuses(pool: &PgPool, guests: &Guests, statuses: &[&str]) -> Vec<Uuid> {
    let results = service(pool)
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
    let service = service(&pool);
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
    let service = service(&pool);
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
