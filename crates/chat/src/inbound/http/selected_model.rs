//! HTTP adapter for the composer's saved model.

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{FromRef, State};
use axum::routing::get;
use macro_authorization::{
    ActingUser, MacroAuthorizationExtractor, MacroAuthorizationService, MacroAuthorizationState,
};
use roles_and_permissions::domain::port::UserRolesAndPermissionsService;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::models::Result;
use crate::domain::ports::SelectedModelRepo;
use crate::domain::service::SelectedModelService;
use crate::inbound::http::extractors::{ChatModelAccess, UserPermissionsState};

/// Router state for reading and recording the composer's saved model.
pub struct SelectedModelRouterState<R, Auth, P> {
    service: Arc<SelectedModelService<R>>,
    authorization_state: MacroAuthorizationState<Auth>,
    permissions_state: UserPermissionsState<P>,
}

impl<R, Auth, P> Clone for SelectedModelRouterState<R, Auth, P> {
    fn clone(&self) -> Self {
        Self {
            service: Arc::clone(&self.service),
            authorization_state: self.authorization_state.clone(),
            permissions_state: self.permissions_state.clone(),
        }
    }
}

impl<R, Auth, P> FromRef<SelectedModelRouterState<R, Auth, P>> for MacroAuthorizationState<Auth> {
    fn from_ref(state: &SelectedModelRouterState<R, Auth, P>) -> Self {
        state.authorization_state.clone()
    }
}

impl<R, Auth, P> FromRef<SelectedModelRouterState<R, Auth, P>> for UserPermissionsState<P> {
    fn from_ref(state: &SelectedModelRouterState<R, Auth, P>) -> Self {
        state.permissions_state.clone()
    }
}

impl<R, Auth, P> SelectedModelRouterState<R, Auth, P> {
    /// Create router state from the domain service and the auth extractors' dependencies.
    pub fn new(
        service: SelectedModelService<R>,
        authorization_state: MacroAuthorizationState<Auth>,
        permissions_service: Arc<P>,
    ) -> Self {
        Self {
            service: Arc::new(service),
            authorization_state,
            permissions_state: UserPermissionsState(permissions_service),
        }
    }
}

/// The model the composer should open on.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SelectedModelResponse {
    /// Provider-qualified model id.
    pub model_id: String,
    /// Whether the user picked `model_id`.
    pub explicit: bool,
}

/// Body for recording a composer model choice.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RememberSelectedModelRequest {
    /// Provider-qualified model id.
    pub model_id: String,
}

/// Routes for `GET` and `PUT /chats/selected-model`.
pub fn selected_model_router<
    R: SelectedModelRepo,
    Auth: MacroAuthorizationService,
    P: UserRolesAndPermissionsService,
    T: Send + Sync + 'static,
>(
    state: SelectedModelRouterState<R, Auth, P>,
) -> Router<T> {
    Router::new()
        .route(
            "/selected-model",
            get(get_selected_model_handler::<R, Auth, P>)
                .put(remember_selected_model_handler::<R, Auth, P>),
        )
        .with_state(state)
}

#[utoipa::path(
    get,
    path = "/chats/selected-model",
    tag = "chats",
    operation_id = "get_selected_model",
    responses(
        (status = 200, body = SelectedModelResponse),
        (status = 401, body = String),
        (status = 500, body = String),
    )
)]
/// The model the signed-in user's composer should open on.
#[tracing::instrument(skip(state, access, user), err(Debug))]
pub async fn get_selected_model_handler<
    R: SelectedModelRepo,
    Auth: MacroAuthorizationService,
    P: UserRolesAndPermissionsService,
>(
    access: ChatModelAccess<Auth, P>,
    user: MacroAuthorizationExtractor<Auth, ActingUser>,
    State(state): State<SelectedModelRouterState<R, Auth, P>>,
) -> Result<Json<SelectedModelResponse>> {
    let model = state
        .service
        .composer_model(
            user.authorization.user.macro_user_id.as_ref(),
            access.professional(),
        )
        .await?;
    Ok(Json(SelectedModelResponse {
        model_id: model.model_id,
        explicit: model.explicit,
    }))
}

#[utoipa::path(
    put,
    path = "/chats/selected-model",
    tag = "chats",
    operation_id = "remember_selected_model",
    request_body = RememberSelectedModelRequest,
    responses(
        (status = 200, body = SelectedModelResponse),
        (status = 400, body = String),
        (status = 401, body = String),
        (status = 500, body = String),
    )
)]
/// Record a model the signed-in user picked while other models were available.
#[tracing::instrument(skip(state, access, user), err(Debug))]
pub async fn remember_selected_model_handler<
    R: SelectedModelRepo,
    Auth: MacroAuthorizationService,
    P: UserRolesAndPermissionsService,
>(
    access: ChatModelAccess<Auth, P>,
    user: MacroAuthorizationExtractor<Auth, ActingUser>,
    State(state): State<SelectedModelRouterState<R, Auth, P>>,
    Json(body): Json<RememberSelectedModelRequest>,
) -> Result<Json<SelectedModelResponse>> {
    let model = state
        .service
        .remember(
            user.authorization.user.macro_user_id.as_ref(),
            access.professional(),
            &body.model_id,
        )
        .await?;
    Ok(Json(SelectedModelResponse {
        model_id: model.model_id,
        explicit: model.explicit,
    }))
}
