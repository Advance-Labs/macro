//! [`CrmContactPhoneService`] for [`CrmServiceImpl`].

use entity_access::domain::models::{MemberTeamRole, ViewAccessLevel};
use phone_number::PhoneNumber;

use super::CrmServiceImpl;
use crate::domain::{
    auth::{CrmContactReceipt, CrmTeamReceipt},
    companies_repo::CompaniesRepository,
    company_metadata_resolver::CompanyMetadataResolver,
    contact_phones::{ContactPhoneRepository, CrmContactPhoneService, parse_contact_phone_numbers},
    model::{CrmContact, CrmError},
};

impl<CR, R> CrmContactPhoneService for CrmServiceImpl<CR, R>
where
    CR: CompaniesRepository + ContactPhoneRepository,
    R: CompanyMetadataResolver,
{
    #[tracing::instrument(skip(self, access), err)]
    async fn list_contact_phone_numbers(
        &self,
        access: &CrmContactReceipt<ViewAccessLevel>,
    ) -> Result<Vec<PhoneNumber>, CrmError> {
        self.companies_repository
            .list_contact_phone_numbers(
                &access.team_id(),
                &access.contact_id()?,
                access.include_hidden(),
            )
            .await?
            .ok_or(CrmError::ContactNotFoundForTeam)
    }

    #[tracing::instrument(skip(self, access, numbers), err)]
    async fn set_contact_phone_numbers(
        &self,
        access: &CrmContactReceipt<ViewAccessLevel>,
        numbers: &[String],
    ) -> Result<Vec<PhoneNumber>, CrmError> {
        let numbers = parse_contact_phone_numbers(numbers)?;
        self.companies_repository
            .replace_contact_phone_numbers(
                &access.team_id(),
                &access.contact_id()?,
                &numbers,
                access.include_hidden(),
            )
            .await?
            .ok_or(CrmError::ContactNotFoundForTeam)
    }

    #[tracing::instrument(skip(self, access), err)]
    async fn get_contact_by_phone(
        &self,
        access: &CrmTeamReceipt<MemberTeamRole>,
        number: &PhoneNumber,
    ) -> Result<Option<CrmContact>, CrmError> {
        self.companies_repository
            .get_contact_by_phone_for_team(&access.team_id(), number, access.include_hidden())
            .await
    }
}
