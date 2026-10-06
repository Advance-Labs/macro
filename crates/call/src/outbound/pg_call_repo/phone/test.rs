use super::*;
use crate::domain::phone::NewPhoneLeg;
use macro_db_migrator::MACRO_DB_MIGRATIONS;

const OWNER: &str = "owner@phone.test";
const TEAMMATE: &str = "teammate@phone.test";
const OWNER_NUMBER: &str = "+15559876543";
const CALLER: &str = "+15552345678";
const CONTACT_ID: Uuid = Uuid::from_u128(0x0198a1b2_c3d4_7e5f_8061_000000000c01);

fn user(email: &str) -> MacroUserIdStr<'static> {
    MacroUserIdStr::try_from_email(email).unwrap().into_owned()
}

fn number(value: &str) -> PhoneNumber {
    PhoneNumber::from_e164(value).unwrap()
}

async fn repo(pool: PgPool) -> PgCallRepo {
    for email in [OWNER, TEAMMATE] {
        super::super::test::insert_user_mapping(&pool, &user(email), Uuid::now_v7())
            .await
            .unwrap();
    }
    PgCallRepo::new(pool)
}

fn new_call(direction: PhoneCallDirection, room_name: Option<&str>) -> NewPhoneCall {
    let call_id = Uuid::now_v7();
    NewPhoneCall {
        call_id,
        room_name: room_name.map_or_else(|| call_id.to_string(), str::to_string),
        owner: user(OWNER),
        leg: NewPhoneLeg {
            direction,
            remote_number: number(CALLER),
            local_number: Some(number(OWNER_NUMBER)),
            participant_identity: format!("sip_{CALLER}"),
            status: match direction {
                PhoneCallDirection::Outbound => PhoneCallStatus::Dialing,
                PhoneCallDirection::Inbound => PhoneCallStatus::Ringing,
            },
            contact: Some(PhoneContact {
                contact_id: CONTACT_ID,
                name: Some("Ada Lovelace".to_string()),
            }),
            sip_call_id: None,
        },
    }
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn numbers_are_assigned_reassigned_and_released(pool: PgPool) {
    let repo = repo(pool).await;
    let phone_number = number(OWNER_NUMBER);

    repo.assign_phone_number(&phone_number, user(OWNER))
        .await
        .unwrap();
    assert_eq!(
        repo.phone_number_owner(&phone_number)
            .await
            .unwrap()
            .map(|owner| owner.to_string()),
        Some(user(OWNER).to_string())
    );
    assert_eq!(
        repo.phone_numbers_for_user(user(OWNER)).await.unwrap(),
        vec![phone_number.clone()]
    );

    repo.assign_phone_number(&phone_number, user(TEAMMATE))
        .await
        .unwrap();
    assert!(
        repo.phone_numbers_for_user(user(OWNER))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repo.phone_numbers_for_user(user(TEAMMATE)).await.unwrap(),
        vec![phone_number.clone()]
    );

    assert!(repo.release_phone_number(&phone_number).await.unwrap());
    assert!(!repo.release_phone_number(&phone_number).await.unwrap());
    assert_eq!(repo.phone_number_owner(&phone_number).await.unwrap(), None);

    let missing_user = repo
        .assign_phone_number(&phone_number, user("nobody@phone.test"))
        .await
        .unwrap_err();
    assert!(
        matches!(missing_user, CallError::NotFound(_)),
        "{missing_user:?}"
    );
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn outbound_calls_are_owned_joined_and_carry_their_leg(pool: PgPool) {
    let repo = repo(pool.clone()).await;
    let new_call = new_call(PhoneCallDirection::Outbound, None);
    let call_id = new_call.call_id;

    let call = repo.create_outbound_phone_call(new_call).await.unwrap();

    assert_eq!(call.id, call_id);
    assert_eq!(call.channel_id, None);
    assert_eq!(call.created_by, user(OWNER).to_string());
    assert!(
        repo.is_participant(&call_id, user(OWNER).as_ref())
            .await
            .unwrap()
    );
    let grant: String = sqlx::query_scalar(
        "SELECT access_level::text FROM entity_access WHERE entity_id = $1 AND entity_type = 'call' AND source_id = $2",
    )
    .bind(call_id)
    .bind(user(OWNER).as_ref())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(grant, "owner");

    let leg = repo.get_live_phone_leg(&call_id).await.unwrap().unwrap();
    assert_eq!(leg.direction, PhoneCallDirection::Outbound);
    assert_eq!(leg.status, PhoneCallStatus::Dialing);
    assert_eq!(leg.remote_number, number(CALLER));
    assert_eq!(leg.contact.as_ref().unwrap().contact_id, CONTACT_ID);

    let record = repo
        .get_call_record_by_call_id(&call_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.phone, Some(leg));
    assert!(!record.share_with_team);

    // A caller can only be in one call at a time.
    let error = repo
        .create_outbound_phone_call(new_call_for_owner_again())
        .await
        .unwrap_err();
    assert!(matches!(error, CallError::AlreadyInCall(_)), "{error:?}");
}

fn new_call_for_owner_again() -> NewPhoneCall {
    new_call(PhoneCallDirection::Outbound, None)
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn inbound_calls_are_created_once_per_room(pool: PgPool) {
    let repo = repo(pool).await;
    let room = "phone_+15552345678_abcd";

    let first = repo
        .create_inbound_phone_call(new_call(PhoneCallDirection::Inbound, Some(room)))
        .await
        .unwrap()
        .expect("first delivery creates the call");
    assert_eq!(first.room_name, room);
    assert!(
        !repo
            .is_participant(&first.id, user(OWNER).as_ref())
            .await
            .unwrap(),
        "nobody has answered yet"
    );
    assert!(
        repo.create_inbound_phone_call(new_call(PhoneCallDirection::Inbound, Some(room)))
            .await
            .unwrap()
            .is_none()
    );

    repo.archive_call(&first.id).await.unwrap();
    assert!(
        repo.create_inbound_phone_call(new_call(PhoneCallDirection::Inbound, Some(room)))
            .await
            .unwrap()
            .is_none(),
        "a late repeat must not resurrect an ended call"
    );
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn leg_updates_never_change_an_outcome(pool: PgPool) {
    let repo = repo(pool).await;
    let new_call = new_call(PhoneCallDirection::Outbound, None);
    let call_id = new_call.call_id;
    repo.create_outbound_phone_call(new_call).await.unwrap();

    let answered_at = Utc::now().trunc_subsecs(6);
    let answered = repo
        .update_phone_leg(
            &call_id,
            PhoneLegUpdate::answered(answered_at, Some("SCL_1".to_string())),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(answered.status, PhoneCallStatus::Active);
    assert_eq!(answered.answered_at, Some(answered_at));

    let ended_at = Utc::now().trunc_subsecs(6);
    let ended = repo
        .update_phone_leg(
            &call_id,
            PhoneLegUpdate::ended(PhoneCallStatus::Completed, ended_at),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ended.status, PhoneCallStatus::Completed);
    assert_eq!(ended.answered_at, Some(answered_at));
    assert_eq!(ended.ended_at, Some(ended_at));

    assert!(
        repo.update_phone_leg(
            &call_id,
            PhoneLegUpdate::ended(PhoneCallStatus::Busy, Utc::now())
        )
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(
        repo.get_live_phone_leg(&call_id)
            .await
            .unwrap()
            .unwrap()
            .status,
        PhoneCallStatus::Completed
    );
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn archiving_finalizes_the_leg_and_keeps_it_on_the_record(pool: PgPool) {
    let repo = repo(pool).await;
    let new_call = new_call(PhoneCallDirection::Inbound, Some("phone_room_missed"));
    let call_id = new_call.call_id;
    repo.create_inbound_phone_call(new_call).await.unwrap();

    repo.archive_call(&call_id).await.unwrap();

    assert_eq!(repo.get_live_phone_leg(&call_id).await.unwrap(), None);
    let record = repo
        .get_call_record_by_call_id(&call_id)
        .await
        .unwrap()
        .unwrap();
    assert!(!record.is_active);
    let leg = record.phone.expect("archived phone leg");
    assert_eq!(leg.status, PhoneCallStatus::Missed);
    assert!(leg.ended_at.is_some());
    assert_eq!(leg.contact.unwrap().name.as_deref(), Some("Ada Lovelace"));

    let people = repo.get_call_record_people(&call_id).await.unwrap();
    assert_eq!(people.phone_numbers, vec![number(CALLER)]);
    assert!(
        people
            .user_ids
            .iter()
            .any(|user_id| user_id.as_ref() == user(OWNER).as_ref()),
        "an unanswered phone call still belongs to its owner"
    );
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn call_lists_and_ringing_calls_carry_phone_legs(pool: PgPool) {
    let repo = repo(pool).await;
    let ringing = new_call(PhoneCallDirection::Inbound, Some("phone_room_ringing"));
    let ringing_id = ringing.call_id;
    repo.create_inbound_phone_call(ringing).await.unwrap();

    let records = repo
        .get_call_records_by_user(user(OWNER), 10, &None)
        .await
        .unwrap();
    let record = records
        .iter()
        .find(|record| record.call_id == ringing_id)
        .expect("the owner sees their phone call");
    assert_eq!(
        record.phone.as_ref().map(|leg| leg.status),
        Some(PhoneCallStatus::Ringing)
    );

    let incoming = repo.list_ringing_phone_calls(user(OWNER)).await.unwrap();
    assert_eq!(incoming.len(), 1);
    assert_eq!(incoming[0].call_id, ringing_id);
    assert_eq!(incoming[0].from, number(CALLER));
    assert_eq!(incoming[0].to, Some(number(OWNER_NUMBER)));
    assert!(
        repo.list_ringing_phone_calls(user(TEAMMATE))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        repo.get_call_records_by_user(user(TEAMMATE), 10, &None)
            .await
            .unwrap()
            .iter()
            .all(|record| record.call_id != ringing_id),
        "phone calls are private to their owner"
    );
}
