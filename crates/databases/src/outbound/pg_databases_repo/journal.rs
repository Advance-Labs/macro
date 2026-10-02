//! The change journal's statements: recording a batch's entries inside its
//! transaction, reading what its before-image needs under its locks, and
//! reading history back.

use std::collections::HashMap;

use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use super::{PgDatabasesRepoError, uuids};
use crate::domain::journal::{
    ChangeInverse, JournalActor, JournalEntry, JournaledRowChange, StoredChange,
};
use crate::domain::models::{
    ChangeId, ColumnId, CommittedChange, DatabaseId, RowId, TableId, TableVersion,
};
use models_databases::position::Position;

/// Record a batch's journal entries, answering each one's id.
pub(crate) async fn record(
    connection: &mut PgConnection,
    actor: &JournalActor,
    entries: &[JournalEntry],
) -> Result<Vec<CommittedChange>, PgDatabasesRepoError> {
    let acting_bot = actor.acting_bot.as_ref().map(ToString::to_string);
    let mut committed = Vec::with_capacity(entries.len());
    for entry in entries {
        let id = sqlx::query_scalar!(
            r#"INSERT INTO database_changes (database_id, table_id, version, actor, acting_bot, ops, inverse)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               RETURNING id"#,
            entry.database_id.into_uuid(),
            entry.table.into_uuid(),
            entry.version.0,
            actor.user,
            acting_bot,
            serde_json::to_value(&entry.ops)?,
            serde_json::to_value(&entry.inverse)?,
        )
        .fetch_one(&mut *connection)
        .await?;
        if !entry.rows.is_empty() {
            let rows: Vec<Uuid> = entry
                .rows
                .iter()
                .map(|touch| touch.row.into_uuid())
                .collect();
            let kinds: Vec<String> = entry
                .rows
                .iter()
                .map(|touch| <&str>::from(touch.kind).to_string())
                .collect();
            // Postgres has no array of arrays of varying length, so each
            // row's columns travel as one text of comma-separated ids.
            let columns: Vec<String> = entry
                .rows
                .iter()
                .map(|touch| {
                    touch
                        .columns
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .collect();
            sqlx::query!(
                r#"INSERT INTO database_change_rows (change_id, row_id, kind, columns)
                   SELECT $1, row_id, kind,
                          COALESCE(string_to_array(NULLIF(columns, ''), ',')::uuid[], '{}')
                   FROM UNNEST($2::uuid[], $3::text[], $4::text[]) AS data(row_id, kind, columns)"#,
                id,
                &rows,
                &kinds,
                &columns,
            )
            .execute(&mut *connection)
            .await?;
        }
        if !entry.columns.is_empty() {
            let columns: Vec<Uuid> = entry
                .columns
                .iter()
                .map(|touch| touch.column.into_uuid())
                .collect();
            let kinds: Vec<String> = entry
                .columns
                .iter()
                .map(|touch| <&str>::from(touch.kind).to_string())
                .collect();
            sqlx::query!(
                r#"INSERT INTO database_change_columns (change_id, column_id, kind)
                   SELECT $1, column_id, kind
                   FROM UNNEST($2::uuid[], $3::text[]) AS data(column_id, kind)"#,
                id,
                &columns,
                &kinds,
            )
            .execute(&mut *connection)
            .await?;
        }
        committed.push(CommittedChange {
            table: entry.table,
            version: entry.version,
            change: ChangeId(id),
        });
    }
    Ok(committed)
}

/// The rows among `rows` that exist, with their table and position.
pub(crate) async fn row_places(
    executor: impl PgExecutor<'_>,
    rows: &[RowId],
) -> Result<Vec<(RowId, TableId, Position)>, PgDatabasesRepoError> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let records = sqlx::query!(
        "SELECT id, table_id, position FROM database_rows WHERE id = ANY($1)",
        &uuids(rows),
    )
    .fetch_all(executor)
    .await?;
    records
        .into_iter()
        .map(|record| {
            Ok((
                RowId::from_uuid(record.id),
                TableId::from_uuid(record.table_id),
                record.position.parse()?,
            ))
        })
        .collect()
}

/// Every row of a table.
pub(crate) async fn table_rows(
    executor: impl PgExecutor<'_>,
    table: TableId,
) -> Result<Vec<RowId>, sqlx::Error> {
    let rows = sqlx::query_scalar!(
        "SELECT id FROM database_rows WHERE table_id = $1",
        table.into_uuid(),
    )
    .fetch_all(executor)
    .await?;
    Ok(rows.into_iter().map(RowId::from_uuid).collect())
}

/// Stamp who last wrote these rows' cells, and when.
pub(crate) async fn stamp_rows(
    executor: impl PgExecutor<'_>,
    rows: &[RowId],
    updated_by: &str,
) -> Result<(), sqlx::Error> {
    if rows.is_empty() {
        return Ok(());
    }
    sqlx::query!(
        "UPDATE database_rows SET updated_by = $2, updated_at = now() WHERE id = ANY($1)",
        &uuids(rows),
        updated_by,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Forget a purged database's history.
pub(crate) async fn purge(
    executor: impl PgExecutor<'_>,
    database_id: DatabaseId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM database_changes WHERE database_id = $1",
        database_id.into_uuid(),
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// A row's changes in one table of a database, newest first.
pub(crate) async fn row_history(
    executor: impl PgExecutor<'_>,
    database_id: DatabaseId,
    table: TableId,
    row: RowId,
) -> Result<Vec<JournaledRowChange>, PgDatabasesRepoError> {
    let records = sqlx::query!(
        r#"SELECT c.id, c.table_id, c.version, c.actor, c.acting_bot, c.at, c.ops, c.inverse,
                  r.kind, r.columns
           FROM database_change_rows r
           JOIN database_changes c ON c.id = r.change_id
           WHERE r.row_id = $1 AND c.table_id = $2 AND c.database_id = $3
           ORDER BY r.change_id DESC"#,
        row.into_uuid(),
        table.into_uuid(),
        database_id.into_uuid(),
    )
    .fetch_all(executor)
    .await?;
    records
        .into_iter()
        .map(|record| {
            Ok(JournaledRowChange {
                change: StoredChange {
                    id: ChangeId(record.id),
                    table: TableId::from_uuid(record.table_id),
                    version: TableVersion(record.version),
                    actor: record.actor,
                    acting_bot: record.acting_bot,
                    at: record.at,
                    ops: serde_json::from_value(record.ops)?,
                    inverse: serde_json::from_value::<ChangeInverse>(record.inverse)?,
                },
                kind: record
                    .kind
                    .parse::<crate::domain::journal::RowChangeKind>()
                    .map_err(|_| PgDatabasesRepoError::CorruptChangeKind(record.kind.clone()))?,
                columns: record
                    .columns
                    .into_iter()
                    .map(ColumnId::from_uuid)
                    .collect(),
            })
        })
        .collect()
}

/// The cells of `rows` the properties side keyed by row entity id, as the
/// before-image takes them.
pub(crate) fn entity_ids(rows: &[RowId]) -> Vec<String> {
    rows.iter().map(ToString::to_string).collect()
}

/// Group `(entity id, definition, value)` triples by row.
pub(crate) fn by_row<Value>(
    triples: Vec<(String, Uuid, Value)>,
) -> HashMap<RowId, HashMap<Uuid, Value>> {
    let mut rows: HashMap<RowId, HashMap<Uuid, Value>> = HashMap::new();
    for (entity, definition, value) in triples {
        if let Ok(row) = entity.parse() {
            rows.entry(row).or_default().insert(definition, value);
        }
    }
    rows
}
