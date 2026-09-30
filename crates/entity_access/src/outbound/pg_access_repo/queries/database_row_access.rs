//! Query for database row access level.

#[cfg(feature = "explain_binary")]
use crate::{
    domain::models::AccessGrant, outbound::pg_access_repo::queries::list_entity_access_grants,
};
use crate::{domain::models::AccessLevel, outbound::pg_access_repo::queries::SourceIds};
#[cfg(feature = "explain_binary")]
use model_entity::EntityType;
use sqlx::PgPool;
use std::str::FromStr;

#[cfg(test)]
mod test;

/// Get the highest access level a user has for a database row.
///
/// A row carries no `entity_access` rows of its own: its grants are those of
/// the database that owns its table, so this is
/// [`get_database_access`](super::database_access::get_database_access)
/// reached through `database_rows -> database_tables`.
#[tracing::instrument(err, skip(pool, source_ids))]
pub async fn get_database_row_access(
    pool: &PgPool,
    row_id: &uuid::Uuid,
    source_ids: &SourceIds,
) -> Result<Option<AccessLevel>, sqlx::Error> {
    if source_ids.0.is_empty() {
        return Ok(None);
    }

    let all_level_strings: Vec<Option<String>> = sqlx::query_scalar!(
        r#"
        SELECT ea.access_level::text
        FROM database_rows r
        JOIN database_tables t ON t.id = r.table_id
        JOIN databases d ON d.id = t.database_id
        JOIN entity_access ea ON ea.entity_id = d.id
        WHERE r.id = $1
        AND ea.entity_type = 'database'
        AND ea.source_id = ANY($2)
        "#,
        row_id,
        &source_ids.0,
    )
    .fetch_all(pool)
    .await?;

    let highest_level = all_level_strings
        .iter()
        .filter_map(|opt| opt.as_ref().and_then(|s| AccessLevel::from_str(s).ok()))
        .max();

    Ok(highest_level)
}

/// The database a row's table belongs to.
#[tracing::instrument(err, skip(pool))]
pub async fn get_database_row_database(
    pool: &PgPool,
    row_id: &uuid::Uuid,
) -> Result<Option<uuid::Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"
        SELECT t.database_id
        FROM database_rows r
        JOIN database_tables t ON t.id = r.table_id
        WHERE r.id = $1
        "#,
        row_id,
    )
    .fetch_optional(pool)
    .await
}

/// List the grants behind a database row's access: those of its database.
#[cfg(feature = "explain_binary")]
#[tracing::instrument(err, skip(pool, source_ids))]
pub async fn explain_database_row_access(
    pool: &PgPool,
    row_id: &uuid::Uuid,
    source_ids: &SourceIds,
) -> Result<Vec<AccessGrant>, sqlx::Error> {
    let database_id = sqlx::query_scalar!(
        r#"
        SELECT t.database_id
        FROM database_rows r
        JOIN database_tables t ON t.id = r.table_id
        WHERE r.id = $1
        "#,
        row_id,
    )
    .fetch_optional(pool)
    .await?;

    let Some(database_id) = database_id else {
        return Ok(vec![]);
    };
    list_entity_access_grants(pool, &database_id, EntityType::Database, source_ids).await
}
