use super::*;
use crate::domain::models::{QueryDefinition, SavedQuery};

/// Request body for saving a query.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SaveQueryRequest {
    /// What the query asks: `{"version": 1, "query": "<SELECT>"}`.
    pub definition: QueryDefinition,
    /// The database whose tables win name resolution. The caller must be
    /// able to see it.
    #[serde(default)]
    #[schema(nullable = false)]
    pub database_id: Option<Uuid>,
}

/// Path params for the saved-query routes.
#[derive(Debug, Deserialize)]
pub struct QueryPath {
    /// Saved query id.
    pub query_id: Uuid,
}

/// Save an immutable query. Editing a question saves a new one.
#[utoipa::path(
    post,
    tag = "databases",
    operation_id = "save_database_query",
    path = "/databases/queries",
    request_body = SaveQueryRequest,
    responses(
        (status = 201, body = SavedQuery),
        (status = 400, description = "The query does not compile", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "The query is not a SELECT", body = ErrorResponse),
        (status = 404, description = "The database is missing or not visible", body = ErrorResponse),
        (status = 422, description = "Query budget exceeded", body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
pub async fn save_query_handler<S, Eas, Auth>(
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Json(req): Json<SaveQueryRequest>,
) -> Result<(StatusCode, Json<SavedQuery>), QueryError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    let saved = state
        .service
        .save_query(viewer_of(&user), req.database_id, req.definition)
        .await?;
    Ok((StatusCode::CREATED, Json(saved)))
}

/// A saved query's definition, for its creator or a viewer of its database.
#[utoipa::path(
    get,
    tag = "databases",
    operation_id = "get_database_query",
    path = "/databases/queries/{query_id}",
    params(("query_id" = Uuid, Path, description = "Saved query id")),
    responses(
        (status = 200, body = SavedQuery),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 404, description = "Missing, or not readable by the caller", body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
pub async fn get_query_handler<S, Eas, Auth>(
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(QueryPath { query_id }): Path<QueryPath>,
) -> Result<Json<SavedQuery>, QueryError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    state
        .service
        .get_query(viewer_of(&user), query_id)
        .await
        .map(Json)
}

/// Run a saved query as the caller; results are permission-filtered.
#[utoipa::path(
    post,
    tag = "databases",
    operation_id = "run_database_query",
    path = "/databases/queries/{query_id}/run",
    params(("query_id" = Uuid, Path, description = "Saved query id")),
    responses(
        (status = 200, body = ExecOutcome),
        (status = 400, description = "The query no longer compiles", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 404, description = "Missing, or not readable by the caller", body = ErrorResponse),
        (status = 422, description = "Query budget exceeded", body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
pub async fn run_query_handler<S, Eas, Auth>(
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(QueryPath { query_id }): Path<QueryPath>,
) -> Result<Json<ExecOutcome>, QueryError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    state
        .service
        .run_query(viewer_of(&user), query_id)
        .await
        .map(Json)
}
