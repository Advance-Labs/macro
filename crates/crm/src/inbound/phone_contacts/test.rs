use entity_access::{domain::service::EntityAccessServiceImpl, outbound::PgAccessRepository};
use macro_user_id::cowlike::CowLike;
use macro_db_migrator::MACRO_DB_MIGRATIONS;
use sqlx::PgPool;
use uuid::Uuid;

use super::*;
use crate::{
    domain::service::CrmServiceImpl,
    outbound::{companies_repo::CompaniesRepositoryImpl, no_op_resolver::NoOpCompanyMetadataResolver},
};

const MEMBER: &str = "macro|rep@ours.com";
const OUTSIDER: &str = "macro|someone@elsewhere.com";
const NUMBER: &str = "+15552345678";

async fn insert_user(pool: &PgPool, user_id: &str) {
    let macro_user_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO macro_user (id, username, email, stripe_customer_id) VALUES ($1, $2, $2, $3)",
    )
    .bind(macro_user_id)
    .bind(user_id)
    .bind(format!("stripe_{macro_user_id}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO "User" (id, email, macro_user_id) VALUES ($1, $1, $2)"#)
        .bind(user_id)
        .bind(macro_user_id)
        .execute(pool)
        .await
        .unwrap();
}

/// A team whose CRM has one contact, Ada, at `NUMBER`. Returns her id and
/// the id of a hidden contact at the same number.
async fn seed(pool: &PgPool) -> (Uuid, Uuid) {
    insert_user(pool, MEMBER).await;
    insert_user(pool, OUTSIDER).await;
    let team_id = Uuid::now_v7();
    sqlx::query("INSERT INTO team (id, name, owner_id) VALUES ($1, 'team', $2)")
        .bind(team_id)
        .bind(MEMBER)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO team_user (user_id, team_id, team_role) VALUES ($1, $2, 'member'::team_role)",
    )
    .bind(MEMBER)
    .bind(team_id)
    .execute(pool)
    .await
    .unwrap();

    let mut contacts = Vec::new();
    for (hidden, email, last_interaction) in [
        (false, "ada@acme.com", "2024-01-01T00:00:00Z"),
        (true, "hidden@acme.com", "2025-01-01T00:00:00Z"),
    ] {
        let company_id = Uuid::now_v7();
        sqlx::query(
            r#"INSERT INTO crm_companies (id, team_id, hidden, first_interaction, last_interaction)
               VALUES ($1, $2, FALSE, now(), now())"#,
        )
        .bind(company_id)
        .bind(team_id)
        .execute(pool)
        .await
        .unwrap();
        let contact_id = Uuid::now_v7();
        sqlx::query(
            r#"INSERT INTO crm_contacts (id, company_id, email, name, hidden, first_interaction, last_interaction)
               VALUES ($1, $2, $3, 'Ada Lovelace', $4, now(), $5::timestamptz)"#,
        )
        .bind(contact_id)
        .bind(company_id)
        .bind(email)
        .bind(hidden)
        .bind(last_interaction)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO crm_contact_phone_numbers (contact_id, phone_number, position) VALUES ($1, $2, 0)",
        )
        .bind(contact_id)
        .bind(NUMBER)
        .execute(pool)
        .await
        .unwrap();
        contacts.push(contact_id);
    }
    (contacts[0], contacts[1])
}

fn directory(
    pool: &PgPool,
) -> CrmPhoneContacts<
    CrmServiceImpl<CompaniesRepositoryImpl, NoOpCompanyMetadataResolver>,
    EntityAccessServiceImpl<PgAccessRepository>,
> {
    CrmPhoneContacts::new(
        CrmServiceImpl::new(
            CompaniesRepositoryImpl::new(pool.clone()),
            NoOpCompanyMetadataResolver,
        ),
        EntityAccessServiceImpl::new(PgAccessRepository::new(pool.clone())),
    )
}

fn user(user_id: &str) -> MacroUserIdStr<'static> {
    MacroUserIdStr::parse_from_str(user_id).unwrap().into_owned()
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn names_callers_from_the_users_team_crm(pool: PgPool) {
    let (ada, hidden) = seed(&pool).await;
    let number = PhoneNumber::from_e164(NUMBER).unwrap();

    let contact = directory(&pool)
        .find_contact(user(MEMBER), &number)
        .await
        .unwrap()
        .expect("the member's CRM knows the number");
    assert_eq!(contact.contact_id, ada, "a hidden contact is never shown to a member");
    assert_ne!(contact.contact_id, hidden);
    assert_eq!(contact.name.as_deref(), Some("Ada Lovelace"));
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn users_outside_a_team_see_no_contacts(pool: PgPool) {
    seed(&pool).await;
    let number = PhoneNumber::from_e164(NUMBER).unwrap();

    assert_eq!(
        directory(&pool)
            .find_contact(user(OUTSIDER), &number)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        directory(&pool)
            .find_contact(user(MEMBER), &PhoneNumber::from_e164("+15559876543").unwrap())
            .await
            .unwrap(),
        None
    );
}
