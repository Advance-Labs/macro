//! Phone numbers of CRM contacts.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use entity_access::{
    domain::{
        models::{MemberTeamRole, ViewAccessLevel},
        ports::EntityAccessService,
    },
    inbound::axum_extractors::MacroUserTeamExtractorV2,
};
use macro_authorization::MacroAuthorizationService;
use model_error_response::ErrorResponse;
use phone_number::PhoneNumber;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    domain::{auth::CrmTeamReceipt, contact_phones::CrmContactPhoneService, model::CrmError},
    inbound::axum_extractors::CrmContactAccessLevelExtractor,
};

use super::{CrmRouterState, list_company_contacts::CrmContactResponse};

/// A contact's phone numbers, in E.164, in the order they were entered.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContactPhoneNumbersResponse {
    /// The contact's numbers.
    pub phone_numbers: Vec<PhoneNumber>,
}

/// Request body for replacing a contact's phone numbers.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetContactPhoneNumbersRequest {
    /// The complete new list, as typed: E.164, formatted, or North American
    /// national numbers. Duplicates are dropped; an empty list clears them.
    pub phone_numbers: Vec<String>,
}

/// List a CRM contact's phone numbers.
#[utoipa::path(
    get,
    path = "/crm/contacts/{contact_id}/phone-numbers",
    operation_id = "get_crm_contact_phone_numbers",
    params(("contact_id" = Uuid, Path, description = "The CRM contact")),
    responses(
        (status = 200, body = ContactPhoneNumbersResponse),
        (status = 401, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    ),
)]
#[tracing::instrument(skip_all, err, fields(contact_id = %contact_id))]
pub async fn list_handler<
    C: CrmContactPhoneService,
    St,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
>(
    access: CrmContactAccessLevelExtractor<ViewAccessLevel, Eas, Auth>,
    State(state): State<CrmRouterState<C, St, Eas, Auth>>,
    Path(contact_id): Path<Uuid>,
) -> Result<Json<ContactPhoneNumbersResponse>, CrmError> {
    let phone_numbers = state
        .service
        .list_contact_phone_numbers(&access.receipt)
        .await?;
    Ok(Json(ContactPhoneNumbersResponse { phone_numbers }))
}

/// Replace a CRM contact's phone numbers. Any team member who can see the
/// contact may edit them, like its name.
#[utoipa::path(
    put,
    path = "/crm/contacts/{contact_id}/phone-numbers",
    operation_id = "set_crm_contact_phone_numbers",
    params(("contact_id" = Uuid, Path, description = "The CRM contact")),
    request_body = SetContactPhoneNumbersRequest,
    responses(
        (status = 200, body = ContactPhoneNumbersResponse),
        (status = 400, body = ErrorResponse, description = "A number could not be read"),
        (status = 401, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    ),
)]
#[tracing::instrument(skip_all, err, fields(contact_id = %contact_id))]
pub async fn set_handler<
    C: CrmContactPhoneService,
    St,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
>(
    access: CrmContactAccessLevelExtractor<ViewAccessLevel, Eas, Auth>,
    State(state): State<CrmRouterState<C, St, Eas, Auth>>,
    Path(contact_id): Path<Uuid>,
    Json(request): Json<SetContactPhoneNumbersRequest>,
) -> Result<Json<ContactPhoneNumbersResponse>, CrmError> {
    let phone_numbers = state
        .service
        .set_contact_phone_numbers(&access.receipt, &request.phone_numbers)
        .await?;
    Ok(Json(ContactPhoneNumbersResponse { phone_numbers }))
}

/// Query parameters for resolving a CRM contact by phone number.
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct GetContactByPhoneParams {
    /// The number to look up, as typed or in E.164.
    pub phone: String,
}

/// Response from looking up a CRM contact by phone number.
#[derive(Debug, Serialize, ToSchema)]
pub struct GetContactByPhoneResponse {
    /// The matching contact, or `null` when none is visible.
    pub contact: Option<CrmContactResponse>,
}

/// Look up who a phone number belongs to in the caller's team CRM.
#[utoipa::path(
    get,
    path = "/crm/contacts/by-phone",
    operation_id = "get_contact_by_phone",
    params(GetContactByPhoneParams),
    responses(
        (status = 200, body = GetContactByPhoneResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    ),
)]
#[tracing::instrument(skip_all, err)]
pub async fn by_phone_handler<
    C: CrmContactPhoneService,
    St,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
>(
    access: MacroUserTeamExtractorV2<MemberTeamRole, Eas, Auth>,
    State(state): State<CrmRouterState<C, St, Eas, Auth>>,
    Query(params): Query<GetContactByPhoneParams>,
) -> Result<Json<GetContactByPhoneResponse>, CrmError> {
    let receipt = CrmTeamReceipt::from_team_receipt(access.entity_access_receipt)?;
    let number = PhoneNumber::parse(&params.phone)
        .map_err(|error| CrmError::InvalidRequest(error.to_string()))?;
    let contact = state
        .service
        .get_contact_by_phone(&receipt, &number)
        .await?
        .map(Into::into);
    Ok(Json(GetContactByPhoneResponse { contact }))
}
