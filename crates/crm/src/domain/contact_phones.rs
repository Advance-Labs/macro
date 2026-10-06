//! Phone numbers of CRM contacts.
//!
//! Numbers let people call a contact from the CRM, and let a phone call be
//! matched to the contact on the other end. They are stored in E.164 and
//! scoped exactly like the contact they belong to: team members see the
//! numbers of visible contacts; admins and owners also see hidden ones.

use std::future::Future;

use entity_access::domain::models::{MemberTeamRole, ViewAccessLevel};
use phone_number::PhoneNumber;
use uuid::Uuid;

use crate::domain::{
    auth::{CrmContactReceipt, CrmTeamReceipt},
    model::{CrmContact, CrmError},
};

/// Most phone numbers one contact may have.
pub const MAX_CONTACT_PHONE_NUMBERS: usize = 10;

/// Persistence for contact phone numbers, scoped to a team like every
/// contact read: a contact on another team, or a hidden contact (or one
/// under a hidden company) when `include_hidden` is false, is invisible.
pub trait ContactPhoneRepository: Send + Sync + 'static {
    /// The contact's numbers in the order they were added, or `None` when
    /// the contact is not visible to the team.
    fn list_contact_phone_numbers(
        &self,
        team_id: &Uuid,
        contact_id: &Uuid,
        include_hidden: bool,
    ) -> impl Future<Output = Result<Option<Vec<PhoneNumber>>, CrmError>> + Send;

    /// Replace the contact's numbers with `numbers` (deduplicated, in
    /// order) and return the stored list, or `None` without changing
    /// anything when the contact is not visible to the team.
    fn replace_contact_phone_numbers(
        &self,
        team_id: &Uuid,
        contact_id: &Uuid,
        numbers: &[PhoneNumber],
        include_hidden: bool,
    ) -> impl Future<Output = Result<Option<Vec<PhoneNumber>>, CrmError>> + Send;

    /// The visible contact with `number`, most recently interacted with
    /// first when several share it (e.g. a front desk).
    fn get_contact_by_phone_for_team(
        &self,
        team_id: &Uuid,
        number: &PhoneNumber,
        include_hidden: bool,
    ) -> impl Future<Output = Result<Option<CrmContact>, CrmError>> + Send;
}

/// Contact phone number use cases.
pub trait CrmContactPhoneService: Send + Sync + 'static {
    /// The numbers of the contact addressed by `access`.
    fn list_contact_phone_numbers(
        &self,
        access: &CrmContactReceipt<ViewAccessLevel>,
    ) -> impl Future<Output = Result<Vec<PhoneNumber>, CrmError>> + Send;

    /// Replace the numbers of the contact addressed by `access` with
    /// `numbers`, as typed: formatted, national (North American), or E.164,
    /// each parsed and normalized to E.164. Any team member who can see the
    /// contact may edit them, like its name. Returns the stored list.
    fn set_contact_phone_numbers(
        &self,
        access: &CrmContactReceipt<ViewAccessLevel>,
        numbers: &[String],
    ) -> impl Future<Output = Result<Vec<PhoneNumber>, CrmError>> + Send;

    /// The contact at `number` in the caller's team, if any.
    fn get_contact_by_phone(
        &self,
        access: &CrmTeamReceipt<MemberTeamRole>,
        number: &PhoneNumber,
    ) -> impl Future<Output = Result<Option<CrmContact>, CrmError>> + Send;
}

/// Parse typed numbers into a distinct, ordered list of E.164 numbers.
pub(crate) fn parse_contact_phone_numbers(
    numbers: &[String],
) -> Result<Vec<PhoneNumber>, CrmError> {
    let mut parsed: Vec<PhoneNumber> = Vec::with_capacity(numbers.len());
    for raw in numbers {
        let number = PhoneNumber::parse(raw)
            .map_err(|error| CrmError::InvalidRequest(format!("{raw:?}: {error}")))?;
        if !parsed.contains(&number) {
            parsed.push(number);
        }
    }
    if parsed.len() > MAX_CONTACT_PHONE_NUMBERS {
        return Err(CrmError::InvalidRequest(format!(
            "A contact can have at most {MAX_CONTACT_PHONE_NUMBERS} phone numbers"
        )));
    }
    Ok(parsed)
}

#[cfg(test)]
mod test;
