//! Statements on views and the places of their cards, over any connection:
//! the repository reads them on the pool, the cell store writes them inside
//! a batch's transaction. A card's lane is stored as its option's id, or as
//! the empty string for the lane of cards without one.

use sqlx::PgExecutor;
use uuid::Uuid;

use super::PgDatabasesRepoError;
use crate::domain::models::{CardPosition, DatabaseView, TableId, ViewId, ViewPosition};

/// The lane as stored.
fn lane_key(lane: Option<Uuid>) -> String {
    lane.map(|option| option.to_string()).unwrap_or_default()
}

/// The lane a stored key names; `None` for the lane of cards without an
/// option, and for a key that names nothing.
fn lane_of(key: &str) -> Option<Uuid> {
    Uuid::parse_str(key).ok()
}

/// Every view of the given tables, ordered by table then position.
pub(crate) async fn views_for_tables(
    executor: impl PgExecutor<'_>,
    table_ids: &[TableId],
) -> Result<Vec<DatabaseView>, PgDatabasesRepoError> {
    let rows = sqlx::query!(
        r#"
        SELECT id, database_id, table_id, name, position, query, layout, created_at, updated_at
        FROM database_views
        WHERE table_id = ANY($1)
        ORDER BY table_id, position, id
        "#,
        table_ids,
    )
    .fetch_all(executor)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(DatabaseView {
                id: row.id,
                database_id: row.database_id,
                table_id: row.table_id,
                name: row.name,
                position: row.position,
                query: serde_json::from_value(row.query)?,
                layout: serde_json::from_value(row.layout)?,
                created_at: row.created_at,
                updated_at: row.updated_at,
            })
        })
        .collect()
}

/// Where a board's cards sit, those that have a place, by lane then key.
pub(crate) async fn view_positions(
    executor: impl PgExecutor<'_>,
    view_id: ViewId,
) -> Result<Vec<CardPosition>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT row_id, lane, position
        FROM database_view_positions
        WHERE view_id = $1
        ORDER BY lane, position, row_id
        "#,
        view_id,
    )
    .fetch_all(executor)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| CardPosition {
            row: row.row_id,
            lane: lane_of(&row.lane),
            position: row.position,
        })
        .collect())
}

/// Store a new view.
pub(crate) async fn insert_view(
    executor: impl PgExecutor<'_>,
    view: &DatabaseView,
) -> Result<(), PgDatabasesRepoError> {
    sqlx::query!(
        r#"
        INSERT INTO database_views
            (id, database_id, table_id, name, position, query, layout, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
        view.id,
        view.database_id,
        view.table_id,
        view.name,
        view.position,
        serde_json::to_value(&view.query)?,
        serde_json::to_value(&view.layout)?,
        view.created_at,
        view.updated_at,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Replace a view's name, query and layout; `false` when the table no
/// longer has it.
pub(crate) async fn update_view(
    executor: impl PgExecutor<'_>,
    view: &DatabaseView,
) -> Result<bool, PgDatabasesRepoError> {
    let updated = sqlx::query!(
        r#"
        UPDATE database_views
        SET name = $3, query = $4, layout = $5, updated_at = $6
        WHERE id = $1 AND table_id = $2
        "#,
        view.id,
        view.table_id,
        view.name,
        serde_json::to_value(&view.query)?,
        serde_json::to_value(&view.layout)?,
        view.updated_at,
    )
    .execute(executor)
    .await?;
    Ok(updated.rows_affected() == 1)
}

/// Remove a view, its card places with it; `false` when the table no longer
/// has it.
pub(crate) async fn delete_view(
    executor: impl PgExecutor<'_>,
    table_id: TableId,
    view_id: ViewId,
) -> Result<bool, sqlx::Error> {
    let deleted = sqlx::query!(
        "DELETE FROM database_views WHERE id = $1 AND table_id = $2",
        view_id,
        table_id,
    )
    .execute(executor)
    .await?;
    Ok(deleted.rows_affected() == 1)
}

/// Give a table's views new positions; `false` when one of them is not the
/// table's any more.
pub(crate) async fn order_views(
    executor: impl PgExecutor<'_>,
    table_id: TableId,
    positions: &[ViewPosition],
) -> Result<bool, sqlx::Error> {
    let views: Vec<Uuid> = positions.iter().map(|placed| placed.view).collect();
    let keys: Vec<String> = positions
        .iter()
        .map(|placed| placed.position.clone())
        .collect();
    let updated = sqlx::query!(
        r#"
        UPDATE database_views target
        SET position = ordered.position
        FROM UNNEST($2::uuid[], $3::text[]) AS ordered(id, position)
        WHERE target.id = ordered.id AND target.table_id = $1
        "#,
        table_id,
        &views,
        &keys,
    )
    .execute(executor)
    .await?;
    Ok(updated.rows_affected() == positions.len() as u64)
}

/// Forget where a board's cards were.
pub(crate) async fn clear_positions(
    executor: impl PgExecutor<'_>,
    view_id: ViewId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM database_view_positions WHERE view_id = $1",
        view_id
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Store cards' places on a board, each replacing the card's earlier one.
pub(crate) async fn place_cards(
    executor: impl PgExecutor<'_>,
    view_id: ViewId,
    positions: &[CardPosition],
) -> Result<(), sqlx::Error> {
    let rows: Vec<Uuid> = positions.iter().map(|placed| placed.row).collect();
    let lanes: Vec<String> = positions
        .iter()
        .map(|placed| lane_key(placed.lane))
        .collect();
    let keys: Vec<String> = positions
        .iter()
        .map(|placed| placed.position.clone())
        .collect();
    sqlx::query!(
        r#"
        INSERT INTO database_view_positions (view_id, row_id, lane, position)
        SELECT $1, row_id, lane, position
        FROM UNNEST($2::uuid[], $3::text[], $4::text[]) AS placed(row_id, lane, position)
        ON CONFLICT (view_id, row_id)
        DO UPDATE SET lane = EXCLUDED.lane, position = EXCLUDED.position
        "#,
        view_id,
        &rows,
        &lanes,
        &keys,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Forget the places cards had in the lane of an option that is gone, on
/// every board of the given tables.
pub(crate) async fn clear_lane(
    executor: impl PgExecutor<'_>,
    table_ids: &[TableId],
    option: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        DELETE FROM database_view_positions placed
        USING database_views board
        WHERE placed.view_id = board.id AND board.table_id = ANY($1) AND placed.lane = $2
        "#,
        table_ids,
        lane_key(Some(option)),
    )
    .execute(executor)
    .await?;
    Ok(())
}
