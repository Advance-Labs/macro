use crate::domain::journal::RowHistoryEntry;
use crate::domain::models::RowId;

use super::*;

/// A row's history.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RowHistoryResponse {
    /// Every committed change that touched the row, newest first: who made
    /// it, when, how, and the touched columns' values before and after.
    pub changes: Vec<RowHistoryEntry>,
}

/// Path params for a row's routes.
#[derive(Debug, Deserialize)]
pub struct RowPath {
    /// Database id.
    pub id: DatabaseId,
    /// Table id.
    pub table_id: TableId,
    /// Row id.
    pub row_id: RowId,
}

/// A row's history, from the change journal: every committed change that
/// touched it, newest first, with who made it, when, and the values of the
/// columns it touched before and after. It reads after the row is removed,
/// so a removed row's last values stay readable.
#[utoipa::path(
    get,
    tag = "databases",
    operation_id = "get_database_row_history",
    path = "/databases/{id}/tables/{table_id}/rows/{row_id}/history",
    params(
        ("id" = Uuid, Path, description = "Database id"),
        ("table_id" = Uuid, Path, description = "Table id"),
        ("row_id" = Uuid, Path, description = "Row id"),
    ),
    responses(
        (status = 200, body = RowHistoryResponse),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "No access to the database", body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn row_history_handler<Service, EntityAccess, Authorization>(
    access: DatabaseAccessLevelExtractor<ViewAccessLevel, EntityAccess, Authorization>,
    State(state): State<DatabasesRouterState<Service, EntityAccess, Authorization>>,
    Path(path): Path<RowPath>,
) -> Result<Json<RowHistoryResponse>, DatabaseError>
where
    Service: DatabasesService,
    EntityAccess: EntityAccessService,
    Authorization: MacroAuthorizationService,
{
    let changes = state
        .service
        .row_history(access.entity_access_receipt, path.table_id, path.row_id)
        .await?;
    Ok(Json(RowHistoryResponse { changes }))
}
