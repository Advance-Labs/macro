use super::*;
use crate::domain::{
    models::{CreateDraftInput, ResolvedDraftInput, UpsertedContacts},
    send_attempt::*,
};

fn actor() -> MacroUserIdStr<'static> {
    MacroUserIdStr::parse_from_str("macro|user1@test.com").unwrap()
}
fn link() -> Uuid {
    Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap()
}
fn message() -> Uuid {
    Uuid::parse_str("ee000002-0000-0000-0000-000000000002").unwrap()
}
fn snapshot() -> SendSnapshot {
    SendSnapshot {
        message: CreateDraftInput {
            db_id: Some(message()),
            thread_db_id: Some(Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()),
            provider_id: None,
            provider_thread_id: None,
            replying_to_id: None,
            subject: "Approved snapshot".into(),
            to: vec![],
            cc: vec![],
            bcc: vec![],
            body_text: Some("Approved body".into()),
            body_html: None,
            body_macro: None,
            headers_json: None,
            send_time: None,
            include_signature: None,
            actor: None,
            draft_client_binding: None,
            thread_client_binding: None,
        },
        attachment_ids: vec![],
        forwarded_attachment_ids: vec![],
        restore_body_html: None,
        restore_body_text: Some("Editable body".into()),
        restore_body_macro: None,
    }
}
fn prepared() -> PreparedSend {
    let snapshot = snapshot();
    let input = &snapshot.message;
    PreparedSend {
        undo_delay_secs: 5,
        message: ResolvedDraftInput {
            db_id: message(),
            thread_db_id: input.thread_db_id.unwrap(),
            provider_id: None,
            provider_thread_id: None,
            replying_to_id: None,
            subject: input.subject.clone(),
            to: vec![],
            cc: vec![],
            bcc: vec![],
            body_text: input.body_text.clone(),
            body_html: None,
            body_macro: None,
            headers_json: None,
            send_time: Some(Utc::now() + chrono::Duration::seconds(5)),
            actor_id: Some(actor().to_string()),
            draft_client_id: None,
            thread_client_id: None,
        },
        snapshot,
        contacts: UpsertedContacts {
            from_contact_id: None,
            recipients: vec![],
        },
        new_thread: None,
        restore_html: None,
    }
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../../fixtures", scripts("email_draft"))
)]
async fn concurrent_replay_preserves_one_message_and_undo_deadline(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = EmailPgRepo::new(pool.clone());
    let attempt = SendAttemptId(macro_uuid::generate_uuid_v7());
    let actor = actor();
    let (first, second) = tokio::join!(
        repo.admit_send(&actor, link(), attempt, prepared()),
        repo.admit_send(&actor, link(), attempt, prepared())
    );
    let first = first?;
    let second = second?;
    assert_eq!(first.message_id, second.message_id);
    assert_eq!(
        first.send_time.unwrap().timestamp_micros(),
        second.send_time.unwrap().timestamp_micros()
    );
    let mut changed = snapshot();
    changed.message.subject = "different".into();
    assert!(matches!(
        repo.read_send_attempt(&actor, link(), attempt, Some(&changed))
            .await,
        Err(EmailErr::SendAttemptConflict)
    ));
    assert!(matches!(
        repo.admit_send(
            &actor,
            link(),
            SendAttemptId(macro_uuid::generate_uuid_v7()),
            prepared()
        )
        .await,
        Err(EmailErr::MessageDeliveryConflict(_))
    ));
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../../fixtures", scripts("email_draft"))
)]
async fn cancellation_before_admission_is_a_durable_tombstone(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = EmailPgRepo::new(pool.clone());
    let attempt = SendAttemptId(macro_uuid::generate_uuid_v7());
    assert_eq!(
        repo.cancel_send(&actor(), link(), attempt).await?.status,
        SendAttemptStatus::Cancelled
    );
    assert_eq!(
        repo.admit_send(&actor(), link(), attempt, prepared())
            .await?
            .status,
        SendAttemptStatus::Cancelled
    );
    let scheduled = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM email_scheduled_messages WHERE message_id = $1",
        message()
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(scheduled, Some(0));
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../../fixtures", scripts("email_draft"))
)]
async fn cancellation_restores_body_and_cannot_cancel_a_later_resend(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = EmailPgRepo::new(pool.clone());
    let first = SendAttemptId(macro_uuid::generate_uuid_v7());
    repo.admit_send(&actor(), link(), first, prepared()).await?;
    assert_eq!(
        repo.cancel_send(&actor(), link(), first).await?.status,
        SendAttemptStatus::Cancelled
    );
    let restored = sqlx::query!(
        "SELECT is_draft, body_text FROM email_messages WHERE id = $1",
        message()
    )
    .fetch_one(&pool)
    .await?;
    assert!(restored.is_draft);
    assert_eq!(restored.body_text.as_deref(), Some("Editable body"));
    let second = SendAttemptId(macro_uuid::generate_uuid_v7());
    repo.admit_send(&actor(), link(), second, prepared())
        .await?;
    repo.cancel_send(&actor(), link(), first).await?;
    assert_eq!(
        repo.read_send_attempt(&actor(), link(), second, None)
            .await?
            .unwrap()
            .status,
        SendAttemptStatus::Accepted
    );
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../../fixtures", scripts("email_draft"))
)]
async fn claimed_delivery_cannot_be_cancelled(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let repo = EmailPgRepo::new(pool.clone());
    let attempt = SendAttemptId(macro_uuid::generate_uuid_v7());
    let mut input = prepared();
    input.undo_delay_secs = 0;
    repo.admit_send(&actor(), link(), attempt, input).await?;
    assert!(
        email_db_client::messages::scheduled::get::get_and_start_processing_scheduled_message(
            &pool,
            link(),
            message()
        )
        .await?
        .is_some()
    );
    assert_eq!(
        repo.cancel_send(&actor(), link(), attempt).await?.status,
        SendAttemptStatus::Sending
    );
    assert!(
        !sqlx::query_scalar!(
            "SELECT is_draft FROM email_messages WHERE id = $1",
            message()
        )
        .fetch_one(&pool)
        .await?
    );
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../../fixtures", scripts("email_draft"))
)]
async fn attachment_mismatch_rolls_back_admission(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let repo = EmailPgRepo::new(pool.clone());
    let attempt = SendAttemptId(macro_uuid::generate_uuid_v7());
    let mut input = prepared();
    input
        .snapshot
        .attachment_ids
        .push(macro_uuid::generate_uuid_v7());
    assert!(matches!(
        repo.admit_send(&actor(), link(), attempt, input).await,
        Err(EmailErr::InvalidSendSnapshot(_))
    ));
    assert!(
        repo.read_send_attempt(&actor(), link(), attempt, None)
            .await?
            .is_none()
    );
    assert!(
        sqlx::query_scalar!(
            "SELECT is_draft FROM email_messages WHERE id = $1",
            message()
        )
        .fetch_one(&pool)
        .await?
    );
    Ok(())
}

struct UnusedFrecency;
impl frecency::domain::ports::FrecencyQueryService for UnusedFrecency {
    async fn get_frecency_page<'a>(
        &self,
        _: frecency::domain::models::FrecencyPageRequest<'a>,
    ) -> Result<
        frecency::domain::models::FrecencyPageResponse,
        frecency::domain::models::FrecencyQueryErr,
    > {
        panic!("send does not query frecency")
    }
    async fn get_frecencies_by_ids<'a>(
        &self,
        _: frecency::domain::models::FrecencyByIdsRequest<'a>,
    ) -> Result<
        frecency::domain::models::FrecencyPageResponse,
        frecency::domain::models::FrecencyQueryErr,
    > {
        panic!("send does not query frecency")
    }
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../../fixtures", scripts("email_draft"))
)]
async fn service_authorizes_send_cancel_and_status_before_touching_attempts(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let service = crate::domain::service::EmailServiceImpl {
        email_repo: EmailPgRepo::new(pool.clone()),
        frecency_service: UnusedFrecency,
        enqueuer: crate::domain::ports::NoOpEnqueuer,
        crm_service: crm::domain::service::NoOpCrmService,
        entity_access_management_service: (),
        macro_event_broker: macro_event_broker::NoopMacroEventBroker,
        sent_undo_delay_secs: 5,
    };
    let attempt = SendAttemptId(macro_uuid::generate_uuid_v7());
    let outsider = MacroUserIdStr::parse_from_str("macro|user2@test.com")?;
    assert!(
        service
            .send_email(outsider.clone(), link(), attempt, snapshot())
            .await
            .is_err()
    );
    assert!(
        service
            .cancel_email_send(outsider.clone(), link(), attempt)
            .await
            .is_err()
    );
    assert!(
        service
            .email_send_status(outsider, link(), attempt)
            .await
            .is_err()
    );
    assert!(
        service
            .email_send_status(actor(), link(), attempt)
            .await?
            .is_none()
    );
    let mut input = snapshot();
    input.message.to.push(crate::domain::models::ContactInfo {
        email: "recipient@example.com".into(),
        name: None,
        photo_url: None,
    });
    let other_inbox = Uuid::parse_str("cccccccc-cccc-cccc-cccc-cccccccccccc")?;
    assert!(matches!(
        service
            .send_email(actor(), other_inbox, attempt, input.clone())
            .await,
        Err(EmailErr::InvalidSendSnapshot(_))
    ));
    let unchanged = sqlx::query!(
        "SELECT link_id, subject, is_draft FROM email_messages WHERE id = $1",
        message()
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(unchanged.link_id, link());
    assert_eq!(unchanged.subject.as_deref(), Some("Re: Hello World"));
    assert!(unchanged.is_draft);
    let admitted = service
        .send_email(actor(), link(), attempt, input.clone())
        .await?;
    assert_eq!(admitted.status, SendAttemptStatus::Accepted);
    assert!(admitted.message.is_some());
    let replay = service.send_email(actor(), link(), attempt, input).await?;
    assert_eq!(admitted.message_id, replay.message_id);
    assert_eq!(
        admitted.send_time.map(|t| t.timestamp_micros()),
        replay.send_time.map(|t| t.timestamp_micros())
    );
    assert_eq!(
        service
            .cancel_email_send(actor(), link(), attempt)
            .await?
            .status,
        SendAttemptStatus::Cancelled
    );
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../../fixtures", scripts("email_draft"))
)]
async fn attachment_edits_cannot_change_accepted_snapshot(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = EmailPgRepo::new(pool.clone());
    let attempt = SendAttemptId(macro_uuid::generate_uuid_v7());
    repo.admit_send(&actor(), link(), attempt, prepared())
        .await?;
    let attachment = models_email::service::attachment::AttachmentDraft {
        id: macro_uuid::generate_uuid_v7(),
        draft_id: message(),
        file_name: "file.txt".into(),
        content_type: "text/plain".into(),
        sha: "hash".into(),
        size: 1,
        s3_key: "file".into(),
    };
    assert!(
        email_db_client::attachments::draft::insert_draft_attachment(&pool, link(), attachment)
            .await
            .is_err()
    );
    assert_eq!(
        email_db_client::attachments::draft::get_total_attachments_size_by_draft_id(
            &pool,
            link(),
            message()
        )
        .await?,
        0
    );
    Ok(())
}
