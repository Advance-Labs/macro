//! Query for database access level.

use crate::{domain::models::AccessLevel, outbound::pg_access_repo::queries::SourceIds};
use sqlx::PgPool;
use std::collections::BTreeMap;
use std::str::FromStr;
use uuid::Uuid;

#[cfg(test)]
mod test;

/// Get the highest access level a user has for a database.
///
/// A database's grants are written when it is created - the creator as owner
/// - and extended when it is shared with other users or teams. Those grants
/// reach a caller through `source_ids`, so gaining one of a grant's source
/// ids (joining the granted team, say) gives the database on the next
/// request.
///
/// Unlike documents there is no public-sharing arm: a database carries no
/// `SharePermission`, so a caller with no source ids has no way to reach one.
#[tracing::instrument(err, skip(pool, source_ids))]
pub async fn get_database_access(
    pool: &PgPool,
    database_id: &uuid::Uuid,
    source_ids: &SourceIds,
) -> Result<Option<AccessLevel>, sqlx::Error> {
    if source_ids.0.is_empty() {
        return Ok(None);
    }

    let all_level_strings: Vec<Option<String>> = sqlx::query_scalar!(
        r#"
        SELECT access_level::text
        FROM entity_access
        WHERE entity_id = $1
        AND entity_type = 'database'
        AND source_id = ANY($2)
        "#,
        database_id,
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

/// The highest access level a user has on every database their source ids
/// reach, ordered by database id. Trash is the databases domain's concern:
/// a trashed database's grants are still listed.
#[tracing::instrument(err, skip(pool, source_ids))]
pub async fn list_database_access(
    pool: &PgPool,
    source_ids: &SourceIds,
) -> Result<Vec<(Uuid, AccessLevel)>, sqlx::Error> {
    if source_ids.0.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query!(
        r#"
        SELECT entity_id AS "entity_id!", access_level AS "access_level!: AccessLevel"
        FROM entity_access
        WHERE entity_type = 'database'
        AND source_id = ANY($1)
        "#,
        &source_ids.0,
    )
    .fetch_all(pool)
    .await?;

    let mut highest: BTreeMap<Uuid, AccessLevel> = BTreeMap::new();
    for row in rows {
        highest
            .entry(row.entity_id)
            .and_modify(|level| *level = (*level).max(row.access_level))
            .or_insert(row.access_level);
    }
    Ok(highest.into_iter().collect())
}
