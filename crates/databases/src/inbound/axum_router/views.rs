use crate::domain::models::{CardPosition, ViewId};

use super::*;

/// Where a board's cards sit.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ViewPositionsResponse {
    /// The places of the cards that have one: each card's lane and its key
    /// there. Cards without a place show after the placed ones of their
    /// lane, oldest first.
    pub positions: Vec<CardPosition>,
}

/// Where a board's cards sit, for drawing it: the views come with the
/// database's detail, their cards' places from here.
#[utoipa::path(
    get,
    tag = "databases",
    operation_id = "get_database_view_positions",
    path = "/databases/{id}/views/{view_id}/positions",
    params(("id" = Uuid, Path, description = "Database id"), ("view_id" = Uuid, Path, description = "View id")),
    responses(
        (status = 200, body = ViewPositionsResponse),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "No access to the database", body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn view_positions_handler<S, Eas, Auth>(
    access: DatabaseAccessLevelExtractor<ViewAccessLevel, Eas, Auth>,
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    Path((_database, view_id)): Path<(Uuid, ViewId)>,
) -> Result<Json<ViewPositionsResponse>, DatabaseError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    let positions = state
        .service
        .view_positions(access.entity_access_receipt, view_id)
        .await?;
    Ok(Json(ViewPositionsResponse { positions }))
}
