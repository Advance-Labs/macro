//! View and card-place statements over any connection. A card's lane is
//! stored as its option's id, or as the empty string for cards without one.

use sqlx::PgExecutor;
use uuid::Uuid;

use super::{PgDatabasesRepoError, uuids};
use crate::domain::models::{
    CardPosition, DatabaseId, DatabaseView, OptionId, RowId, TableId, ViewId, ViewPosition,
};

/// The lane as stored.
fn lane_key(lane: Option<OptionId>) -> String {
    lane.map(|option| option.to_string()).unwrap_or_default()
}

/// The lane a stored key names; `None` for the lane of cards without an
/// option.
fn lane_of(key: &str) -> Result<Option<OptionId>, PgDatabasesRepoError> {
    if key.is_empty() {
        return Ok(None);
    }
    key.parse()
        .map(Some)
        .map_err(|_| PgDatabasesRepoError::CorruptLane(key.to_string()))
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
        &uuids(table_ids),
    )
    .fetch_all(executor)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(DatabaseView {
                id: ViewId::from_uuid(row.id),
                database_id: DatabaseId::from_uuid(row.database_id),
                table_id: TableId::from_uuid(row.table_id),
                name: row.name,
                position: row.position.parse()?,
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
) -> Result<Vec<CardPosition>, PgDatabasesRepoError> {
    let rows = sqlx::query!(
        r#"
        SELECT row_id, lane, position
        FROM database_view_positions
        WHERE view_id = $1
        ORDER BY lane, position, row_id
        "#,
        view_id.into_uuid(),
    )
    .fetch_all(executor)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(CardPosition {
                row: RowId::from_uuid(row.row_id),
                lane: lane_of(&row.lane)?,
                position: row.position.parse()?,
            })
        })
        .collect()
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
        view.id.into_uuid(),
        view.database_id.into_uuid(),
        view.table_id.into_uuid(),
        view.name,
        view.position.as_str(),
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
        view.id.into_uuid(),
        view.table_id.into_uuid(),
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
        view_id.into_uuid(),
        table_id.into_uuid(),
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
    let views: Vec<Uuid> = positions
        .iter()
        .map(|placed| placed.view.into_uuid())
        .collect();
    let keys: Vec<String> = positions
        .iter()
        .map(|placed| placed.position.to_string())
        .collect();
    let updated = sqlx::query!(
        r#"
        UPDATE database_views target
        SET position = ordered.position
        FROM UNNEST($2::uuid[], $3::text[]) AS ordered(id, position)
        WHERE target.id = ordered.id AND target.table_id = $1
        "#,
        table_id.into_uuid(),
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
        view_id.into_uuid()
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
    let rows: Vec<Uuid> = positions
        .iter()
        .map(|placed| placed.row.into_uuid())
        .collect();
    let lanes: Vec<String> = positions
        .iter()
        .map(|placed| lane_key(placed.lane))
        .collect();
    let keys: Vec<String> = positions
        .iter()
        .map(|placed| placed.position.to_string())
        .collect();
    sqlx::query!(
        r#"
        INSERT INTO database_view_positions (view_id, row_id, lane, position)
        SELECT $1, row_id, lane, position
        FROM UNNEST($2::uuid[], $3::text[], $4::text[]) AS placed(row_id, lane, position)
        ON CONFLICT (view_id, row_id)
        DO UPDATE SET lane = EXCLUDED.lane, position = EXCLUDED.position
        "#,
        view_id.into_uuid(),
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
    option: OptionId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        DELETE FROM database_view_positions placed
        USING database_views board
        WHERE placed.view_id = board.id AND board.table_id = ANY($1) AND placed.lane = $2
        "#,
        &uuids(table_ids),
        lane_key(Some(option)),
    )
    .execute(executor)
    .await?;
    Ok(())
}
