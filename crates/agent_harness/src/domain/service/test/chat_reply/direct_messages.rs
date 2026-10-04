use super::*;
use crate::domain::model::ReplyPlacement;
use macro_db_migrator::MACRO_DB_MIGRATIONS;

fn dm_command() -> OpenSession {
    let mut command = chat_open_command();
    mention_origin_mut(&mut command).reply_placement = ReplyPlacement::Timeline;
    command
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn quota_denial_finishes_queued_dm_journal_entries_without_stopping_the_running_turn(
    pool: sqlx::PgPool,
) {
    use crate::domain::dm_turns::{DmTurnState, DmTurnStore};
    use agent_session::domain::agent_dm::AgentDmConversationRepo;
    use channels::domain::agent_dm::AgentDmRepo;

    let journal = Arc::new(crate::outbound::dm_turns::PgDmTurnStore::new(pool.clone()));
    let ((service, _, containers, announcer, _), _) = harness_with_ports_and_journal(
        PromptContextMock::default(),
        PromptComposerMock::default(),
        KindDefaultPolicies,
        HarnessDefaultCodingAgents,
        PromptMentionsMock::new(),
        Some(journal.clone()),
    );
    let mut command = dm_command();
    let channel = channels::outbound::pg_channels_repo::PgChannelsRepo::new(pool.clone())
        .ensure(mention_origin(&command).sender.clone(), command.bot_id)
        .await
        .unwrap()
        .dm
        .channel_id;
    let session = crate::testing::postgres_sessions(pool)
        .current_or_create(channel)
        .await
        .unwrap();
    mention_origin_mut(&mut command).parent = MessageParent::Channel(channel);
    let running_source = mention_origin(&command).message_id;
    let _container = open_dm(&service, &containers, session, command.clone()).await;
    let mut queued = Vec::new();
    for _ in 0..2 {
        let source = macro_uuid::generate_uuid_v7();
        mention_origin_mut(&mut command).message_id = source;
        queued.push(source);
        assert_eq!(
            service
                .execute(session, HarnessCommand::DirectMessage(command.clone()))
                .await
                .unwrap(),
            CommandOutcome::Queued,
        );
    }
    service
        .inner
        .reject_waiting_on_denial(
            session,
            &HarnessError::Admission(ai_billing::AiAdmissionError::Denied(
                ai_billing::DenyReason::AllowanceExhausted,
            )),
        )
        .await
        .unwrap();

    for source in queued {
        let record = journal.get(source).await.unwrap().unwrap();
        assert_eq!(record.state, DmTurnState::Failed);
        assert!(record.reply_finalized);
    }
    assert_eq!(
        journal.get(running_source).await.unwrap().unwrap().state,
        DmTurnState::Running
    );
    assert!(service.inner.busy.turn(session).is_some());
    assert!(service.inner.queues.snapshot(session).is_empty());
    assert!(
        service
            .inner
            .sessions
            .list_queued_actions(session)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        announcer.resolved().is_empty(),
        "the active reply is still running"
    );
}

async fn open_dm(
    service: &TestHarness,
    containers: &MockContainerManager,
    id: AgentSessionId,
    command: OpenSession,
) -> ContainerMock {
    let open = service.execute(id, HarnessCommand::DirectMessage(command));
    let drive = async {
        while containers.spawned() == 0 {
            tokio::task::yield_now().await;
        }
        let container = containers.container(session_of(containers)).unwrap();
        complete_session_handshake(&container).await;
        container.agent().wait_for_requests(3).await;
        container
    };
    let (result, container) = tokio::join!(open, drive);
    result.expect("the reserved DM session opens");
    container
}

#[tokio::test]
async fn direct_messages_share_one_session_queue_in_order_and_answer_in_the_timeline() {
    let ((service, sessions, containers, announcer, _), turns) =
        harness_with_signals(PromptContextMock::default(), PromptComposerMock::default());
    let id = AgentSessionId::new();
    let first = dm_command();
    let container = open_dm(&service, &containers, id, first.clone()).await;
    let agent = container.agent();
    let mut next = first.clone();
    mention_origin_mut(&mut next).message_id = macro_uuid::generate_uuid_v7();
    mention_origin_mut(&mut next).thread_id = mention_origin(&next).message_id;
    mention_origin_mut(&mut next).content = "Follow up".to_owned();
    let outcome = service
        .execute(id, HarnessCommand::DirectMessage(next))
        .await
        .unwrap();
    assert_eq!(outcome, CommandOutcome::Queued);
    assert_eq!(containers.spawned(), 1);
    assert!(
        agent.received_notifications().is_empty(),
        "a DM follow-up must not cancel the current turn"
    );
    assert_eq!(
        announcer.announced().len(),
        1,
        "the second answer waits for its turn"
    );
    assert_eq!(
        announcer.announced()[0].reply_placement,
        ReplyPlacement::Timeline
    );
    assert!(
        sessions.get(id).await.unwrap().thread_id.is_none(),
        "the whole DM owns the conversation, not its first message's thread"
    );
    says(&agent, "First answer.");
    agent.completes_prompt().await;
    turns.lifecycle_published(4).await;
    assert_eq!(announcer.announced().len(), 2);
    says(&agent, "Second answer.");
    agent.completes_prompt().await;
    turns.lifecycle_published(6).await;
    assert_eq!(announcer.resolved().len(), 2);
    assert!(
        announcer
            .announced()
            .iter()
            .all(|reply| reply.reply_placement == ReplyPlacement::Timeline && !reply.is_coding)
    );
}

#[tokio::test]
async fn a_different_person_cannot_prompt_the_reserved_dm_session() {
    let ((service, _, containers, announcer, _), _) =
        harness_with_signals(PromptContextMock::default(), PromptComposerMock::default());
    let id = AgentSessionId::new();
    let mut command = dm_command();
    let _container = open_dm(&service, &containers, id, command.clone()).await;
    mention_origin_mut(&mut command).sender = staff_sender();
    mention_origin_mut(&mut command).message_id = macro_uuid::generate_uuid_v7();
    assert!(matches!(
        service
            .execute(id, HarnessCommand::DirectMessage(command))
            .await,
        Err(HarnessError::Session(AgentSessionError::Forbidden))
    ));
    assert_eq!(announcer.announced().len(), 1);
}

#[tokio::test]
async fn an_external_persona_dm_binds_the_connected_runtime_without_spawning_a_container() {
    let (service, repo, containers, announcer, runtimes) = harness();
    let id = AgentSessionId::new();
    let mut command = dm_command();
    command.bot_id = BotId::new_from_uuid(macro_uuid::generate_uuid_v7());
    command.runtime.kind = AgentKind::External;
    command.runtime.harness = "external".to_owned();
    let runtime = ContainerMock::default();
    runtimes.attach(harness_for_bot(command.bot_id), runtime.clone());
    let open = service.execute(id, HarnessCommand::DirectMessage(command.clone()));
    let (result, ()) = tokio::join!(open, complete_bound_handshake(&runtime));
    result.expect("a DM delivers its first prompt to the connected persona");
    assert_eq!(containers.spawned(), 0);
    assert_eq!(repo.get(id).await.unwrap().harness, "external");
    assert_eq!(prompts(&runtime.agent()).len(), 1);
    assert_eq!(announcer.announced().len(), 1);
    assert_eq!(
        announcer.announced()[0].reply_placement,
        ReplyPlacement::Timeline
    );

    mention_origin_mut(&mut command).message_id = macro_uuid::generate_uuid_v7();
    assert_eq!(
        service
            .execute(id, HarnessCommand::DirectMessage(command))
            .await
            .unwrap(),
        CommandOutcome::Queued
    );
    assert!(runtime.agent().received_notifications().is_empty());
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn a_completed_dm_message_never_runs_again_when_the_broker_replays_it(pool: sqlx::PgPool) {
    use crate::domain::dm_turns::{DmTurnState, DmTurnStore};
    use agent_session::domain::agent_dm::AgentDmConversationRepo;
    use channels::domain::agent_dm::AgentDmRepo;
    let journal = Arc::new(crate::outbound::dm_turns::PgDmTurnStore::new(pool.clone()));
    let ((service, _, containers, announcer, _), signals) = harness_with_ports_and_journal(
        PromptContextMock::default(),
        PromptComposerMock::default(),
        KindDefaultPolicies,
        HarnessDefaultCodingAgents,
        PromptMentionsMock::new(),
        Some(journal.clone()),
    );
    let mut command = dm_command();
    let channel = channels::outbound::pg_channels_repo::PgChannelsRepo::new(pool.clone())
        .ensure(mention_origin(&command).sender.clone(), command.bot_id)
        .await
        .unwrap()
        .dm
        .channel_id;
    let session = crate::testing::postgres_sessions(pool)
        .current_or_create(channel)
        .await
        .unwrap();
    mention_origin_mut(&mut command).parent = MessageParent::Channel(channel);
    let source = mention_origin(&command).message_id;
    let container = open_dm(&service, &containers, session, command.clone()).await;
    says(&container.agent(), "Recorded answer");
    container.agent().completes_prompt().await;
    signals.lifecycle_published(4).await;
    let completed = journal.get(source).await.unwrap().unwrap();
    assert_eq!(completed.state, DmTurnState::Succeeded);
    assert!(completed.reply_finalized);
    assert_eq!(announcer.resolved().len(), 1);
    assert_eq!(
        service
            .execute(session, HarnessCommand::DirectMessage(command))
            .await
            .unwrap(),
        CommandOutcome::Completed
    );
    service.recover_direct_messages().await.unwrap();
    assert_eq!(prompts(&container.agent()).len(), 1);
    assert_eq!(announcer.announced().len(), 1);
    assert_eq!(announcer.resolved().len(), 1);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn recovered_messages_dispatch_in_durable_order_even_when_delivered_in_reverse(
    pool: sqlx::PgPool,
) {
    use crate::domain::dm_turns::DmTurnStore;
    use agent_session::domain::agent_dm::AgentDmConversationRepo;
    use channels::domain::agent_dm::AgentDmRepo;
    let journal = Arc::new(crate::outbound::dm_turns::PgDmTurnStore::new(pool.clone()));
    let ((service, _, containers, announcer, _), signals) = harness_with_ports_and_journal(
        PromptContextMock::default(),
        PromptComposerMock::default(),
        KindDefaultPolicies,
        HarnessDefaultCodingAgents,
        PromptMentionsMock::new(),
        Some(journal.clone()),
    );
    let mut first = dm_command();
    let channel = channels::outbound::pg_channels_repo::PgChannelsRepo::new(pool.clone())
        .ensure(mention_origin(&first).sender.clone(), first.bot_id)
        .await
        .unwrap()
        .dm
        .channel_id;
    let session = crate::testing::postgres_sessions(pool)
        .current_or_create(channel)
        .await
        .unwrap();
    mention_origin_mut(&mut first).parent = MessageParent::Channel(channel);
    mention_origin_mut(&mut first).content = "First admitted message".into();
    let mut second = first.clone();
    mention_origin_mut(&mut second).message_id = macro_uuid::generate_uuid_v7();
    mention_origin_mut(&mut second).content = "Second admitted message".into();
    journal
        .admit(session, channel, first.clone())
        .await
        .unwrap();
    journal
        .admit(session, channel, second.clone())
        .await
        .unwrap();

    let open = service.execute(session, HarnessCommand::DirectMessage(second.clone()));
    let drive = async {
        while containers.spawned() == 0 {
            tokio::task::yield_now().await;
        }
        let container = containers.container(session_of(&containers)).unwrap();
        complete_session_handshake(&container).await;
        container
    };
    let (opened, container) = tokio::join!(open, drive);
    opened.unwrap();
    assert!(
        prompts(&container.agent()).is_empty(),
        "the later arrival waits for the older durable admission"
    );
    service
        .execute(session, HarnessCommand::DirectMessage(first.clone()))
        .await
        .unwrap();
    container.agent().wait_for_requests(3).await;
    assert_eq!(
        announcer.announced()[0].origin_message_id,
        mention_origin(&first).message_id
    );
    container.agent().completes_prompt().await;
    signals.lifecycle_published(4).await;
    assert_eq!(
        announcer.announced()[1].origin_message_id,
        mention_origin(&second).message_id
    );
    assert_eq!(containers.spawned(), 1);
}
