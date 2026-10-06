//! [`ContactPhoneRepository`] for [`CompaniesRepositoryImpl`].

use phone_number::PhoneNumber;
use uuid::Uuid;

use super::CompaniesRepositoryImpl;
use crate::domain::{
    contact_phones::ContactPhoneRepository,
    model::{CrmContact, CrmError},
};

fn storage_error(error: sqlx::Error) -> CrmError {
    CrmError::StorageLayerError(error.into())
}

/// Stored numbers are E.164 by constraint; one that is not names nobody.
fn stored_numbers(numbers: Vec<String>) -> Vec<PhoneNumber> {
    numbers
        .iter()
        .filter_map(|number| PhoneNumber::from_e164(number).ok())
        .collect()
}

impl CompaniesRepositoryImpl {
    /// Whether the contact is visible to the team, locking it for the rest
    /// of the transaction when one is passed.
    async fn contact_visible(
        conn: &mut sqlx::PgConnection,
        team_id: &Uuid,
        contact_id: &Uuid,
        include_hidden: bool,
    ) -> Result<bool, CrmError> {
        Ok(sqlx::query_scalar!(
            r#"
            SELECT ct.id
            FROM crm_contacts ct
            JOIN crm_companies co ON co.id = ct.company_id
            WHERE ct.id = $1
              AND co.team_id = $2
              AND ($3 OR (ct.hidden = FALSE AND co.hidden = FALSE))
            FOR SHARE OF ct
            "#,
            contact_id,
            team_id,
            include_hidden,
        )
        .fetch_optional(conn)
        .await
        .map_err(storage_error)?
        .is_some())
    }

    async fn contact_phone_numbers(
        conn: &mut sqlx::PgConnection,
        contact_id: &Uuid,
    ) -> Result<Vec<PhoneNumber>, CrmError> {
        let numbers = sqlx::query_scalar!(
            r#"
            SELECT phone_number
            FROM crm_contact_phone_numbers
            WHERE contact_id = $1
            ORDER BY position, phone_number
            "#,
            contact_id,
        )
        .fetch_all(conn)
        .await
        .map_err(storage_error)?;
        Ok(stored_numbers(numbers))
    }
}

impl ContactPhoneRepository for CompaniesRepositoryImpl {
    #[tracing::instrument(skip(self), err)]
    async fn list_contact_phone_numbers(
        &self,
        team_id: &Uuid,
        contact_id: &Uuid,
        include_hidden: bool,
    ) -> Result<Option<Vec<PhoneNumber>>, CrmError> {
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        if !Self::contact_visible(&mut tx, team_id, contact_id, include_hidden).await? {
            return Ok(None);
        }
        let numbers = Self::contact_phone_numbers(&mut tx, contact_id).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(Some(numbers))
    }

    #[tracing::instrument(skip(self, numbers), err)]
    async fn replace_contact_phone_numbers(
        &self,
        team_id: &Uuid,
        contact_id: &Uuid,
        numbers: &[PhoneNumber],
        include_hidden: bool,
    ) -> Result<Option<Vec<PhoneNumber>>, CrmError> {
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        if !Self::contact_visible(&mut tx, team_id, contact_id, include_hidden).await? {
            return Ok(None);
        }
        sqlx::query!(
            "DELETE FROM crm_contact_phone_numbers WHERE contact_id = $1",
            contact_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        let numbers: Vec<String> = numbers.iter().map(ToString::to_string).collect();
        sqlx::query!(
            r#"
            INSERT INTO crm_contact_phone_numbers (contact_id, phone_number, position)
            SELECT $1, number, (ordinality - 1)::smallint
            FROM UNNEST($2::text[]) WITH ORDINALITY AS entered(number, ordinality)
            ON CONFLICT (contact_id, phone_number) DO NOTHING
            "#,
            contact_id,
            &numbers,
        )
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        let stored = Self::contact_phone_numbers(&mut tx, contact_id).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(Some(stored))
    }

    #[tracing::instrument(skip(self), err)]
    async fn get_contact_by_phone_for_team(
        &self,
        team_id: &Uuid,
        number: &PhoneNumber,
        include_hidden: bool,
    ) -> Result<Option<CrmContact>, CrmError> {
        let row = sqlx::query!(
            r#"
            SELECT
                ct.id,
                ct.company_id,
                ct.email,
                ct.name,
                ct.hidden,
                ct.first_interaction,
                ct.last_interaction,
                ct.created_at,
                ct.updated_at
            FROM crm_contact_phone_numbers p
            JOIN crm_contacts ct ON ct.id = p.contact_id
            JOIN crm_companies co ON co.id = ct.company_id
            WHERE p.phone_number = $1
              AND co.team_id = $2
              AND ($3 OR (ct.hidden = FALSE AND co.hidden = FALSE))
            ORDER BY ct.last_interaction DESC, ct.id DESC
            LIMIT 1
            "#,
            number.as_str(),
            team_id,
            include_hidden,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(row.map(|row| CrmContact {
            id: row.id,
            company_id: row.company_id,
            email: row.email,
            name: row.name,
            hidden: row.hidden,
            first_interaction: row.first_interaction,
            last_interaction: row.last_interaction,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }))
    }
}
