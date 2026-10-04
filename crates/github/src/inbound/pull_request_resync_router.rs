//! Explicit internal repair, never run by installation setup or webhook sync.

use crate::domain::{
    models::{GithubError, PullRequestResyncPage, PullRequestResyncRequest},
    ports::GithubPullRequestResync,
};
use axum::{
    Json, Router,
    extract::{FromRef, State},
    routing::post,
};
use macro_authorization::{
    InternalOnly, MacroAuthorizationExtractor, MacroAuthorizationService, MacroAuthorizationState,
};
use std::sync::Arc;

/// State for the internal repair route.
pub struct PullRequestResyncRouterState<S, Auth> {
    /// Repair domain service.
    pub service: Arc<S>,
    /// Internal caller authentication.
    pub authorization_state: MacroAuthorizationState<Auth>,
}

impl<S, Auth> Clone for PullRequestResyncRouterState<S, Auth> {
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            authorization_state: self.authorization_state.clone(),
        }
    }
}

impl<S, Auth> FromRef<PullRequestResyncRouterState<S, Auth>> for MacroAuthorizationState<Auth> {
    fn from_ref(state: &PullRequestResyncRouterState<S, Auth>) -> Self {
        state.authorization_state.clone()
    }
}

/// POST /resync-pull-requests under the owning service's /internal/github prefix.
pub fn pull_request_resync_router<S, Auth, T>(
    state: PullRequestResyncRouterState<S, Auth>,
) -> Router<T>
where
    S: GithubPullRequestResync,
    Auth: MacroAuthorizationService,
    T: Send + Sync + 'static,
{
    Router::new()
        .route(
            "/resync-pull-requests",
            post(resync_pull_requests_handler::<S, Auth>),
        )
        .with_state(state)
}

/// Authenticate an internal caller, then delegate repair policy to the domain service.
#[tracing::instrument(skip_all, err)]
pub async fn resync_pull_requests_handler<S, Auth>(
    State(state): State<PullRequestResyncRouterState<S, Auth>>,
    _internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Json(request): Json<PullRequestResyncRequest>,
) -> Result<Json<PullRequestResyncPage>, GithubError>
where
    S: GithubPullRequestResync,
    Auth: MacroAuthorizationService,
{
    Ok(Json(state.service.resync_pull_requests(request).await?))
}
