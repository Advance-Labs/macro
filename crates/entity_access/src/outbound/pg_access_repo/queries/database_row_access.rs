//! Query for database row access level.

#[cfg(feature = "explain_binary")]
use crate::{
    domain::models::AccessGrant, outbound::pg_access_repo::queries::list_entity_access_grants,
};
use crate::{domain::models::AccessLevel, outbound::pg_access_repo::queries::SourceIds};
#[cfg(feature = "explain_binary")]
use model_entity::EntityType;
use sqlx::PgPool;

#[cfg(test)]
mod test;

/// The highest access level `source_ids` hold on a database row: a row has
/// no grants of its own, so this is its database's access.
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

    super::database_access::highest_access_level(&all_level_strings)
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
    let Some(database_id) = get_database_row_database(pool, row_id).await? else {
        return Ok(vec![]);
    };
    list_entity_access_grants(pool, &database_id, EntityType::Database, source_ids).await
}
