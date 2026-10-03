use crate::domain::{
    models::*,
    ports::{Documents, Invitations, Repository},
    service::Service,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, FromRef, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine, prelude::BASE64_STANDARD};
use macro_authorization::{
    MacroAuthorizationExtractor, MacroAuthorizationService, MacroAuthorizationState, UserOnly,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

/// Management routes authenticate with the existing Macro authorization service.
pub struct RouterState<R, D, N, Auth> {
    /// Envelope use cases.
    pub service: Arc<Service<R, D, N>>,
    /// Authentication configuration.
    pub authorization_state: MacroAuthorizationState<Auth>,
}
impl<R, D, N, Auth> Clone for RouterState<R, D, N, Auth> {
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            authorization_state: self.authorization_state.clone(),
        }
    }
}
impl<R, D, N, Auth> FromRef<RouterState<R, D, N, Auth>> for MacroAuthorizationState<Auth> {
    fn from_ref(state: &RouterState<R, D, N, Auth>) -> Self {
        state.authorization_state.clone()
    }
}

/// Create sender management routes; account authorization is enforced in the domain.
pub fn management_router<
    R: Repository,
    D: Documents,
    N: Invitations,
    Auth: MacroAuthorizationService,
    T: Send + Sync + 'static,
>(
    state: RouterState<R, D, N, Auth>,
) -> Router<T> {
    Router::new()
        .route(
            "/envelopes",
            get(list::<R, D, N, Auth>).post(create::<R, D, N, Auth>),
        )
        .route(
            "/envelopes/{id}",
            get(read::<R, D, N, Auth>).put(update::<R, D, N, Auth>),
        )
        .route("/envelopes/{id}/send", post(send::<R, D, N, Auth>))
        .route("/envelopes/{id}/resend", post(resend::<R, D, N, Auth>))
        .route("/envelopes/{id}/void", post(void::<R, D, N, Auth>))
        .route("/envelopes/{id}/document", get(document::<R, D, N, Auth>))
        .layer(DefaultBodyLimit::max(15 * 1024 * 1024))
        .with_state(state)
}
/// Create public signing routes authorized only by one-recipient capabilities.
pub fn signing_router<R: Repository, D: Documents, N: Invitations, T: Send + Sync + 'static>(
    service: Arc<Service<R, D, N>>,
) -> Router<T> {
    Router::new()
        .route("/session", post(session::<R, D, N>))
        .route("/sign", post(sign::<R, D, N>))
        .route("/decline", post(decline::<R, D, N>))
        .route("/document", get(signing_document::<R, D, N>))
        .layer(DefaultBodyLimit::max(128 * 1024))
        .layer(axum::middleware::from_fn(private_response))
        .with_state(service)
}
async fn private_response(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert("referrer-policy", "no-referrer".parse().unwrap());
    response
        .headers_mut()
        .insert("x-content-type-options", "nosniff".parse().unwrap());
    response
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = match self {
            Error::Invalid(_) | Error::Pdf(_) => StatusCode::BAD_REQUEST,
            Error::NotFound => StatusCode::NOT_FOUND,
            Error::Conflict(_) => StatusCode::CONFLICT,
            Error::Gone => StatusCode::GONE,
            Error::Delivery => StatusCode::BAD_GATEWAY,
            Error::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(serde_json::json!({ "message": self.to_string() })),
        )
            .into_response()
    }
}
/// Creation body carrying a bounded PDF upload.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequest {
    /// Envelope subject.
    pub title: String,
    /// Original filename.
    pub filename: String,
    /// Base64 encoded immutable PDF snapshot.
    pub document_base64: String,
}
/// Optimistic mutation revision.
#[derive(Deserialize)]
pub struct RevisionRequest {
    /// Expected revision.
    pub revision: i64,
}
/// Reasoned lifecycle transition.
#[derive(Deserialize)]
pub struct ReasonRequest {
    /// Expected revision.
    pub revision: i64,
    /// User's decline/void reason.
    pub reason: String,
}
/// Select source or completed file.
#[derive(Deserialize, Default)]
pub struct DocumentQuery {
    /// Whether to fetch completed output.
    #[serde(default)]
    pub completed: bool,
}
async fn list<R: Repository, D: Documents, N: Invitations, Auth: MacroAuthorizationService>(
    State(s): State<RouterState<R, D, N, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOnly>,
) -> Result<Json<Vec<Envelope>>, Error> {
    Ok(Json(
        s.service
            .list(user.authorization.macro_user_id.as_ref())
            .await?,
    ))
}
async fn create<R: Repository, D: Documents, N: Invitations, Auth: MacroAuthorizationService>(
    State(s): State<RouterState<R, D, N, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOnly>,
    Json(body): Json<CreateRequest>,
) -> Result<(StatusCode, Json<Envelope>), Error> {
    let bytes = BASE64_STANDARD
        .decode(body.document_base64)
        .map_err(|_| Error::Invalid("Invalid PDF upload".into()))?;
    Ok((
        StatusCode::CREATED,
        Json(
            s.service
                .create(
                    user.authorization.macro_user_id.as_ref(),
                    body.title,
                    body.filename,
                    bytes,
                )
                .await?,
        ),
    ))
}
async fn read<R: Repository, D: Documents, N: Invitations, Auth: MacroAuthorizationService>(
    State(s): State<RouterState<R, D, N, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOnly>,
    Path(id): Path<Uuid>,
) -> Result<Json<Envelope>, Error> {
    Ok(Json(
        s.service
            .get(user.authorization.macro_user_id.as_ref(), id)
            .await?,
    ))
}
async fn update<R: Repository, D: Documents, N: Invitations, Auth: MacroAuthorizationService>(
    State(s): State<RouterState<R, D, N, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOnly>,
    Path(id): Path<Uuid>,
    Json(body): Json<Draft>,
) -> Result<Json<Envelope>, Error> {
    Ok(Json(
        s.service
            .update(user.authorization.macro_user_id.as_ref(), id, body)
            .await?,
    ))
}
async fn send<R: Repository, D: Documents, N: Invitations, Auth: MacroAuthorizationService>(
    State(s): State<RouterState<R, D, N, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOnly>,
    Path(id): Path<Uuid>,
    Json(body): Json<RevisionRequest>,
) -> Result<Json<Envelope>, Error> {
    Ok(Json(
        s.service
            .send(user.authorization.macro_user_id.as_ref(), id, body.revision)
            .await?,
    ))
}
async fn resend<R: Repository, D: Documents, N: Invitations, Auth: MacroAuthorizationService>(
    State(s): State<RouterState<R, D, N, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOnly>,
    Path(id): Path<Uuid>,
) -> Result<Json<Envelope>, Error> {
    Ok(Json(
        s.service
            .resend(user.authorization.macro_user_id.as_ref(), id)
            .await?,
    ))
}
async fn void<R: Repository, D: Documents, N: Invitations, Auth: MacroAuthorizationService>(
    State(s): State<RouterState<R, D, N, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOnly>,
    Path(id): Path<Uuid>,
    Json(body): Json<ReasonRequest>,
) -> Result<Json<Envelope>, Error> {
    Ok(Json(
        s.service
            .void(
                user.authorization.macro_user_id.as_ref(),
                id,
                body.revision,
                body.reason,
            )
            .await?,
    ))
}
async fn document<R: Repository, D: Documents, N: Invitations, Auth: MacroAuthorizationService>(
    State(s): State<RouterState<R, D, N, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOnly>,
    Path(id): Path<Uuid>,
    Query(query): Query<DocumentQuery>,
) -> Result<Response, Error> {
    Ok(pdf_response(
        s.service
            .document(
                user.authorization.macro_user_id.as_ref(),
                id,
                query.completed,
            )
            .await?,
    ))
}
fn token(headers: &HeaderMap) -> Result<&str, Error> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(Error::NotFound)
}
fn pdf_response(bytes: Vec<u8>) -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/pdf"),
            (header::CACHE_CONTROL, "no-store"),
            (
                header::CONTENT_DISPOSITION,
                "inline; filename=\"agreement.pdf\"",
            ),
        ],
        bytes,
    )
        .into_response()
}
async fn session<R: Repository, D: Documents, N: Invitations>(
    State(s): State<Arc<Service<R, D, N>>>,
    headers: HeaderMap,
) -> Result<Json<SigningSession>, Error> {
    Ok(Json(s.session(token(&headers)?).await?))
}
async fn sign<R: Repository, D: Documents, N: Invitations>(
    State(s): State<Arc<Service<R, D, N>>>,
    headers: HeaderMap,
    Json(body): Json<SignatureSubmission>,
) -> Result<Json<SigningSession>, Error> {
    let agent = headers
        .get(header::USER_AGENT)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default()
        .to_string();
    Ok(Json(s.sign(token(&headers)?, body, agent).await?))
}
async fn decline<R: Repository, D: Documents, N: Invitations>(
    State(s): State<Arc<Service<R, D, N>>>,
    headers: HeaderMap,
    Json(body): Json<ReasonRequest>,
) -> Result<StatusCode, Error> {
    s.decline(token(&headers)?, body.revision, body.reason)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn signing_document<R: Repository, D: Documents, N: Invitations>(
    State(s): State<Arc<Service<R, D, N>>>,
    headers: HeaderMap,
    Query(query): Query<DocumentQuery>,
) -> Result<Response, Error> {
    Ok(pdf_response(
        s.signing_document(token(&headers)?, query.completed)
            .await?,
    ))
}
