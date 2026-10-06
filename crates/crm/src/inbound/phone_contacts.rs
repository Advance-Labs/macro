//! Names the person on the other end of a phone call from the CRM of the
//! call owner's team.

use call::domain::phone::{PhoneContact, PhoneNumber};
use call::domain::ports::phone::PhoneContactDirectory;
use entity_access::domain::{
    models::{EntityType, MemberTeamRole},
    ports::EntityAccessService,
};
use macro_user_id::user_id::MacroUserIdStr;

use crate::domain::{auth::CrmTeamReceipt, contact_phones::CrmContactPhoneService};

/// [`PhoneContactDirectory`] backed by the CRM: a user sees the contacts of
/// their team's CRM, exactly as they would through the CRM API.
#[derive(Clone)]
pub struct CrmPhoneContacts<C, E> {
    crm: C,
    entity_access: E,
}

impl<C, E> CrmPhoneContacts<C, E>
where
    C: CrmContactPhoneService + Clone,
    E: EntityAccessService,
{
    /// Look contacts up through `crm`, authorizing each lookup as the user
    /// it is made for.
    pub fn new(crm: C, entity_access: E) -> Self {
        Self { crm, entity_access }
    }
}

impl<C, E> PhoneContactDirectory for CrmPhoneContacts<C, E>
where
    C: CrmContactPhoneService + Clone,
    E: EntityAccessService,
{
    #[tracing::instrument(skip(self), err)]
    async fn find_contact(
        &self,
        user_id: MacroUserIdStr<'_>,
        number: &PhoneNumber,
    ) -> Result<Option<PhoneContact>, rootcause::Report> {
        let Some(team) = self
            .entity_access
            .get_user_team(&user_id)
            .await
            .map_err(|error| rootcause::report!(error))?
        else {
            return Ok(None);
        };
        let receipt = self
            .entity_access
            .generate_entity_access_receipt::<MemberTeamRole>(
                &user_id,
                None,
                &team.team_id.to_string(),
                EntityType::Team,
            )
            .await
            .map_err(|error| rootcause::report!(error))?;
        let receipt =
            CrmTeamReceipt::from_team_receipt(receipt).map_err(|error| rootcause::report!(error))?;
        Ok(self
            .crm
            .get_contact_by_phone(&receipt, number)
            .await
            .map_err(|error| rootcause::report!(error))?
            .map(|contact| PhoneContact {
                contact_id: contact.id,
                name: contact.name,
            }))
    }
}

#[cfg(test)]
mod test;
