use super::recording_rules::{
    HOST, HOST_TEAM, expect_host_preferences, huddle_repo, kinds, not_a_direct_message, rules,
    service, teams,
};
use super::*;
use crate::domain::recording::CallKind::{ExternalMeeting, Huddle, OneOnOneMeeting};
use crate::domain::recording::{
    CallKinds, CallKindsPatch, CallPreferences, CallSettings, HuddleSharing, RecordingRules,
    TeamCallPolicy, UpdateCallSettingsRequest, UpdateTeamCallPolicyRequest,
};
use entity_access::domain::models::{AdminTeamRole, TeamRole};

fn sharing(share_by_default: bool, blocked: bool) -> CallPreferences {
    CallPreferences {
        huddle_sharing: HuddleSharing {
            share_by_default,
            blocked,
        },
        ..CallPreferences::default()
    }
}

#[tokio::test]
async fn huddles_start_shared_unless_their_host_or_team_says_otherwise() {
    for (preferences, shared) in [
        (CallPreferences::default(), true),
        (sharing(false, false), false),
        (sharing(true, true), false),
    ] {
        let mut repo = huddle_repo();
        expect_host_preferences(&mut repo, 1, preferences);
        not_a_direct_message(&mut repo);
        repo.expect_set_egress_id()
            .returning(|_, _| Box::pin(async { Ok(()) }));
        repo.expect_patch_call_record()
            .times(usize::from(!shared))
            .returning(|_, args| {
                assert_eq!(args.live_share_with_team, Some(false));
                assert!(args.team_share.is_none());
                Box::pin(async { Ok(()) })
            });
        service(repo, MockRtcClient::new(), teams())
            .get_or_create_call(&STARTED_EVENT_CHANNEL_ID, user(HOST))
            .await
            .unwrap();
    }
}

/// The live channel call's creator's settings, read without a team.
fn expect_creator_sharing(repo: &mut MockCallRepository, blocked: bool) {
    repo.expect_get_call_preferences()
        .times(1)
        .returning(move |host, team| {
            assert_eq!(host, user("creator@example.com"));
            assert_eq!(team, None);
            Box::pin(async move { Ok(sharing(true, blocked)) })
        });
}

fn live_record_shared(shared: bool) -> CallRecord {
    let mut record = call_record_for_mutation();
    record.is_active = true;
    record.share_with_team = shared;
    record
}

#[tokio::test]
async fn a_team_block_stops_anyone_turning_huddle_sharing_on() {
    let mut repo = MockCallRepository::new();
    repo.expect_get_call_record_by_call_id()
        .times(1)
        .returning(|_| Box::pin(async { Ok(Some(live_record_shared(false))) }));
    expect_creator_sharing(&mut repo, true);
    repo.expect_toggle_share_with_team().never();
    repo.expect_patch_call_record().never();
    let event_broker = RecordingEventBroker::default();
    let service = build_mutation_service(repo, event_broker.clone());
    assert!(matches!(
        service
            .toggle_share_with_team(authenticated_mutation_receipt())
            .await,
        Err(CallError::Forbidden(_))
    ));
    assert!(event_broker.events().is_empty());
}

#[tokio::test]
async fn a_team_block_still_lets_anyone_turn_huddle_sharing_off() {
    let mut repo = MockCallRepository::new();
    repo.expect_get_call_record_by_call_id()
        .times(1)
        .returning(|_| Box::pin(async { Ok(Some(live_record_shared(true))) }));
    expect_creator_sharing(&mut repo, true);
    repo.expect_toggle_share_with_team().never();
    repo.expect_patch_call_record()
        .times(1)
        .returning(|_, args| {
            assert_eq!(args.live_share_with_team, Some(false));
            Box::pin(async { Ok(()) })
        });
    repo.expect_get_participants()
        .times(1)
        .returning(|_| Box::pin(async { Ok(Vec::new()) }));
    let event_broker = RecordingEventBroker::default();
    let service = build_mutation_service(repo, event_broker.clone());
    assert!(
        !service
            .toggle_share_with_team(authenticated_mutation_receipt())
            .await
            .unwrap()
    );
    assert_updated_event(&event_broker, Some(MUTATED_EVENT_ACTOR), None, Some(false));
}

#[tokio::test]
async fn a_team_block_rejects_sharing_huddles_through_an_edit() {
    for (live, request) in [
        (
            true,
            team_edit(Some(team_share_request(Some(AccessLevel::View))), None),
        ),
        (true, team_edit(None, Some(true))),
        (
            false,
            team_edit(Some(team_share_request(Some(AccessLevel::View))), None),
        ),
    ] {
        let mut repo = MockCallRepository::new();
        repo.expect_get_call_record_by_call_id()
            .times(1)
            .returning(move |_| {
                let mut record = call_record_for_mutation();
                record.is_active = live;
                Box::pin(async move { Ok(Some(record)) })
            });
        expect_creator_sharing(&mut repo, true);
        repo.expect_patch_call_record().never();
        repo.expect_get_team_share_facts().never();
        let event_broker = RecordingEventBroker::default();
        let service = build_mutation_service(repo, event_broker.clone());
        assert!(matches!(
            service
                .edit_call_record(creator_mutation_receipt(), request)
                .await,
            Err(CallError::Forbidden(_))
        ));
        assert!(event_broker.events().is_empty());
    }
}

#[tokio::test]
async fn turning_huddle_sharing_off_needs_no_settings_lookup() {
    let mut repo = MockCallRepository::new();
    repo.expect_get_call_record_by_call_id()
        .times(1)
        .returning(|_| Box::pin(async { Ok(Some(live_record_shared(true))) }));
    repo.expect_get_call_preferences().never();
    repo.expect_patch_call_record()
        .times(1)
        .returning(|_, args| {
            assert_eq!(args.live_share_with_team, Some(false));
            Box::pin(async { Ok(()) })
        });
    repo.expect_get_participants()
        .times(1)
        .returning(|_| Box::pin(async { Ok(Vec::new()) }));
    build_mutation_service(repo, RecordingEventBroker::default())
        .edit_call_record(
            authenticated_mutation_receipt(),
            team_edit(Some(team_share_request(None)), None),
        )
        .await
        .unwrap();
}

fn settings_repo(team: Option<Uuid>, preferences: CallPreferences) -> MockCallRepository {
    let mut repo = MockCallRepository::new();
    repo.expect_get_call_preferences()
        .returning(move |_, requested| {
            assert_eq!(requested, team);
            Box::pin(async move { Ok(preferences) })
        });
    repo
}

#[tokio::test]
async fn settings_show_team_blocks_and_who_may_edit_them() {
    let preferences = CallPreferences {
        recording: rules(
            kinds(&[Huddle, OneOnOneMeeting, ExternalMeeting]),
            kinds(&[ExternalMeeting]),
        ),
        huddle_sharing: HuddleSharing {
            share_by_default: false,
            blocked: true,
        },
        refuses_one_on_one_recording: true,
    };
    for (role, can_edit) in [
        (TeamRole::Member, false),
        (TeamRole::Admin, true),
        (TeamRole::Owner, true),
    ] {
        let mut access = teams();
        access.roles.insert(user(HOST).to_string(), role);
        let settings = service(
            settings_repo(Some(HOST_TEAM), preferences),
            MockCallRtcClient::new(),
            access,
        )
        .get_call_settings(user(HOST))
        .await
        .unwrap();
        assert_eq!(
            settings,
            CallSettings {
                record_by_default: preferences.recording.record_by_default,
                share_huddles_by_default: false,
                refuse_one_on_one_recording: true,
                team: Some(TeamCallPolicy {
                    recording_blocked: preferences.recording.blocked,
                    huddle_sharing_blocked: true,
                    can_edit,
                }),
            }
        );
    }
}

#[tokio::test]
async fn settings_without_a_team_have_no_team_policy() {
    let settings = service(
        settings_repo(None, CallPreferences::default()),
        MockCallRtcClient::new(),
        super::meeting_invites::access::TeamAccessService::default(),
    )
    .get_call_settings(user(HOST))
    .await
    .unwrap();
    assert_eq!(settings.record_by_default, CallKinds::ALL);
    assert!(settings.share_huddles_by_default);
    assert!(!settings.refuse_one_on_one_recording);
    assert_eq!(settings.team, None);
}

#[tokio::test]
async fn people_change_only_their_own_settings() {
    let request = UpdateCallSettingsRequest {
        record_by_default: CallKindsPatch {
            huddles: Some(false),
            ..CallKindsPatch::default()
        },
        share_huddles_by_default: Some(false),
        refuse_one_on_one_recording: Some(true),
    };
    let mut repo = settings_repo(Some(HOST_TEAM), CallPreferences::default());
    repo.expect_update_call_preferences()
        .times(1)
        .returning(move |actor, requested| {
            assert_eq!(actor, user(HOST));
            assert_eq!(requested, request);
            Box::pin(async { Ok(()) })
        });
    service(repo, MockCallRtcClient::new(), teams())
        .update_call_settings(user(HOST), request)
        .await
        .unwrap();
}

fn admin_receipt() -> EntityAccessReceipt<AdminTeamRole> {
    EntityAccessReceipt::<AdminTeamRole>::dangerously_assert_authenticated_user(
        user(HOST),
        &HOST_TEAM.to_string(),
        EntityType::Team,
    )
}

#[tokio::test]
async fn team_policy_changes_the_receipts_team() {
    for (huddle_sharing_blocked, unshares) in
        [(Some(true), true), (Some(false), false), (None, false)]
    {
        let request = UpdateTeamCallPolicyRequest {
            recording_blocked: CallKindsPatch {
                external_meetings: Some(true),
                ..CallKindsPatch::default()
            },
            huddle_sharing_blocked,
        };
        let mut repo = settings_repo(Some(HOST_TEAM), CallPreferences::default());
        repo.expect_update_team_call_policy()
            .times(1)
            .returning(move |team, requested| {
                assert_eq!(*team, HOST_TEAM);
                assert_eq!(requested, request);
                Box::pin(async { Ok(()) })
            });
        // Blocking huddle sharing also unshares the team's live huddles.
        repo.expect_unshare_live_team_huddles()
            .times(usize::from(unshares))
            .returning(|team| {
                assert_eq!(*team, HOST_TEAM);
                Box::pin(async { Ok(()) })
            });
        service(repo, MockCallRtcClient::new(), teams())
            .update_team_call_policy(admin_receipt(), request)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn team_policy_needs_a_person_behind_the_receipt() {
    let mut repo = MockCallRepository::new();
    repo.expect_update_team_call_policy().never();
    let receipt = EntityAccessReceipt::<AdminTeamRole>::dangerously_assert_internal_user(
        &HOST_TEAM.to_string(),
        EntityType::Team,
    );
    let result = service(repo, MockCallRtcClient::new(), teams())
        .update_team_call_policy(receipt, UpdateTeamCallPolicyRequest::default())
        .await;
    assert!(matches!(result, Err(CallError::Forbidden(_))));
}

#[test]
fn untouched_preferences_record_everything() {
    assert_eq!(
        CallPreferences::default().recording,
        RecordingRules::default()
    );
}
