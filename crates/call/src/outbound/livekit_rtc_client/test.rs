use base64::Engine;
use livekit_api::access_token::AccessToken;
use livekit_api::services::{ServiceError, TwirpError, TwirpErrorCode};
use macro_user_id::user_id::MacroUserIdStr;
use sha2::{Digest, Sha256};

use super::*;
use crate::domain::models::CallError;
use crate::domain::ports::CallRtcClient as _;

const API_KEY: &str = "test-api-key";
const API_SECRET: &str = "test-api-secret-test-api-secret-test";

fn twirp(code: &str, msg: &str) -> ServiceError {
    ServiceError::Twirp(TwirpError::Twirp(TwirpErrorCode {
        code: code.to_string(),
        msg: msg.to_string(),
    }))
}

fn client() -> LivekitRtcClient {
    LivekitRtcClient::new("wss://lk.example", API_KEY, API_SECRET, None)
}

fn client_with_transcription_agent(agent_name: &str) -> LivekitRtcClient {
    LivekitRtcClient::new(
        "wss://lk.example",
        API_KEY,
        API_SECRET,
        Some(agent_name.to_owned()),
    )
}

fn sign_webhook(body: &str) -> String {
    let digest = Sha256::digest(body.as_bytes());
    let sha256 = base64::engine::general_purpose::STANDARD.encode(digest);
    AccessToken::with_api_key(API_KEY, API_SECRET)
        .with_sha256(&sha256)
        .to_jwt()
        .expect("webhook jwt")
}

fn participant_joined_body(identity: &str) -> String {
    serde_json::json!({
        "event": "participant_joined",
        "id": "EV_test",
        "createdAt": 1,
        "room": { "name": "room-1" },
        "participant": { "identity": identity }
    })
    .to_string()
}

fn receive_participant_joined(
    client: &LivekitRtcClient,
    identity: &str,
) -> Result<crate::domain::models::CallWebhookEvent, CallError> {
    let body = participant_joined_body(identity);
    let token = sign_webhook(&body);
    client.receive_webhook(&body, &token)
}

#[test]
fn room_composite_egress_uses_speaker_layout() {
    let request = build_room_composite_egress_request(
        "room-1",
        &EgressS3Config {
            bucket: "test-bucket".to_owned(),
            region: "us-east-1".to_owned(),
            access_key: "test-access-key".to_owned(),
            secret: "test-secret".to_owned(),
        },
    );

    assert_eq!(request.options.layout, "speaker");
}

#[tokio::test]
async fn verify_access_token_round_trips_identity_and_room() {
    let client = client();
    let identity = MacroUserIdStr::try_from_email("alice@example.com").unwrap();

    let token = client
        .generate_token("room-1", identity.clone())
        .await
        .expect("token mint is pure JWT crypto, no network");

    let verified = client.verify_access_token(&token).expect("token verifies");
    assert_eq!(verified.identity, identity.as_ref());
    assert_eq!(verified.room.as_deref(), Some("room-1"));
}

#[tokio::test]
async fn verify_access_token_rejects_token_signed_with_a_different_secret() {
    let identity = MacroUserIdStr::try_from_email("alice@example.com").unwrap();
    let token = client()
        .generate_token("room-1", identity)
        .await
        .expect("token mint is pure JWT crypto, no network");

    let other = LivekitRtcClient::new(
        "wss://lk.example",
        "test-api-key",
        "a-completely-different-secret-value!",
        None,
    );
    assert!(other.verify_access_token(&token).is_err());
}

#[test]
fn verify_access_token_rejects_garbage() {
    assert!(client().verify_access_token("not-a-jwt").is_err());
}

#[test]
fn remove_participant_not_found_is_already_gone() {
    let error = twirp(TwirpErrorCode::NOT_FOUND, "participant does not exist");
    interpret_remove_participant_result(Err(error))
        .expect("leave must succeed when LiveKit already dropped the participant");
}

#[test]
fn remove_participant_room_not_found_is_already_gone() {
    let error = twirp(TwirpErrorCode::NOT_FOUND, "requested room does not exist");
    interpret_remove_participant_result(Err(error))
        .expect("leave must succeed when the LiveKit room is already gone");
}

#[test]
fn remove_participant_unavailable_still_fails() {
    let error = twirp(TwirpErrorCode::UNAVAILABLE, "overloaded");
    let message = interpret_remove_participant_result(Err(error))
        .expect_err("transient LiveKit failures must still surface")
        .to_string();
    assert_eq!(message, "twirp error: twirp error: unavailable: overloaded");
}

#[test]
fn receive_webhook_accepts_livekit_agent_participant_identity() {
    let event = receive_participant_joined(
        &client_with_transcription_agent("macro-transcriber"),
        "agent-AJ_CNFob6rwkHxK",
    )
    .expect("agent join must not fail webhook ingest");

    assert_eq!(event.event, "participant_joined");
    assert_eq!(event.room_name.as_deref(), Some("room-1"));
    assert_eq!(event.participant_identity, None);
}

#[test]
fn receive_webhook_parses_macro_user_participant_identity() {
    let event = receive_participant_joined(&client(), "macro|alice@example.com")
        .expect("user join must parse identity");

    assert_eq!(
        event
            .participant_identity
            .as_ref()
            .map(|identity| identity.as_ref()),
        Some("macro|alice@example.com")
    );
}

#[test]
fn receive_webhook_skips_configured_transcription_agent_name() {
    let event = receive_participant_joined(
        &client_with_transcription_agent("macro-transcriber"),
        "macro-transcriber",
    )
    .expect("configured agent name must not fail webhook ingest");

    assert_eq!(event.participant_identity, None);
}

#[tokio::test]
async fn guest_tokens_preserve_names_and_only_grant_the_room() {
    let client = client();
    let guest_id = crate::domain::meetings::GuestId::generate();
    let token = client
        .generate_guest_token("meeting-room", guest_id, "Ada")
        .await
        .unwrap();
    let verified = client.verify_access_token(&token).unwrap();
    assert_eq!(verified.identity, guest_id.to_string());
    assert_eq!(verified.room.as_deref(), Some("meeting-room"));
    let payload = token.split('.').nth(1).unwrap();
    let claims: serde_json::Value = serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(claims["name"], "Ada");
    assert_eq!(claims["video"]["roomJoin"], true);
    assert_ne!(claims["video"]["roomAdmin"], true);
    let event = receive_participant_joined(&client, &guest_id.to_string()).unwrap();
    assert_eq!(event.guest_identity, Some(guest_id));
    assert_eq!(event.participant_identity, None);
}

fn receive_sip_participant(
    client: &LivekitRtcClient,
    event: &str,
    identity: &str,
    attributes: serde_json::Value,
) -> crate::domain::models::CallWebhookEvent {
    let body = serde_json::json!({
        "event": event,
        "id": "EV_sip",
        "createdAt": 1,
        "room": { "name": "phone_+15552345678_abcd" },
        "participant": { "identity": identity, "kind": "SIP", "attributes": attributes }
    })
    .to_string();
    let token = sign_webhook(&body);
    client
        .receive_webhook(&body, &token)
        .expect("signed webhook")
}

#[test]
fn receive_webhook_reads_inbound_sip_participants_from_their_attributes() {
    let event = receive_sip_participant(
        &client(),
        "participant_joined",
        "sip_+15552345678",
        serde_json::json!({
            "sip.phoneNumber": "+15552345678",
            "sip.trunkPhoneNumber": "+15559876543",
            "sip.callStatus": "ringing",
            "sip.callID": "SCL_abc",
            "sip.ruleID": "SDR_rule",
            "sip.trunkID": "ST_inbound",
        }),
    );

    let sip = event.sip_participant.expect("SIP participant");
    assert_eq!(sip.identity, "sip_+15552345678");
    assert_eq!(sip.phone_number.unwrap().as_str(), "+15552345678");
    assert_eq!(sip.trunk_phone_number.unwrap().as_str(), "+15559876543");
    assert_eq!(
        sip.call_status,
        Some(crate::domain::phone::SipCallStatus::Ringing)
    );
    assert_eq!(sip.sip_call_id.as_deref(), Some("SCL_abc"));
    assert!(sip.is_inbound);
    assert_eq!(event.participant_identity, None);
    assert_eq!(event.guest_identity, None);
}

#[test]
fn receive_webhook_tolerates_withheld_caller_ids_and_outbound_legs() {
    let event = receive_sip_participant(
        &client(),
        "participant_left",
        "sip_+15552345678",
        serde_json::json!({
            "sip.phoneNumber": "anonymous",
            "sip.callStatus": "hangup",
            "sip.ruleID": "",
        }),
    );

    let sip = event.sip_participant.expect("SIP participant");
    assert_eq!(sip.phone_number, None);
    assert_eq!(sip.trunk_phone_number, None);
    assert!(!sip.is_inbound);
}

#[test]
fn receive_webhook_never_mistakes_a_sip_participant_for_a_guest() {
    let guest_shaped = crate::domain::meetings::GuestId::generate().to_string();
    let event = receive_sip_participant(
        &client(),
        "participant_joined",
        &guest_shaped,
        serde_json::json!({}),
    );
    assert_eq!(event.guest_identity, None);
    assert_eq!(event.sip_participant.unwrap().identity, guest_shaped);
}

#[test]
fn dial_failures_are_classified_by_sip_status_then_twirp_code() {
    use crate::domain::phone::DialFailure;

    for (code, message, expected) in [
        (
            TwirpErrorCode::UNAVAILABLE,
            "INVITE failed: sip status: 486: Busy Here",
            DialFailure::Busy,
        ),
        (
            TwirpErrorCode::UNKNOWN,
            "sip status 603 (Decline)",
            DialFailure::Declined,
        ),
        (
            TwirpErrorCode::UNKNOWN,
            "call failed with 480 Temporarily Unavailable",
            DialFailure::NoAnswer,
        ),
        (
            TwirpErrorCode::NOT_FOUND,
            "sip status: 404 Not Found",
            DialFailure::Unreachable,
        ),
        (
            TwirpErrorCode::DEADLINE_EXCEEDED,
            "ringing timeout",
            DialFailure::NoAnswer,
        ),
        (
            TwirpErrorCode::RESOURCE_EXHAUSTED,
            "busy",
            DialFailure::Busy,
        ),
        (
            TwirpErrorCode::INTERNAL,
            "trunk 200 misconfigured",
            DialFailure::Failed,
        ),
    ] {
        assert_eq!(
            classify_dial_error(&twirp(code, message)),
            expected,
            "{message}"
        );
    }
}

#[test]
fn sip_statuses_are_whole_three_digit_failure_codes() {
    assert_eq!(sip_status_in("sip status: 486: Busy Here"), Some(486));
    assert_eq!(
        sip_status_in("200 OK then 487 Request Terminated"),
        Some(487)
    );
    assert_eq!(sip_status_in("room 4860 not found"), None);
    assert_eq!(sip_status_in("no status"), None);
}
