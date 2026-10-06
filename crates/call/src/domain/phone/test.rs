use super::*;

fn number(value: &str) -> PhoneNumber {
    PhoneNumber::parse(value).unwrap()
}

fn dialing_config(allowed: &[&str]) -> PhoneDialingConfig {
    PhoneDialingConfig {
        outbound_trunk_id: "ST_outbound".to_string(),
        default_caller_id: None,
        allowed_country_codes: allowed.iter().map(ToString::to_string).collect(),
    }
}

#[test]
fn statuses_round_trip_through_storage_spelling() {
    for status in [
        PhoneCallStatus::Dialing,
        PhoneCallStatus::Ringing,
        PhoneCallStatus::Active,
        PhoneCallStatus::Completed,
        PhoneCallStatus::Missed,
        PhoneCallStatus::NoAnswer,
        PhoneCallStatus::Busy,
        PhoneCallStatus::Declined,
        PhoneCallStatus::Failed,
        PhoneCallStatus::Cancelled,
    ] {
        assert_eq!(status.as_str().parse::<PhoneCallStatus>().unwrap(), status);
    }
    assert!("ringing ".parse::<PhoneCallStatus>().is_err());
    for direction in [PhoneCallDirection::Outbound, PhoneCallDirection::Inbound] {
        assert_eq!(
            direction.as_str().parse::<PhoneCallDirection>().unwrap(),
            direction
        );
    }
}

#[test]
fn only_dialing_ringing_and_active_legs_are_live() {
    assert!(PhoneCallStatus::Dialing.is_live());
    assert!(PhoneCallStatus::Ringing.is_live());
    assert!(PhoneCallStatus::Active.is_live());
    assert!(!PhoneCallStatus::Completed.is_live());
    assert!(!PhoneCallStatus::Missed.is_live());
    assert!(!PhoneCallStatus::Cancelled.is_live());
}

#[test]
fn concluding_a_leg_distinguishes_answered_missed_and_unanswered_calls() {
    use PhoneCallDirection::{Inbound, Outbound};
    use PhoneCallStatus::*;
    assert_eq!(Active.concluded(Inbound), Completed);
    assert_eq!(Active.concluded(Outbound), Completed);
    assert_eq!(Ringing.concluded(Inbound), Missed);
    assert_eq!(Dialing.concluded(Outbound), NoAnswer);
    assert_eq!(Ringing.concluded(Outbound), NoAnswer);
    // Outcomes are final.
    assert_eq!(Busy.concluded(Outbound), Busy);
    assert_eq!(Declined.concluded(Inbound), Declined);
}

#[test]
fn hanging_up_declines_inbound_and_cancels_outbound_calls_before_answer() {
    use PhoneCallDirection::{Inbound, Outbound};
    use PhoneCallStatus::*;
    assert_eq!(Ringing.hung_up(Inbound), Declined);
    assert_eq!(Dialing.hung_up(Outbound), Cancelled);
    assert_eq!(Active.hung_up(Inbound), Completed);
    assert_eq!(Active.hung_up(Outbound), Completed);
    assert_eq!(Missed.hung_up(Inbound), Missed);
}

#[test]
fn sip_failures_map_to_call_outcomes() {
    assert_eq!(DialFailure::from_sip_status(486), DialFailure::Busy);
    assert_eq!(DialFailure::from_sip_status(600), DialFailure::Busy);
    assert_eq!(DialFailure::from_sip_status(480), DialFailure::NoAnswer);
    assert_eq!(DialFailure::from_sip_status(408), DialFailure::NoAnswer);
    assert_eq!(DialFailure::from_sip_status(603), DialFailure::Declined);
    assert_eq!(DialFailure::from_sip_status(404), DialFailure::Unreachable);
    assert_eq!(DialFailure::from_sip_status(503), DialFailure::Failed);
    assert_eq!(DialFailure::Busy.status(), PhoneCallStatus::Busy);
    assert_eq!(DialFailure::Unreachable.status(), PhoneCallStatus::Failed);
}

#[test]
fn dialing_policy_allows_configured_countries_only() {
    let config = dialing_config(&["1", "44"]);
    assert!(config.permits(&number("+1 555 234 5678")).is_ok());
    assert!(config.permits(&number("+44 20 7946 0958")).is_ok());
    assert!(config.permits(&number("+33 1 42 68 53 00")).is_err());
    assert!(dialing_config(&[]).permits(&number("+1 555 234 5678")).is_err());
}

#[test]
fn premium_rate_numbers_are_never_dialed() {
    let config = dialing_config(&["1"]);
    assert!(config.permits(&number("+1 900 234 5678")).is_err());
    assert!(config.permits(&number("+1 976 234 5678")).is_err());
}

#[test]
fn outbound_identities_follow_the_inbound_sip_convention() {
    assert_eq!(
        outbound_participant_identity(&number("+1 555 234 5678")),
        "sip_+15552345678"
    );
}

#[test]
fn extensions_wait_for_the_switchboard_before_dialing() {
    let dialable = DialablePhoneNumber::parse("+1 555 234 5678 x89").unwrap();
    assert_eq!(extension_dtmf(dialable.extension.as_ref().unwrap()), "wwww89");
}

#[test]
fn remote_parties_are_named_by_contact_or_number() {
    let mut leg = PhoneLeg {
        direction: PhoneCallDirection::Inbound,
        remote_number: number("+1 555 234 5678"),
        local_number: None,
        participant_identity: "sip_+15552345678".to_string(),
        status: PhoneCallStatus::Ringing,
        contact: None,
        answered_at: None,
        ended_at: None,
    };
    assert_eq!(leg.remote_party_label(), "+1 (555) 234-5678");
    leg.contact = Some(PhoneContact {
        contact_id: Uuid::now_v7(),
        name: Some("Ada Lovelace".to_string()),
    });
    assert_eq!(leg.remote_party_label(), "Ada Lovelace");
}

#[test]
fn sip_call_statuses_parse_from_livekit_attributes() {
    assert_eq!("ringing".parse(), Ok(SipCallStatus::Ringing));
    assert_eq!("active".parse(), Ok(SipCallStatus::Active));
    assert_eq!("hangup".parse(), Ok(SipCallStatus::Hangup));
    assert!("connected".parse::<SipCallStatus>().is_err());
}
