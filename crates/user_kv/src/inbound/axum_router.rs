//! Axum router for the per-user key-value endpoints.

#[cfg(test)]
mod test;

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use macro_authorization::{
    MacroAuthorizationExtractor, MacroAuthorizationService, MacroAuthorizationState, UserOrInternal,
};
use model_error_response::ErrorResponse;
use serde::{Deserialize, Serialize};

use crate::domain::{
    models::{KvKey, KvNamespace, KvValue, UserKvEntry, UserKvError},
    ports::UserKvService,
};

/// Router state for the key-value endpoints.
pub struct UserKvRouterState<S, Auth> {
    service: Arc<S>,
    authorization_state: MacroAuthorizationState<Auth>,
}

impl<S, Auth> Clone for UserKvRouterState<S, Auth> {
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            authorization_state: self.authorization_state.clone(),
        }
    }
}

impl<S, Auth> UserKvRouterState<S, Auth>
where
    S: UserKvService,
{
    /// Create router state from a shared service and authorization state.
    pub fn new(service: Arc<S>, authorization_state: MacroAuthorizationState<Auth>) -> Self {
        Self {
            service,
            authorization_state,
        }
    }
}

impl<S, Auth> FromRef<UserKvRouterState<S, Auth>> for MacroAuthorizationState<Auth> {
    fn from_ref(state: &UserKvRouterState<S, Auth>) -> Self {
        state.authorization_state.clone()
    }
}

/// Build the key-value router. Every route acts on the caller's own entries.
///
/// Routes:
/// - `GET /{namespace}` — list the caller's entries in a namespace.
/// - `GET /{namespace}/{key}` — fetch one entry.
/// - `PUT /{namespace}/{key}` — create the entry or replace its value.
/// - `DELETE /{namespace}/{key}` — remove one entry.
pub fn user_kv_router<S, Auth, T>(state: UserKvRouterState<S, Auth>) -> Router<T>
where
    S: UserKvService,
    Auth: MacroAuthorizationService,
    T: Send + Sync + 'static,
{
    Router::new()
        .route("/{namespace}", get(list_user_kv_handler::<S, Auth>))
        .route(
            "/{namespace}/{key}",
            get(get_user_kv_handler::<S, Auth>)
                .put(put_user_kv_handler::<S, Auth>)
                .delete(delete_user_kv_handler::<S, Auth>),
        )
        .with_state(state)
}

/// Path params naming a namespace.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Path)]
pub struct UserKvNamespaceParams {
    /// Lowercase slug grouping one use case, e.g. `tours`.
    pub namespace: String,
}

/// Path params naming one entry.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Path)]
pub struct UserKvEntryParams {
    /// Lowercase slug grouping one use case, e.g. `tours`.
    pub namespace: String,
    /// Lowercase slug naming the entry, e.g. `calendar`.
    pub key: String,
}

/// The caller's entries in one namespace.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserKvEntriesList {
    /// Entries ordered by key.
    pub entries: Vec<UserKvEntry>,
}

/// Request body for writing an entry.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PutUserKvRequest {
    /// The JSON object to store. Replaces any existing value.
    #[schema(value_type = Object)]
    pub value: KvValue,
}

fn parse_entry(params: UserKvEntryParams) -> Result<(KvNamespace, KvKey), UserKvError> {
    Ok((
        KvNamespace::parse(params.namespace)?,
        KvKey::parse(params.key)?,
    ))
}

/// List the caller's entries in a namespace.
#[utoipa::path(
    get,
    tag = "user-kv",
    operation_id = "list_user_kv",
    path = "/user-kv/{namespace}",
    params(UserKvNamespaceParams),
    responses(
        (status = 200, body = UserKvEntriesList),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn list_user_kv_handler<S, Auth>(
    State(state): State<UserKvRouterState<S, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(params): Path<UserKvNamespaceParams>,
) -> Result<Json<UserKvEntriesList>, UserKvError>
where
    S: UserKvService,
    Auth: MacroAuthorizationService,
{
    let namespace = KvNamespace::parse(params.namespace)?;
    let entries = state
        .service
        .list_entries(&user.authorization.user.macro_user_id, &namespace)
        .await?;
    Ok(Json(UserKvEntriesList { entries }))
}

/// Fetch one of the caller's entries.
#[utoipa::path(
    get,
    tag = "user-kv",
    operation_id = "get_user_kv",
    path = "/user-kv/{namespace}/{key}",
    params(UserKvEntryParams),
    responses(
        (status = 200, body = UserKvEntry),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn get_user_kv_handler<S, Auth>(
    State(state): State<UserKvRouterState<S, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(params): Path<UserKvEntryParams>,
) -> Result<Json<UserKvEntry>, UserKvError>
where
    S: UserKvService,
    Auth: MacroAuthorizationService,
{
    let (namespace, key) = parse_entry(params)?;
    let entry = state
        .service
        .get_entry(&user.authorization.user.macro_user_id, &namespace, &key)
        .await?;
    Ok(Json(entry))
}

/// Create one of the caller's entries or replace its value.
#[utoipa::path(
    put,
    tag = "user-kv",
    operation_id = "put_user_kv",
    path = "/user-kv/{namespace}/{key}",
    params(UserKvEntryParams),
    request_body = PutUserKvRequest,
    responses(
        (status = 200, body = UserKvEntry),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 413, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn put_user_kv_handler<S, Auth>(
    State(state): State<UserKvRouterState<S, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(params): Path<UserKvEntryParams>,
    Json(body): Json<PutUserKvRequest>,
) -> Result<Json<UserKvEntry>, UserKvError>
where
    S: UserKvService,
    Auth: MacroAuthorizationService,
{
    let (namespace, key) = parse_entry(params)?;
    let entry = state
        .service
        .put_entry(
            &user.authorization.user.macro_user_id,
            &namespace,
            &key,
            body.value,
        )
        .await?;
    Ok(Json(entry))
}

/// Remove one of the caller's entries.
#[utoipa::path(
    delete,
    tag = "user-kv",
    operation_id = "delete_user_kv",
    path = "/user-kv/{namespace}/{key}",
    params(UserKvEntryParams),
    responses(
        (status = 204, description = "Entry deleted"),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn delete_user_kv_handler<S, Auth>(
    State(state): State<UserKvRouterState<S, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(params): Path<UserKvEntryParams>,
) -> Result<StatusCode, UserKvError>
where
    S: UserKvService,
    Auth: MacroAuthorizationService,
{
    let (namespace, key) = parse_entry(params)?;
    state
        .service
        .delete_entry(&user.authorization.user.macro_user_id, &namespace, &key)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

impl IntoResponse for UserKvError {
    fn into_response(self) -> axum::response::Response {
        let status_code = match &self {
            UserKvError::NotFound => StatusCode::NOT_FOUND,
            UserKvError::BadRequest(_) | UserKvError::EntryLimitReached { .. } => {
                StatusCode::BAD_REQUEST
            }
            UserKvError::ValueTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            UserKvError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let message = match &self {
            UserKvError::Internal(_) => {
                tracing::error!(error=?self, "user kv internal server error");
                "internal server error".to_string()
            }
            error => error.to_string(),
        };

        (
            status_code,
            Json(ErrorResponse {
                message: message.into(),
            }),
        )
            .into_response()
    }
}
