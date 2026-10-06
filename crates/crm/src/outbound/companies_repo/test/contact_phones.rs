use super::helpers::*;
use crate::domain::contact_phones::ContactPhoneRepository;
use crate::outbound::companies_repo::CompaniesRepositoryImpl;
use macro_db_migrator::MACRO_DB_MIGRATIONS;
use phone_number::PhoneNumber;
use sqlx::PgPool;
use uuid::Uuid;

fn numbers(values: &[&str]) -> Vec<PhoneNumber> {
    values
        .iter()
        .map(|value| PhoneNumber::from_e164(value).unwrap())
        .collect()
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn phone_numbers_are_replaced_in_the_order_entered(pool: PgPool) -> anyhow::Result<()> {
    let team_id = Uuid::now_v7();
    seed_team(&pool, team_id, "macro|owner@test.com").await?;
    let company_id = insert_company(&pool, team_id, true, &["acme.com"]).await?;
    let contact_id = insert_contact(&pool, company_id, "jane@acme.com").await?;
    let repo = CompaniesRepositoryImpl::new(pool.clone());

    assert_eq!(
        repo.list_contact_phone_numbers(&team_id, &contact_id, false)
            .await?,
        Some(Vec::new())
    );
    let entered = numbers(&["+15559876543", "+442079460958", "+15552345678"]);
    assert_eq!(
        repo.replace_contact_phone_numbers(&team_id, &contact_id, &entered, false)
            .await?,
        Some(entered.clone())
    );
    assert_eq!(
        repo.list_contact_phone_numbers(&team_id, &contact_id, false)
            .await?,
        Some(entered)
    );

    let replaced = numbers(&["+15552345678"]);
    repo.replace_contact_phone_numbers(&team_id, &contact_id, &replaced, false)
        .await?;
    assert_eq!(
        repo.list_contact_phone_numbers(&team_id, &contact_id, false)
            .await?,
        Some(replaced)
    );
    Ok(())
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn phone_numbers_follow_contact_visibility(pool: PgPool) -> anyhow::Result<()> {
    let team_id = Uuid::now_v7();
    seed_team(&pool, team_id, "macro|owner@test.com").await?;
    let other_team = Uuid::now_v7();
    seed_team(&pool, other_team, "macro|other@test.com").await?;
    let company_id = insert_company(&pool, team_id, true, &["acme.com"]).await?;
    let contact_id = insert_contact(&pool, company_id, "jane@acme.com").await?;
    sqlx::query("UPDATE crm_contacts SET hidden = TRUE WHERE id = $1")
        .bind(contact_id)
        .execute(&pool)
        .await?;
    let repo = CompaniesRepositoryImpl::new(pool.clone());
    let jane = numbers(&["+15552345678"]);

    // Members cannot see or edit a hidden contact; admins can.
    assert_eq!(
        repo.replace_contact_phone_numbers(&team_id, &contact_id, &jane, false)
            .await?,
        None
    );
    assert_eq!(
        repo.replace_contact_phone_numbers(&team_id, &contact_id, &jane, true)
            .await?,
        Some(jane.clone())
    );
    assert_eq!(
        repo.list_contact_phone_numbers(&team_id, &contact_id, false)
            .await?,
        None
    );
    assert!(
        repo.get_contact_by_phone_for_team(&team_id, &jane[0], false)
            .await?
            .is_none()
    );
    assert_eq!(
        repo.get_contact_by_phone_for_team(&team_id, &jane[0], true)
            .await?
            .map(|contact| contact.id),
        Some(contact_id)
    );
    // Another team never reaches it.
    assert_eq!(
        repo.list_contact_phone_numbers(&other_team, &contact_id, true)
            .await?,
        None
    );
    assert!(
        repo.get_contact_by_phone_for_team(&other_team, &jane[0], true)
            .await?
            .is_none()
    );
    Ok(())
}
