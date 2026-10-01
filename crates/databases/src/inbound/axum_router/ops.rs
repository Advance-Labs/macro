use models_databases::{DatabaseOp, OpResult};

use crate::domain::models::OpRefusal;
use serde::Serialize;

use super::*;

/// A batch of ops for one database, applied together or not at all.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOpsRequest {
    /// The ops, in the order they apply. Every one names a table of this
    /// database; a column type change is sent on its own.
    pub ops: Vec<DatabaseOp>,
}

/// What each op of a batch did.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOpsResponse {
    /// One result per op, in the order the ops were sent.
    pub results: Vec<OpResult>,
}

/// Why an op of a batch was refused. Nothing in the batch was written.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpRefusalResponse {
    /// What is wrong.
    pub message: String,
    /// The refused op's index in the request.
    pub op: usize,
    /// The row's index within the op, when one row is at fault.
    #[schema(required = true)]
    pub row: Option<usize>,
    /// The column placement at fault, when one is.
    #[schema(required = true, value_type = Option<Uuid>)]
    pub column: Option<ColumnId>,
}

impl From<OpRefusal> for OpRefusalResponse {
    fn from(refusal: OpRefusal) -> Self {
        Self {
            message: refusal.reason,
            op: refusal.op,
            row: refusal.row,
            column: refusal.column,
        }
    }
}

/// Apply a batch of typed ops: insert, update and delete rows, or change a
/// column's type. Row ops are last-write-wins. A refused op, named by its
/// index (and row and column where relevant), leaves the whole batch
/// unwritten.
#[utoipa::path(
    post,
    tag = "databases",
    operation_id = "apply_database_ops",
    path = "/databases/{id}/ops",
    params(("id" = Uuid, Path, description = "Database id")),
    request_body = ApplyOpsRequest,
    responses(
        (status = 200, body = ApplyOpsResponse),
        (status = 400, description = "An op was refused; nothing was written", body = OpRefusalResponse),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "No edit access to the database", body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, description = "A column type change raced another schema change", body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn apply_ops_handler<S, Eas, Auth>(
    access: DatabaseAccessLevelExtractor<EditAccessLevel, Eas, Auth>,
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Json(request): Json<ApplyOpsRequest>,
) -> Result<Json<ApplyOpsResponse>, DatabaseError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    let results = state
        .service
        .apply_ops(access.entity_access_receipt, viewer_of(&user), request.ops)
        .await?;
    Ok(Json(ApplyOpsResponse { results }))
}
