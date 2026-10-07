use super::*;

const KINDS: [CallKind; 4] = [
    CallKind::Huddle,
    CallKind::OneOnOneMeeting,
    CallKind::InternalMeeting,
    CallKind::ExternalMeeting,
];

fn only(kind: CallKind) -> CallKinds {
    CallKinds {
        huddles: kind == CallKind::Huddle,
        one_on_one_meetings: kind == CallKind::OneOnOneMeeting,
        internal_meetings: kind == CallKind::InternalMeeting,
        external_meetings: kind == CallKind::ExternalMeeting,
    }
}

#[test]
fn untouched_settings_record_every_call() {
    let rules = RecordingRules::default();
    for kind in KINDS {
        assert!(rules.records(kind), "{kind:?} should record");
    }
}

#[test]
fn a_call_records_only_when_chosen_by_default_and_not_blocked() {
    for kind in KINDS {
        for other in KINDS {
            let chosen = RecordingRules {
                record_by_default: only(kind),
                blocked: CallKinds::NONE,
            };
            assert_eq!(chosen.records(other), kind == other);

            let blocked = RecordingRules {
                record_by_default: CallKinds::ALL,
                blocked: only(kind),
            };
            assert_eq!(blocked.records(other), kind != other);
        }
    }
}

#[test]
fn clearing_every_default_turns_recording_off() {
    let rules = RecordingRules {
        record_by_default: CallKinds::NONE,
        blocked: CallKinds::NONE,
    };
    for kind in KINDS {
        assert!(!rules.records(kind));
    }
}

#[test]
fn a_team_block_overrides_a_personal_default() {
    let rules = RecordingRules {
        record_by_default: CallKinds::ALL,
        blocked: CallKinds::ALL,
    };
    for kind in KINDS {
        assert!(!rules.records(kind));
    }
}

#[test]
fn a_patch_changes_only_the_kinds_it_names() {
    let patched = CallKinds::ALL.patched(CallKindsPatch {
        external_meetings: Some(false),
        ..CallKindsPatch::default()
    });
    assert_eq!(
        patched,
        CallKinds {
            huddles: true,
            one_on_one_meetings: true,
            internal_meetings: true,
            external_meetings: false,
        }
    );
    assert_eq!(patched.patched(CallKindsPatch::default()), patched);
}

#[test]
fn patches_read_camel_case_and_allow_omitted_fields() {
    let request: UpdateTeamCallPolicyRequest =
        serde_json::from_str(r#"{"recordingBlocked":{"oneOnOneMeetings":true}}"#).unwrap();
    assert_eq!(
        request,
        UpdateTeamCallPolicyRequest {
            recording_blocked: CallKindsPatch {
                one_on_one_meetings: Some(true),
                ..CallKindsPatch::default()
            },
            huddle_sharing_blocked: None,
        }
    );
    let request: UpdateCallSettingsRequest =
        serde_json::from_str(r#"{"refuseOneOnOneRecording":true}"#).unwrap();
    assert_eq!(
        request,
        UpdateCallSettingsRequest {
            refuse_one_on_one_recording: Some(true),
            ..UpdateCallSettingsRequest::default()
        }
    );
}

#[test]
fn huddles_start_shared_only_when_chosen_and_not_blocked() {
    for (share_by_default, blocked, shares) in [
        (true, false, true),
        (false, false, false),
        (true, true, false),
        (false, true, false),
    ] {
        let sharing = HuddleSharing {
            share_by_default,
            blocked,
        };
        assert_eq!(sharing.shares_by_default(), shares);
    }
    assert!(HuddleSharing::default().shares_by_default());
}

#[test]
fn untouched_preferences_record_share_and_refuse_nothing() {
    let preferences = CallPreferences::default();
    assert_eq!(preferences.recording, RecordingRules::default());
    assert_eq!(preferences.huddle_sharing, HuddleSharing::default());
    assert!(!preferences.refuses_one_on_one_recording);
}

#[test]
fn settings_serialize_camel_case() {
    let settings = CallSettings {
        record_by_default: only(CallKind::Huddle),
        share_huddles_by_default: false,
        refuse_one_on_one_recording: true,
        team: Some(TeamCallPolicy {
            recording_blocked: only(CallKind::ExternalMeeting),
            huddle_sharing_blocked: true,
            can_edit: false,
        }),
    };
    assert_eq!(
        serde_json::to_value(settings).unwrap(),
        serde_json::json!({
            "recordByDefault": {
                "huddles": true,
                "oneOnOneMeetings": false,
                "internalMeetings": false,
                "externalMeetings": false,
            },
            "shareHuddlesByDefault": false,
            "refuseOneOnOneRecording": true,
            "team": {
                "recordingBlocked": {
                    "huddles": false,
                    "oneOnOneMeetings": false,
                    "internalMeetings": false,
                    "externalMeetings": true,
                },
                "huddleSharingBlocked": true,
                "canEdit": false,
            },
        })
    );
}

#[test]
fn a_recorder_answers_for_every_kind_since_it_started() {
    use CallKind::{ExternalMeeting, InternalMeeting, OneOnOneMeeting};
    let attendance = |more_than_two, external| MeetingAttendance {
        more_than_two,
        external,
    };
    for (since, seen, kinds) in [
        (
            OneOnOneMeeting,
            attendance(false, false),
            vec![OneOnOneMeeting],
        ),
        (
            OneOnOneMeeting,
            attendance(true, false),
            vec![OneOnOneMeeting, InternalMeeting],
        ),
        (
            OneOnOneMeeting,
            attendance(false, true),
            vec![OneOnOneMeeting, ExternalMeeting],
        ),
        (
            OneOnOneMeeting,
            attendance(true, true),
            vec![OneOnOneMeeting, InternalMeeting, ExternalMeeting],
        ),
        (
            InternalMeeting,
            attendance(true, true),
            vec![InternalMeeting, ExternalMeeting],
        ),
        (
            ExternalMeeting,
            attendance(false, true),
            vec![ExternalMeeting],
        ),
        (
            ExternalMeeting,
            attendance(true, true),
            vec![ExternalMeeting],
        ),
    ] {
        assert_eq!(seen.kinds_since(since), kinds, "{since:?} with {seen:?}");
    }
}
