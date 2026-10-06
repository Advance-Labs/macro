//! Phone call HTTP endpoints, nested under `/call/phone`.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use entity_access::{
    domain::{models::ViewAccessLevel, ports::EntityAccessService},
    inbound::axum_extractors::CallAccessLevelExtractor,
};
use macro_authorization::{
    InternalOnly, MacroAuthorizationExtractor, MacroAuthorizationService, UserOrInternal,
};
use model_error_response::ErrorResponse;

use super::CallRouterState;
use crate::domain::models::{CallError, LeaveCallResponse};
use crate::domain::phone::{
    AssignPhoneNumberRequest, DialPhoneRequest, IncomingPhoneCallsResponse, PhoneCallJoinResponse,
    PhoneNumber, PhoneSettingsResponse,
};
use crate::domain::ports::CallService;

fn path_phone_number(value: &str) -> Result<PhoneNumber, CallError> {
    PhoneNumber::parse(value).map_err(|error| CallError::InvalidRequest(error.to_string()))
}

/// Whether the caller can place phone calls, their caller id, and the
/// numbers that ring them.
#[utoipa::path(
    get,
    operation_id = "get_phone_settings",
    path = "/call/phone/settings",
    responses(
        (status = 200, body = PhoneSettingsResponse),
        (status = 401, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn settings<S: CallService, Svc: EntityAccessService, Auth: MacroAuthorizationService>(
    State(state): State<CallRouterState<S, Svc, Auth>>,
    actor: MacroAuthorizationExtractor<Auth, UserOrInternal>,
) -> Result<Json<PhoneSettingsResponse>, CallError> {
    Ok(Json(
        state
            .service
            .get_phone_settings(actor.authorization.user.macro_user_id.clone())
            .await?,
    ))
}

/// Place a phone call. Returns the caller's join credentials once the call
/// is ringing; it is recorded and transcribed like any other call.
#[utoipa::path(
    post,
    operation_id = "dial_phone",
    path = "/call/phone/dial",
    request_body = DialPhoneRequest,
    responses(
        (status = 200, body = PhoneCallJoinResponse),
        (status = 400, body = ErrorResponse, description = "The number cannot or may not be dialed"),
        (status = 401, body = ErrorResponse),
        (status = 409, body = ErrorResponse, description = "The caller is still in another call"),
        (status = 503, body = ErrorResponse, description = "Phone calling is not set up"),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn dial<S: CallService, Svc: EntityAccessService, Auth: MacroAuthorizationService>(
    State(state): State<CallRouterState<S, Svc, Auth>>,
    actor: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Json(request): Json<DialPhoneRequest>,
) -> Result<Json<PhoneCallJoinResponse>, CallError> {
    Ok(Json(
        state
            .service
            .dial_phone(actor.authorization.user.macro_user_id.clone(), request)
            .await?,
    ))
}

/// Inbound phone calls ringing for the caller, newest first.
#[utoipa::path(
    get,
    operation_id = "list_incoming_phone_calls",
    path = "/call/phone/incoming",
    responses(
        (status = 200, body = IncomingPhoneCallsResponse),
        (status = 401, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn incoming<S: CallService, Svc: EntityAccessService, Auth: MacroAuthorizationService>(
    State(state): State<CallRouterState<S, Svc, Auth>>,
    actor: MacroAuthorizationExtractor<Auth, UserOrInternal>,
) -> Result<Json<IncomingPhoneCallsResponse>, CallError> {
    Ok(Json(
        state
            .service
            .list_incoming_phone_calls(actor.authorization.user.macro_user_id.clone())
            .await?,
    ))
}

/// Answer an inbound phone call ringing for the caller.
#[utoipa::path(
    post,
    operation_id = "answer_phone_call",
    path = "/call/phone/{call_id}/answer",
    params(("call_id" = uuid::Uuid, Path, description = "Call ID")),
    responses(
        (status = 200, body = PhoneCallJoinResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse, description = "The call has ended"),
        (status = 409, body = ErrorResponse, description = "The call was already answered"),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn answer<S: CallService, Svc: EntityAccessService, Auth: MacroAuthorizationService>(
    State(state): State<CallRouterState<S, Svc, Auth>>,
    access: CallAccessLevelExtractor<ViewAccessLevel, Svc, Auth>,
) -> Result<Json<PhoneCallJoinResponse>, CallError> {
    Ok(Json(
        state
            .service
            .answer_phone_call(access.entity_access_receipt)
            .await?,
    ))
}

/// Hang up a phone call for everyone. Declines a ringing inbound call and
/// cancels an outbound call that has not connected yet.
#[utoipa::path(
    post,
    operation_id = "hang_up_phone_call",
    path = "/call/phone/{call_id}/hang-up",
    params(("call_id" = uuid::Uuid, Path, description = "Call ID")),
    responses(
        (status = 200, body = LeaveCallResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse, description = "The call has ended"),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn hang_up<S: CallService, Svc: EntityAccessService, Auth: MacroAuthorizationService>(
    State(state): State<CallRouterState<S, Svc, Auth>>,
    access: CallAccessLevelExtractor<ViewAccessLevel, Svc, Auth>,
) -> Result<Json<LeaveCallResponse>, CallError> {
    Ok(Json(
        state
            .service
            .hang_up_phone_call(access.entity_access_receipt)
            .await?,
    ))
}

/// Assign a phone number to a user, taking it from any previous owner.
/// Internal services only.
#[utoipa::path(
    put,
    operation_id = "assign_phone_number",
    path = "/call/phone/numbers/{phone_number}",
    params(("phone_number" = String, Path, description = "Phone number, preferably E.164 (URL-encode the +)")),
    request_body = AssignPhoneNumberRequest,
    responses(
        (status = 204),
        (status = 400, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse, description = "The user does not exist"),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn assign_number<
    S: CallService,
    Svc: EntityAccessService,
    Auth: MacroAuthorizationService,
>(
    State(state): State<CallRouterState<S, Svc, Auth>>,
    _internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Path(phone_number): Path<String>,
    Json(request): Json<AssignPhoneNumberRequest>,
) -> Result<StatusCode, CallError> {
    state
        .service
        .assign_phone_number(path_phone_number(&phone_number)?, request)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Unassign a phone number. Calls to it are then rejected. Internal services
/// only.
#[utoipa::path(
    delete,
    operation_id = "release_phone_number",
    path = "/call/phone/numbers/{phone_number}",
    params(("phone_number" = String, Path, description = "Phone number, preferably E.164 (URL-encode the +)")),
    responses(
        (status = 204),
        (status = 400, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse, description = "The number was not assigned"),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn release_number<
    S: CallService,
    Svc: EntityAccessService,
    Auth: MacroAuthorizationService,
>(
    State(state): State<CallRouterState<S, Svc, Auth>>,
    _internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Path(phone_number): Path<String>,
) -> Result<StatusCode, CallError> {
    state
        .service
        .release_phone_number(path_phone_number(&phone_number)?)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
