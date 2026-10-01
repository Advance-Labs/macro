use super::*;
use crate::domain::models::ColumnCast;

/// What changing one column to each type of the type menu would do to its
/// values: safe, checked (with how many values would not convert and a few
/// of them), or never (with why). Changes nothing.
#[utoipa::path(get, tag = "databases", operation_id = "list_database_column_casts",
    path = "/databases/{id}/tables/{table_id}/columns/{column_id}/casts",
    params(("id" = Uuid, Path), ("table_id" = Uuid, Path), ("column_id" = Uuid, Path)),
    responses((status = 200, body = Vec<ColumnCast>),
        (status = 401, body = ErrorResponse), (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse), (status = 500, body = ErrorResponse)))]
#[tracing::instrument(err, skip_all)]
pub async fn column_casts_handler<Service, EntityAccess, Authorization>(
    access: DatabaseAccessLevelExtractor<ViewAccessLevel, EntityAccess, Authorization>,
    State(state): State<DatabasesRouterState<Service, EntityAccess, Authorization>>,
    Path(path): Path<ColumnPath>,
) -> Result<Json<Vec<ColumnCast>>, DatabaseError>
where
    Service: DatabasesService,
    EntityAccess: EntityAccessService,
    Authorization: MacroAuthorizationService,
{
    state
        .service
        .column_casts(access.entity_access_receipt, path.table_id, path.column_id)
        .await
        .map(Json)
}
