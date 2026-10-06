use super::meeting_invites::access::TeamAccessService;
use super::*;
use crate::domain::meetings::Meeting;
use crate::domain::recording::CallKind::{
    self, ExternalMeeting, Huddle, InternalMeeting, OneOnOneMeeting,
};
use crate::domain::recording::{
    CallKinds, CallKindsPatch, CallRecordingSettings, MeetingAttendance, MeetingKindChange,
    RecordingRules, TeamRecordingPolicy, UpdateRecordingDefaultsRequest,
    UpdateTeamRecordingPolicyRequest,
};
use crate::domain::service::recording::MeetingJoiner;
use entity_access::domain::models::{AdminTeamRole, TeamRole};
use tokio::sync::oneshot;

const HOST: &str = "host@example.com";
const TEAMMATE: &str = "teammate@example.com";
const OUTSIDER: &str = "outsider@example.com";
const HOST_TEAM: Uuid = Uuid::from_u128(0x0198a1b2_c3d4_7e5f_8061_000000000001);
const OTHER_TEAM: Uuid = Uuid::from_u128(0x0198a1b2_c3d4_7e5f_8061_000000000002);
const EGRESS: &str = "meeting-egress";

fn egress_config() -> EgressS3Config {
    EgressS3Config {
        bucket: "recordings".into(),
        region: "us-east-1".into(),
        access_key: "access-key".into(),
        secret: "secret".into(),
    }
}

/// The host and a teammate share a team; the outsider is on another one.
fn teams() -> TeamAccessService {
    TeamAccessService {
        teams: [
            (user(HOST).to_string(), HOST_TEAM),
            (user(TEAMMATE).to_string(), HOST_TEAM),
            (user(OUTSIDER).to_string(), OTHER_TEAM),
        ]
        .into(),
        ..TeamAccessService::default()
    }
}

fn rules(record_by_default: CallKinds, blocked: CallKinds) -> RecordingRules {
    RecordingRules {
        record_by_default,
        blocked,
    }
}

/// Exactly the kinds in `only`.
fn kinds(only: &[CallKind]) -> CallKinds {
    CallKinds {
        huddles: only.contains(&Huddle),
        one_on_one_meetings: only.contains(&OneOnOneMeeting),
        internal_meetings: only.contains(&InternalMeeting),
        external_meetings: only.contains(&ExternalMeeting),
    }
}

/// Every recording-rules read must name the host and the host's team.
fn expect_host_rules(repo: &mut MockCallRepository, times: usize, rules: RecordingRules) {
    repo.expect_get_recording_rules()
        .times(times)
        .returning(move |host, team| {
            assert_eq!(host, user(HOST));
            assert_eq!(team, Some(HOST_TEAM));
            Box::pin(async move { Ok(rules) })
        });
}

fn service<R: CallRtcClient>(
    repo: MockCallRepository,
    rtc: R,
    access: TeamAccessService,
) -> CallServiceImpl<
    MockCallRepository,
    R,
    StubConnectionService,
    TeamAccessService,
    StubNotificationIngress,
    StubRecordingStorage,
    NoopCallSummarizer,
> {
    CallServiceImpl::new(
        repo,
        rtc,
        StubConnectionService,
        access,
        StubNotificationIngress,
        StubRecordingStorage,
        "wss://livekit.example.com",
    )
    .with_egress(egress_config())
}

fn huddle(created_by: &str) -> Call {
    started_event_call(created_by)
}

fn huddle_repo() -> MockCallRepository {
    let mut repo = MockCallRepository::new();
    repo.expect_get_call_by_channel_id()
        .times(1)
        .returning(|_| Box::pin(async { Ok(None) }));
    let call = huddle(user(HOST).as_ref());
    repo.expect_create_call()
        .times(1)
        .return_once(move |_, _, _, _| Box::pin(async move { Ok(Some(call)) }));
    repo.expect_find_active_call_for_user()
        .returning(|_| Box::pin(async { Ok(None) }));
    repo.expect_add_participant().returning(|call_id, user_id| {
        let participant = CallParticipant {
            call_id: *call_id,
            user_id: user_id.as_ref().to_string(),
            joined_at: started_event_timestamp(),
        };
        Box::pin(async move { Ok(participant) })
    });
    repo
}

fn started_recording_flag(broker: &RecordingEventBroker) -> bool {
    let events = broker.events();
    let [started] = events.as_slice() else {
        panic!("expected exactly one call event, got {events:?}");
    };
    started.envelope["metadata"]["recording_enabled"]
        .as_bool()
        .expect("recording_enabled is a bool")
}

#[tokio::test]
async fn huddles_record_only_when_the_starter_records_them_and_the_team_allows_it() {
    for (rules, records) in [
        (RecordingRules::default(), true),
        (
            rules(
                kinds(&[OneOnOneMeeting, InternalMeeting, ExternalMeeting]),
                CallKinds::NONE,
            ),
            false,
        ),
        (rules(CallKinds::ALL, kinds(&[Huddle])), false),
    ] {
        let mut repo = huddle_repo();
        expect_host_rules(&mut repo, 1, rules);
        // The mock rejects any recorder attachment the rules did not allow.
        repo.expect_set_egress_id()
            .times(usize::from(records))
            .returning(|_, _| Box::pin(async { Ok(()) }));
        let broker = RecordingEventBroker::default();
        service(repo, MockRtcClient::new(), teams())
            .with_event_broker(broker.clone())
            .get_or_create_call(&STARTED_EVENT_CHANNEL_ID, user(HOST))
            .await
            .unwrap();
        assert_eq!(started_recording_flag(&broker), records);
    }
}

#[tokio::test]
async fn unreadable_rules_never_record() {
    let mut repo = huddle_repo();
    repo.expect_get_recording_rules()
        .times(1)
        .returning(|_, _| Box::pin(async { Err(CallError::Internal(anyhow::anyhow!("db down"))) }));
    repo.expect_set_egress_id().never();
    let broker = RecordingEventBroker::default();
    service(repo, MockRtcClient::new(), teams())
        .with_event_broker(broker.clone())
        .get_or_create_call(&STARTED_EVENT_CHANNEL_ID, user(HOST))
        .await
        .unwrap();
    assert!(!started_recording_flag(&broker));
}

fn hosted_meeting(call_id: Option<Uuid>) -> Meeting {
    let mut meeting = invitation_for_test();
    meeting.user_id = user(HOST).to_string();
    meeting.call_id = call_id;
    meeting
}

fn live_meeting_call(egress_id: Option<&str>) -> Call {
    let mut call = active_call_for_archived_event(user(HOST).as_ref(), egress_id);
    call.channel_id = None;
    call
}

fn attendance(more_than_two: bool, external: bool) -> MeetingAttendance {
    MeetingAttendance {
        more_than_two,
        external,
    }
}

/// A recorder that attaches to a live meeting with `seen` attendance, and
/// signals once it has re-checked that attendance.
fn attaching_recorder(seen: MeetingAttendance) -> (MockCallRepository, oneshot::Receiver<()>) {
    let (checked, check) = oneshot::channel();
    let mut repo = MockCallRepository::new();
    repo.expect_attach_meeting_recording()
        .times(1)
        .returning(|_, egress| {
            assert_eq!(egress, EGRESS);
            Box::pin(async { Ok(true) })
        });
    repo.expect_get_meeting_attendance()
        .times(1)
        .return_once(move |_| {
            checked.send(()).unwrap();
            Box::pin(async move { Ok(Some(seen)) })
        });
    (repo, check)
}

#[tokio::test]
async fn a_meeting_starts_recording_only_for_the_kind_it_starts_as() {
    for (joiner, records, recording) in [
        (
            MeetingJoiner::Account(user(HOST)),
            kinds(&[OneOnOneMeeting]),
            true,
        ),
        (
            MeetingJoiner::Account(user(TEAMMATE)),
            kinds(&[InternalMeeting, ExternalMeeting]),
            false,
        ),
        (MeetingJoiner::Guest, kinds(&[ExternalMeeting]), true),
        (MeetingJoiner::Guest, kinds(&[OneOnOneMeeting]), false),
    ] {
        let guest = matches!(joiner, MeetingJoiner::Guest);
        let call = live_meeting_call(None);
        let call_id = call.id;
        let mut sequence = mockall::Sequence::new();
        let mut repo = MockCallRepository::new();
        repo.expect_get_meeting_preparation()
            .returning(|_| Box::pin(async { Ok(None) }));
        repo.expect_get_or_create_meeting_call()
            .times(1)
            .return_once(move |_, _| Box::pin(async move { Ok((call, true)) }));
        // A guest makes the call external before the recorder decides.
        if guest {
            repo.expect_mark_call_external()
                .times(1)
                .in_sequence(&mut sequence)
                .returning(move |id| {
                    assert_eq!(*id, call_id);
                    Box::pin(async { Ok(Some(MeetingKindChange { egress_id: None })) })
                });
        } else {
            repo.expect_mark_call_external().never();
        }
        let host_rules = rules(records, CallKinds::NONE);
        repo.expect_get_recording_rules()
            .times(1)
            .in_sequence(&mut sequence)
            .returning(move |_, team| {
                assert_eq!(team, Some(HOST_TEAM));
                Box::pin(async move { Ok(host_rules) })
            });
        if recording {
            repo.expect_claim_meeting_recorder()
                .times(1)
                .in_sequence(&mut sequence)
                .returning(|_| Box::pin(async { Ok(true) }));
        } else {
            repo.expect_claim_meeting_recorder().never();
        }
        let (started, recorder) = oneshot::channel();
        let mut rtc = MockCallRtcClient::new();
        rtc.expect_create_room()
            .returning(|_| Box::pin(async { Ok(()) }));
        rtc.expect_dispatch_transcription_agent()
            .returning(|_| Box::pin(async { Ok(()) }));
        let mut started = Some(started);
        rtc.expect_start_room_composite_egress()
            .times(usize::from(recording))
            .returning(move |_, _| {
                started.take().unwrap().send(()).unwrap();
                Box::pin(async { Err(anyhow::anyhow!("recorder unavailable")) })
            });
        let broker = RecordingEventBroker::default();
        let service = service(repo, rtc, teams()).with_event_broker(broker.clone());
        service
            .prepare_meeting_call(&hosted_meeting(None), joiner)
            .await
            .unwrap();
        assert_eq!(started_recording_flag(&broker), recording);
        if recording {
            tokio::time::timeout(Duration::from_secs(2), recorder)
                .await
                .unwrap()
                .unwrap();
        } else {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

/// How a join finds one of a live meeting's attendance flags.
#[derive(Clone, Copy)]
enum Flag {
    /// The join never checks it.
    Untouched,
    /// The join checks it and leaves it as it was.
    Unchanged,
    /// This join flips it while `egress` is attached.
    Flips { egress: Option<&'static str> },
}

impl Flag {
    fn change(self) -> Option<MeetingKindChange> {
        match self {
            Flag::Untouched | Flag::Unchanged => None,
            Flag::Flips { egress } => Some(MeetingKindChange {
                egress_id: egress.map(str::to_string),
            }),
        }
    }

    fn times(self) -> usize {
        usize::from(!matches!(self, Flag::Untouched))
    }
}

/// A join to a live standalone session hosted by [`HOST`] with [`EGRESS`]
/// attached. `external` is checked before credentials are minted and
/// `more_than_two` after the joiner is recorded as a participant.
fn join_live_meeting(
    external: Flag,
    more_than_two: Flag,
    host_rules: Option<RecordingRules>,
) -> MockCallRepository {
    let call = live_meeting_call(Some(EGRESS));
    let call_id = call.id;
    let meeting = hosted_meeting(Some(call_id));
    let mut repo = MockCallRepository::new();
    repo.expect_get_meeting()
        .return_once(move |_| Box::pin(async move { Ok(Some(meeting)) }));
    repo.expect_get_call_by_id()
        .times(1)
        .return_once(move |_| Box::pin(async move { Ok(Some(call)) }));
    repo.expect_mark_call_external()
        .times(external.times())
        .returning(move |id| {
            assert_eq!(*id, call_id);
            let change = external.change();
            Box::pin(async move { Ok(change) })
        });
    let mut sequence = mockall::Sequence::new();
    repo.expect_add_meeting_participant()
        .times(1)
        .in_sequence(&mut sequence)
        .returning(|call_id, user_id| {
            let participant = CallParticipant {
                call_id: *call_id,
                user_id: user_id.as_ref().to_string(),
                joined_at: started_event_timestamp(),
            };
            Box::pin(async move { Ok(participant) })
        });
    if more_than_two.times() == 0 {
        repo.expect_mark_call_more_than_two().never();
    } else {
        repo.expect_mark_call_more_than_two()
            .times(1)
            .in_sequence(&mut sequence)
            .returning(move |id| {
                assert_eq!(*id, call_id);
                let change = more_than_two.change();
                Box::pin(async move { Ok(change) })
            });
    }
    match host_rules {
        Some(host_rules) => expect_host_rules(&mut repo, 1, host_rules),
        None => {
            repo.expect_get_recording_rules().never();
        }
    }
    repo.expect_find_active_call_for_user()
        .returning(|_| Box::pin(async { Ok(None) }));
    repo
}

fn token_minting_rtc() -> MockCallRtcClient {
    let mut rtc = MockCallRtcClient::new();
    rtc.expect_generate_token()
        .returning(|_, _| Box::pin(async { Ok("token".to_string()) }));
    rtc
}

#[tokio::test]
async fn an_outsider_joining_stops_a_recorder_the_host_does_not_keep_for_external_meetings() {
    let repo = join_live_meeting(
        Flag::Flips {
            egress: Some(EGRESS),
        },
        Flag::Unchanged,
        Some(rules(CallKinds::ALL, kinds(&[ExternalMeeting]))),
    );
    let mut sequence = mockall::Sequence::new();
    let mut rtc = MockCallRtcClient::new();
    rtc.expect_stop_egress()
        .with(mockall::predicate::eq(EGRESS))
        .times(1)
        .in_sequence(&mut sequence)
        .returning(|_| Box::pin(async { Ok(()) }));
    // Credentials are minted only after the recorder was told to stop.
    rtc.expect_generate_token()
        .times(1)
        .in_sequence(&mut sequence)
        .returning(|_, _| Box::pin(async { Ok("token".to_string()) }));
    service(repo, rtc, teams())
        .join_meeting(hosted_meeting(None).share_token, user(OUTSIDER))
        .await
        .unwrap();
}

#[tokio::test]
async fn a_third_teammate_joining_stops_a_recorder_the_host_keeps_only_for_one_on_ones() {
    let mut repo = join_live_meeting(
        Flag::Untouched,
        Flag::Flips {
            egress: Some(EGRESS),
        },
        Some(rules(
            kinds(&[OneOnOneMeeting, ExternalMeeting]),
            CallKinds::NONE,
        )),
    );
    repo.expect_claim_meeting_recorder().never();
    let mut rtc = token_minting_rtc();
    rtc.expect_stop_egress()
        .with(mockall::predicate::eq(EGRESS))
        .times(1)
        .returning(|_| Box::pin(async { Ok(()) }));
    rtc.expect_start_room_composite_egress().never();
    // The stop is awaited, so it has happened by the time the join answers.
    service(repo, rtc, teams())
        .join_meeting(hosted_meeting(None).share_token, user(TEAMMATE))
        .await
        .unwrap();
}

#[tokio::test]
async fn a_team_block_on_internal_meetings_stops_a_one_on_one_recorder_when_it_grows() {
    let mut repo = join_live_meeting(
        Flag::Untouched,
        Flag::Flips {
            egress: Some(EGRESS),
        },
        Some(rules(CallKinds::ALL, kinds(&[InternalMeeting]))),
    );
    repo.expect_claim_meeting_recorder().never();
    let mut rtc = token_minting_rtc();
    rtc.expect_stop_egress()
        .times(1)
        .returning(|_| Box::pin(async { Ok(()) }));
    service(repo, rtc, teams())
        .join_meeting(hosted_meeting(None).share_token, user(TEAMMATE))
        .await
        .unwrap();
}

#[tokio::test]
async fn a_change_to_a_recorded_kind_keeps_the_running_recorder() {
    for (external, more_than_two, joiner) in [
        (
            Flag::Flips {
                egress: Some(EGRESS),
            },
            Flag::Unchanged,
            OUTSIDER,
        ),
        (
            Flag::Untouched,
            Flag::Flips {
                egress: Some(EGRESS),
            },
            TEAMMATE,
        ),
    ] {
        let mut repo = join_live_meeting(external, more_than_two, Some(RecordingRules::default()));
        // The recorder that started with the call holds the claim.
        repo.expect_claim_meeting_recorder()
            .times(1)
            .returning(|_| Box::pin(async { Ok(false) }));
        let mut rtc = token_minting_rtc();
        rtc.expect_stop_egress().never();
        rtc.expect_start_room_composite_egress().never();
        service(repo, rtc, teams())
            .join_meeting(hosted_meeting(None).share_token, user(joiner))
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn joins_that_change_nothing_do_not_reapply_rules() {
    for (joiner, external) in [(TEAMMATE, Flag::Untouched), (OUTSIDER, Flag::Unchanged)] {
        let repo = join_live_meeting(external, Flag::Unchanged, None);
        let mut rtc = token_minting_rtc();
        rtc.expect_stop_egress().never();
        service(repo, rtc, teams())
            .join_meeting(hosted_meeting(None).share_token, user(joiner))
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn a_change_to_a_kind_recorded_on_its_own_starts_a_recorder() {
    for (external, more_than_two, joiner, records, seen) in [
        (
            Flag::Flips { egress: None },
            Flag::Unchanged,
            OUTSIDER,
            kinds(&[Huddle, ExternalMeeting]),
            attendance(false, true),
        ),
        (
            Flag::Untouched,
            Flag::Flips { egress: None },
            TEAMMATE,
            kinds(&[InternalMeeting]),
            attendance(true, false),
        ),
    ] {
        let mut repo = join_live_meeting(
            external,
            more_than_two,
            Some(rules(records, CallKinds::NONE)),
        );
        repo.expect_claim_meeting_recorder()
            .times(1)
            .returning(|_| Box::pin(async { Ok(true) }));
        let (background, check) = attaching_recorder(seen);
        let mut rtc = token_minting_rtc();
        rtc.expect_start_room_composite_egress()
            .times(1)
            .returning(|_, _| Box::pin(async { Ok(EGRESS.to_string()) }));
        // Nothing the session has been since then is unrecorded.
        rtc.expect_stop_egress().never();
        let service = service(repo, rtc, teams());
        configure_repository_clone(&service.repo, background);
        service
            .join_meeting(hosted_meeting(None).share_token, user(joiner))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), check)
            .await
            .unwrap()
            .unwrap();
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn a_stopped_recording_is_not_resumed() {
    // Recorded as a one-on-one, stopped when it grew, then joined by an
    // outsider: external meetings record, but the call already used its claim.
    let mut repo = join_live_meeting(
        Flag::Flips {
            egress: Some(EGRESS),
        },
        Flag::Unchanged,
        Some(rules(
            kinds(&[OneOnOneMeeting, ExternalMeeting]),
            CallKinds::NONE,
        )),
    );
    repo.expect_claim_meeting_recorder()
        .times(1)
        .returning(|_| Box::pin(async { Ok(false) }));
    let mut rtc = token_minting_rtc();
    rtc.expect_start_room_composite_egress().never();
    rtc.expect_stop_egress().never();
    service(repo, rtc, teams())
        .join_meeting(hosted_meeting(None).share_token, user(OUTSIDER))
        .await
        .unwrap();
}

#[tokio::test]
async fn a_failed_participant_count_turns_the_join_away() {
    let call = live_meeting_call(Some(EGRESS));
    let call_id = call.id;
    let meeting = hosted_meeting(Some(call_id));
    let mut repo = MockCallRepository::new();
    repo.expect_get_meeting()
        .return_once(move |_| Box::pin(async move { Ok(Some(meeting)) }));
    repo.expect_get_call_by_id()
        .return_once(move |_| Box::pin(async move { Ok(Some(call)) }));
    repo.expect_find_active_call_for_user()
        .returning(|_| Box::pin(async { Ok(None) }));
    repo.expect_add_meeting_participant()
        .times(1)
        .returning(|call_id, user_id| {
            let participant = CallParticipant {
                call_id: *call_id,
                user_id: user_id.as_ref().to_string(),
                joined_at: started_event_timestamp(),
            };
            Box::pin(async move { Ok(participant) })
        });
    repo.expect_mark_call_more_than_two()
        .times(1)
        .returning(|_| Box::pin(async { Err(CallError::Internal(anyhow::anyhow!("db down"))) }));
    repo.expect_remove_participant()
        .times(1)
        .returning(move |id, user_id| {
            assert_eq!(*id, call_id);
            assert_eq!(user_id, user(TEAMMATE));
            Box::pin(async { Ok(()) })
        });
    let result = service(repo, token_minting_rtc(), teams())
        .join_meeting(hosted_meeting(None).share_token, user(TEAMMATE))
        .await;
    assert!(matches!(result, Err(CallError::Internal(_))));
}

#[tokio::test]
async fn an_attaching_recorder_stops_if_the_call_became_an_unrecorded_kind() {
    let internal_only = kinds(&[OneOnOneMeeting, InternalMeeting]);
    let one_on_ones_only = kinds(&[OneOnOneMeeting]);
    let skips_internal = kinds(&[OneOnOneMeeting, ExternalMeeting]);
    for (records, since, seen, stops) in [
        (
            internal_only,
            OneOnOneMeeting,
            attendance(false, false),
            false,
        ),
        (
            internal_only,
            OneOnOneMeeting,
            attendance(true, false),
            false,
        ),
        (
            internal_only,
            OneOnOneMeeting,
            attendance(false, true),
            true,
        ),
        (
            one_on_ones_only,
            OneOnOneMeeting,
            attendance(true, false),
            true,
        ),
        // It recorded while the call was internal, though it is external now.
        (
            skips_internal,
            OneOnOneMeeting,
            attendance(true, true),
            true,
        ),
        (
            skips_internal,
            OneOnOneMeeting,
            attendance(false, true),
            false,
        ),
        (
            kinds(&[InternalMeeting]),
            InternalMeeting,
            attendance(true, false),
            false,
        ),
        (
            kinds(&[ExternalMeeting]),
            ExternalMeeting,
            attendance(true, true),
            false,
        ),
    ] {
        let mut repo = MockCallRepository::new();
        repo.expect_attach_meeting_recording()
            .returning(|_, _| Box::pin(async { Ok(true) }));
        repo.expect_get_meeting_attendance()
            .times(1)
            .returning(move |_| Box::pin(async move { Ok(Some(seen)) }));
        let mut rtc = MockCallRtcClient::new();
        rtc.expect_start_room_composite_egress()
            .returning(|_, _| Box::pin(async { Ok(EGRESS.to_string()) }));
        rtc.expect_stop_egress()
            .times(usize::from(stops))
            .returning(|_| Box::pin(async { Ok(()) }));
        crate::domain::service::meetings::start_meeting_recording(
            &repo,
            &rtc,
            ARCHIVED_EVENT_CALL_ID,
            "room",
            Some(&egress_config()),
            rules(records, CallKinds::NONE),
            since,
        )
        .await;
    }
}

#[tokio::test]
async fn an_unverifiable_audience_stops_an_attaching_recorder() {
    let mut repo = MockCallRepository::new();
    repo.expect_attach_meeting_recording()
        .returning(|_, _| Box::pin(async { Ok(true) }));
    repo.expect_get_meeting_attendance()
        .returning(|_| Box::pin(async { Err(CallError::Internal(anyhow::anyhow!("db down"))) }));
    let mut rtc = MockCallRtcClient::new();
    rtc.expect_start_room_composite_egress()
        .returning(|_, _| Box::pin(async { Ok(EGRESS.to_string()) }));
    rtc.expect_stop_egress()
        .times(1)
        .returning(|_| Box::pin(async { Ok(()) }));
    crate::domain::service::meetings::start_meeting_recording(
        &repo,
        &rtc,
        ARCHIVED_EVENT_CALL_ID,
        "room",
        Some(&egress_config()),
        RecordingRules::default(),
        OneOnOneMeeting,
    )
    .await;
}

fn settings_repo(team: Option<Uuid>, rules: RecordingRules) -> MockCallRepository {
    let mut repo = MockCallRepository::new();
    repo.expect_get_recording_rules()
        .returning(move |_, requested| {
            assert_eq!(requested, team);
            Box::pin(async move { Ok(rules) })
        });
    repo
}

#[tokio::test]
async fn settings_show_team_blocks_and_who_may_edit_them() {
    let host_rules = rules(
        kinds(&[Huddle, OneOnOneMeeting, ExternalMeeting]),
        kinds(&[ExternalMeeting]),
    );
    for (role, can_edit) in [
        (TeamRole::Member, false),
        (TeamRole::Admin, true),
        (TeamRole::Owner, true),
    ] {
        let mut access = teams();
        access.roles.insert(user(HOST).to_string(), role);
        let settings = service(
            settings_repo(Some(HOST_TEAM), host_rules),
            MockCallRtcClient::new(),
            access,
        )
        .get_recording_settings(user(HOST))
        .await
        .unwrap();
        assert_eq!(
            settings,
            CallRecordingSettings {
                record_by_default: host_rules.record_by_default,
                team: Some(TeamRecordingPolicy {
                    blocked: host_rules.blocked,
                    can_edit,
                }),
            }
        );
    }
}

#[tokio::test]
async fn settings_without_a_team_have_no_team_policy() {
    let settings = service(
        settings_repo(None, RecordingRules::default()),
        MockCallRtcClient::new(),
        TeamAccessService::default(),
    )
    .get_recording_settings(user(HOST))
    .await
    .unwrap();
    assert_eq!(settings.record_by_default, CallKinds::ALL);
    assert_eq!(settings.team, None);
}

#[tokio::test]
async fn people_change_only_their_own_defaults() {
    let patch = CallKindsPatch {
        huddles: Some(false),
        ..CallKindsPatch::default()
    };
    let mut repo = settings_repo(Some(HOST_TEAM), RecordingRules::default());
    repo.expect_update_recording_defaults()
        .times(1)
        .returning(move |actor, requested| {
            assert_eq!(actor, user(HOST));
            assert_eq!(requested, patch);
            Box::pin(async { Ok(CallKinds::ALL) })
        });
    service(repo, MockCallRtcClient::new(), teams())
        .update_recording_defaults(
            user(HOST),
            UpdateRecordingDefaultsRequest {
                record_by_default: patch,
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn team_blocks_change_the_receipts_team() {
    let patch = CallKindsPatch {
        external_meetings: Some(true),
        ..CallKindsPatch::default()
    };
    let mut repo = settings_repo(Some(HOST_TEAM), RecordingRules::default());
    repo.expect_update_team_recording_blocks()
        .times(1)
        .returning(move |team, requested| {
            assert_eq!(*team, HOST_TEAM);
            assert_eq!(requested, patch);
            Box::pin(async { Ok(CallKinds::NONE) })
        });
    let receipt = EntityAccessReceipt::<AdminTeamRole>::dangerously_assert_authenticated_user(
        user(HOST),
        &HOST_TEAM.to_string(),
        EntityType::Team,
    );
    service(repo, MockCallRtcClient::new(), teams())
        .update_team_recording_policy(receipt, UpdateTeamRecordingPolicyRequest { blocked: patch })
        .await
        .unwrap();
}

#[tokio::test]
async fn team_blocks_need_a_person_behind_the_receipt() {
    let mut repo = MockCallRepository::new();
    repo.expect_update_team_recording_blocks().never();
    let receipt = EntityAccessReceipt::<AdminTeamRole>::dangerously_assert_internal_user(
        &HOST_TEAM.to_string(),
        EntityType::Team,
    );
    let result = service(repo, MockCallRtcClient::new(), teams())
        .update_team_recording_policy(
            receipt,
            UpdateTeamRecordingPolicyRequest {
                blocked: CallKindsPatch::default(),
            },
        )
        .await;
    assert!(matches!(result, Err(CallError::Forbidden(_))));
}
