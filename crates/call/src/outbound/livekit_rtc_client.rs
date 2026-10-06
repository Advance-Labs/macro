//! LiveKit adapter for the [`CallRtcClient`] port.
//!
//! Wraps the `livekit-api` crate to provide room management, token generation,
//! egress recording, and webhook validation.

#[cfg(test)]
mod test;

use futures::future;
use livekit_api::access_token::{AccessToken, TokenVerifier, VideoGrants};
use livekit_api::services::agent_dispatch::AgentDispatchClient;
use livekit_api::services::egress::{EgressClient, EgressOutput, RoomCompositeOptions, encoding};
use livekit_api::services::room::{CreateRoomOptions, RoomClient};
use livekit_api::services::sip::{CreateSIPParticipantOptions, SIPClient};
use livekit_api::services::{ServiceError, TwirpError, TwirpErrorCode};
use livekit_api::webhooks::WebhookReceiver;
use livekit_protocol::{
    AudioCodec, CreateAgentDispatchRequest, EncodedFileOutput, EncodedFileType, ParticipantInfo,
    S3Upload, VideoCodec, encoded_file_output, participant_info,
};
use macro_user_id::cowlike::CowLike;
use macro_user_id::user_id::MacroUserIdStr;
use notification::domain::models::apple::VoipPushPayload;

use crate::domain::meetings::GuestId;
use crate::domain::models::{
    CallError, CallWebhookEvent, EgressS3Config, VerifiedRingToken, VoipPushPayloadRequest,
};
use crate::domain::phone::{
    DialFailure, PhoneNumber, SipDialAnswered, SipDialRequest, SipParticipant,
};
use crate::domain::ports::CallRtcClient;

const VOIP_TOKEN_MINT_CONCURRENCY: usize = 16;
/// Participant attributes LiveKit sets on SIP participants.
const SIP_PHONE_NUMBER_ATTRIBUTE: &str = "sip.phoneNumber";
const SIP_TRUNK_PHONE_NUMBER_ATTRIBUTE: &str = "sip.trunkPhoneNumber";
const SIP_CALL_STATUS_ATTRIBUTE: &str = "sip.callStatus";
const SIP_CALL_ID_ATTRIBUTE: &str = "sip.callID";
const SIP_RULE_ID_ATTRIBUTE: &str = "sip.ruleID";
/// Lowest and highest final SIP response codes that describe a failure.
const SIP_FAILURE_STATUS_RANGE: std::ops::RangeInclusive<u16> = 400..=699;

/// LiveKit implementation of [`CallRtcClient`].
pub struct LivekitRtcClient {
    room_client: RoomClient,
    egress_client: EgressClient,
    agent_dispatch_client: AgentDispatchClient,
    sip_client: SIPClient,
    webhook_receiver: WebhookReceiver,
    token_verifier: TokenVerifier,
    api_key: String,
    api_secret: String,
    /// If set, the named agent is dispatched to each new room for transcription.
    transcription_agent_name: Option<String>,
}

impl LivekitRtcClient {
    /// Create a new LiveKit RTC client.
    ///
    /// # Arguments
    /// * `server_url` - LiveKit server URL (e.g. `https://my-livekit.example.com`)
    /// * `api_key` - LiveKit API key
    /// * `api_secret` - LiveKit API secret
    /// * `transcription_agent_name` - If set, this agent is dispatched to new rooms for STT
    pub fn new(
        server_url: &str,
        api_key: impl Into<String>,
        api_secret: impl Into<String>,
        transcription_agent_name: Option<String>,
    ) -> Self {
        let api_key = api_key.into();
        let api_secret = api_secret.into();
        // Twirp RPC requires HTTP(S), not WebSocket. Convert wss:// → https://
        // and ws:// → http:// so the same env var works for both client SDK and
        // server-side API calls.
        let http_url = server_url
            .replace("wss://", "https://")
            .replace("ws://", "http://");
        let room_client = RoomClient::with_api_key(&http_url, &api_key, &api_secret);
        let egress_client = EgressClient::with_api_key(&http_url, &api_key, &api_secret);
        let agent_dispatch_client =
            AgentDispatchClient::with_api_key(&http_url, &api_key, &api_secret);
        let sip_client = SIPClient::with_api_key(&http_url, &api_key, &api_secret);
        let token_verifier = TokenVerifier::with_api_key(&api_key, &api_secret);
        let webhook_receiver = WebhookReceiver::new(token_verifier.clone());
        Self {
            room_client,
            egress_client,
            agent_dispatch_client,
            sip_client,
            webhook_receiver,
            token_verifier,
            api_key,
            api_secret,
            transcription_agent_name,
        }
    }
}

struct RoomCompositeEgressRequest {
    room_name: String,
    outputs: Vec<EgressOutput>,
    options: RoomCompositeOptions,
}

fn build_room_composite_egress_request(
    room_name: &str,
    s3_config: &EgressS3Config,
) -> RoomCompositeEgressRequest {
    let output = EgressOutput::File(EncodedFileOutput {
        file_type: EncodedFileType::Mp4 as i32,
        filepath: format!("calls/{room_name}/{{time}}"),
        output: Some(encoded_file_output::Output::S3(S3Upload {
            bucket: s3_config.bucket.clone(),
            region: s3_config.region.clone(),
            access_key: s3_config.access_key.clone(),
            secret: s3_config.secret.clone(),
            ..Default::default()
        })),
        ..Default::default()
    });

    let options = RoomCompositeOptions {
        layout: "speaker".to_owned(),
        encoding: encoding::EncodingOptions {
            audio_codec: AudioCodec::Aac,
            video_codec: VideoCodec::H264Main,
            ..Default::default()
        },
        ..Default::default()
    };

    RoomCompositeEgressRequest {
        room_name: room_name.to_owned(),
        outputs: vec![output],
        options,
    }
}

impl CallRtcClient for LivekitRtcClient {
    #[tracing::instrument(err, skip(self))]
    async fn prepare_room(&self, room_name: &str) -> anyhow::Result<()> {
        self.room_client
            .create_room(
                room_name,
                CreateRoomOptions {
                    empty_timeout: 300,
                    ..Default::default()
                },
            )
            .await?;
        Ok(())
    }
    #[tracing::instrument(err, skip(self))]
    async fn create_room(&self, room_name: &str) -> anyhow::Result<()> {
        self.room_client
            .create_room(
                room_name,
                CreateRoomOptions {
                    empty_timeout: 60,
                    ..Default::default()
                },
            )
            .await?;
        Ok(())
    }

    #[tracing::instrument(err, skip(self))]
    async fn delete_room(&self, room_name: &str) -> anyhow::Result<()> {
        self.room_client.delete_room(room_name).await?;
        Ok(())
    }

    #[tracing::instrument(err, skip(self))]
    async fn generate_token(
        &self,
        room_name: &str,
        participant_identity: MacroUserIdStr<'_>,
    ) -> anyhow::Result<String> {
        // Pinned so VoIP-delivered tokens survive push delay and lock-screen ringing.
        // TODO(call-phase-2): add token refresh before supporting calls over 6h.
        let token = AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_identity(participant_identity.as_ref())
            .with_ttl(std::time::Duration::from_secs(6 * 3600))
            .with_grants(VideoGrants {
                room_join: true,
                room: room_name.to_string(),
                can_publish: true,
                can_subscribe: true,
                can_publish_data: true,
                ..Default::default()
            })
            .to_jwt()?;
        Ok(token)
    }

    #[tracing::instrument(err, skip(self))]
    async fn generate_guest_token(
        &self,
        room_name: &str,
        guest_id: GuestId,
        display_name: &str,
    ) -> anyhow::Result<String> {
        Ok(AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_identity(&guest_id.to_string())
            .with_name(display_name)
            .with_ttl(std::time::Duration::from_secs(6 * 3600))
            .with_grants(VideoGrants {
                room_join: true,
                room: room_name.to_string(),
                can_publish: true,
                can_subscribe: true,
                can_publish_data: true,
                ..Default::default()
            })
            .to_jwt()?)
    }

    #[tracing::instrument(err, skip(self))]
    async fn remove_guest(&self, room_name: &str, guest_id: GuestId) -> anyhow::Result<()> {
        interpret_remove_participant_result(
            self.room_client
                .remove_participant(room_name, &guest_id.to_string())
                .await,
        )
    }

    #[tracing::instrument(
        skip(self, request),
        fields(
            recipient_count = request.recipients.len(),
            room_name = request.room_name,
            call_id = %request.call_id,
            channel_id = request.channel_id,
        )
    )]
    async fn build_voip_push_payloads<'a>(
        &self,
        request: VoipPushPayloadRequest<'a>,
    ) -> Vec<(MacroUserIdStr<'static>, VoipPushPayload)> {
        let room_name = request.room_name;
        let call_id = request.call_id;
        let channel_id = request.channel_id;
        let channel_name = request.channel_name;
        let caller_name = request.caller_name;
        let livekit_server_url = request.livekit_server_url;
        let ring_status_url = request.ring_status_url;

        let mut payloads = Vec::new();
        for batch in request.recipients.chunks(VOIP_TOKEN_MINT_CONCURRENCY) {
            let results = future::join_all(batch.iter().map(|recipient_id| {
                let recipient_id = recipient_id.clone();
                async move {
                    match self.generate_token(room_name, recipient_id.clone()).await {
                        Ok(livekit_token) => Some((
                            recipient_id,
                            VoipPushPayload {
                                aps: Default::default(),
                                call_id: call_id.to_string(),
                                channel_id: channel_id.to_string(),
                                channel_name: channel_name.to_string(),
                                caller_name: caller_name.to_string(),
                                livekit_server_url: Some(livekit_server_url.to_string()),
                                livekit_token: Some(livekit_token),
                                ring_status_url: ring_status_url.map(str::to_string),
                            },
                        )),
                        Err(e) => {
                            tracing::error!(
                                error=?e,
                                "failed to mint LiveKit token for VoIP push"
                            );
                            None
                        }
                    }
                }
            }))
            .await;
            payloads.extend(results.into_iter().flatten());
        }

        payloads
    }

    #[tracing::instrument(err, skip(self))]
    async fn remove_participant(
        &self,
        room_name: &str,
        participant_identity: MacroUserIdStr<'_>,
    ) -> anyhow::Result<()> {
        interpret_remove_participant_result(
            self.room_client
                .remove_participant(room_name, participant_identity.as_ref())
                .await,
        )
    }

    #[tracing::instrument(err, skip(self))]
    async fn list_meeting_participants(
        &self,
        room_name: &str,
    ) -> anyhow::Result<Option<Vec<crate::domain::meetings::MeetingRtcParticipant>>> {
        match self.room_client.list_participants(room_name).await {
            Ok(participants) => Ok(Some(
                participants
                    .into_iter()
                    .map(
                        |participant| crate::domain::meetings::MeetingRtcParticipant {
                            identity: participant.identity,
                            name: participant.name,
                        },
                    )
                    .collect(),
            )),
            Err(error) if is_not_found(&error) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    #[tracing::instrument(err, skip(self))]
    async fn list_participant_identities(
        &self,
        room_name: &str,
    ) -> anyhow::Result<Option<Vec<String>>> {
        Ok(self
            .list_meeting_participants(room_name)
            .await?
            .map(|participants| {
                participants
                    .into_iter()
                    .map(|participant| participant.identity)
                    .collect()
            }))
    }

    #[tracing::instrument(err, skip(self, s3_config))]
    async fn start_room_composite_egress(
        &self,
        room_name: &str,
        s3_config: &EgressS3Config,
    ) -> anyhow::Result<String> {
        let request = build_room_composite_egress_request(room_name, s3_config);

        let info = self
            .egress_client
            .start_room_composite_egress(&request.room_name, request.outputs, request.options)
            .await?;

        Ok(info.egress_id)
    }

    #[tracing::instrument(err, skip(self))]
    async fn stop_egress(&self, egress_id: &str) -> anyhow::Result<()> {
        self.egress_client.stop_egress(egress_id).await?;
        Ok(())
    }

    #[tracing::instrument(err, skip(self))]
    async fn dispatch_transcription_agent(&self, room_name: &str) -> anyhow::Result<()> {
        let Some(agent_name) = &self.transcription_agent_name else {
            return Ok(());
        };

        self.agent_dispatch_client
            .create_dispatch(CreateAgentDispatchRequest {
                agent_name: agent_name.clone(),
                room: room_name.to_owned(),
                ..Default::default()
            })
            .await?;

        tracing::info!(room_name, agent_name, "dispatched transcription agent");
        Ok(())
    }

    #[tracing::instrument(
        skip(self, request),
        fields(room_name = %request.room_name, trunk_id = %request.trunk_id)
    )]
    async fn dial_sip_participant(
        &self,
        request: SipDialRequest,
    ) -> Result<SipDialAnswered, DialFailure> {
        let options = CreateSIPParticipantOptions {
            participant_identity: request.participant_identity,
            participant_name: Some(request.participant_name),
            sip_number: request.caller_id.map(String::from),
            dtmf: request.dtmf,
            // Block until the call connects or fails, so failures carry the
            // SIP status that explains them.
            wait_until_answered: Some(true),
            // Let the people already in the room hear the call ring.
            play_dialtone: Some(true),
            ringing_timeout: Some(request.ringing_timeout),
            max_call_duration: Some(request.max_call_duration),
            ..Default::default()
        };
        match self
            .sip_client
            .create_sip_participant(
                request.trunk_id,
                request.to.to_string(),
                request.room_name,
                options,
                None,
            )
            .await
        {
            Ok(info) => Ok(SipDialAnswered {
                sip_call_id: Some(info.sip_call_id).filter(|id| !id.is_empty()),
            }),
            Err(error) => {
                let failure = classify_dial_error(&error);
                tracing::info!(error=?error, ?failure, "SIP call did not connect");
                Err(failure)
            }
        }
    }

    fn verify_access_token(&self, token: &str) -> anyhow::Result<VerifiedRingToken> {
        let claims = self
            .token_verifier
            .verify(token)
            .map_err(|e| anyhow::anyhow!("access token verification failed: {e}"))?;

        Ok(VerifiedRingToken {
            identity: claims.sub,
            room: Some(claims.video.room).filter(|r| !r.is_empty()),
        })
    }

    fn receive_webhook(&self, body: &str, auth_token: &str) -> Result<CallWebhookEvent, CallError> {
        let event = self
            .webhook_receiver
            .receive(body, auth_token)
            .map_err(|e| {
                tracing::warn!(error=?e, "webhook signature validation failed");
                CallError::Auth
            })?;

        // Extract file URL from egress info if available.
        let (egress_id, file_url) = match &event.egress_info {
            Some(info) => {
                let url = info.file_results.first().map(|f| f.location.clone());
                let id = if info.egress_id.is_empty() {
                    None
                } else {
                    Some(info.egress_id.clone())
                };
                (id, url)
            }
            None => (None, None),
        };

        // Phone participants are recognized by their kind, whatever identity
        // the SIP stack gave them.
        let sip_participant = event
            .participant
            .as_ref()
            .filter(|p| p.kind == participant_info::Kind::Sip as i32)
            .map(sip_participant);

        // Keep UUID guests separate from Macro users and agent identities.
        let guest_identity = event
            .participant
            .as_ref()
            .filter(|p| p.kind != participant_info::Kind::Sip as i32)
            .filter(|p| MacroUserIdStr::parse_from_str(&p.identity).is_err())
            .and_then(|p| GuestId::parse_rtc_identity(&p.identity));

        Ok(CallWebhookEvent {
            guest_identity,
            sip_participant,
            event: event.event,
            id: event.id,
            room_name: event.room.map(|r| r.name),
            participant_identity: event.participant.and_then(|p| {
                match MacroUserIdStr::parse_from_str(&p.identity) {
                    Ok(id) => Some(id.into_owned()),
                    Err(_) => {
                        tracing::debug!(
                            identity = %p.identity,
                            "skipping non-user LiveKit participant identity"
                        );
                        None
                    }
                }
            }),
            egress_id,
            file_url,
            created_at: event.created_at,
        })
    }
}

fn interpret_remove_participant_result(result: Result<(), ServiceError>) -> anyhow::Result<()> {
    match result {
        Ok(()) => Ok(()),
        Err(error) if is_not_found(&error) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn is_not_found(error: &ServiceError) -> bool {
    matches!(
        error,
        ServiceError::Twirp(TwirpError::Twirp(code)) if code.code == TwirpErrorCode::NOT_FOUND
    )
}

/// Read the SIP facts LiveKit records as participant attributes. Numbers that
/// are absent or not E.164 (a withheld caller id) come back as `None`.
fn sip_participant(participant: &ParticipantInfo) -> SipParticipant {
    let attribute = |key: &str| {
        participant
            .attributes
            .get(key)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
    };
    let phone_number =
        |key: &str| attribute(key).and_then(|value| PhoneNumber::from_e164(value).ok());
    SipParticipant {
        identity: participant.identity.clone(),
        phone_number: phone_number(SIP_PHONE_NUMBER_ATTRIBUTE),
        trunk_phone_number: phone_number(SIP_TRUNK_PHONE_NUMBER_ATTRIBUTE),
        call_status: attribute(SIP_CALL_STATUS_ATTRIBUTE).and_then(|value| value.parse().ok()),
        sip_call_id: attribute(SIP_CALL_ID_ATTRIBUTE).map(str::to_string),
        is_inbound: attribute(SIP_RULE_ID_ATTRIBUTE).is_some(),
    }
}

/// Explain why `CreateSIPParticipant` did not connect. LiveKit reports the
/// final SIP response in the error message (e.g. `486 Busy Here`); the Twirp
/// code is the fallback when no status is present.
fn classify_dial_error(error: &ServiceError) -> DialFailure {
    let ServiceError::Twirp(TwirpError::Twirp(code)) = error else {
        return DialFailure::Failed;
    };
    if let Some(status) = sip_status_in(&code.msg) {
        return DialFailure::from_sip_status(status);
    }
    match code.code.as_str() {
        TwirpErrorCode::DEADLINE_EXCEEDED => DialFailure::NoAnswer,
        TwirpErrorCode::RESOURCE_EXHAUSTED => DialFailure::Busy,
        TwirpErrorCode::PERMISSION_DENIED => DialFailure::Declined,
        TwirpErrorCode::NOT_FOUND | TwirpErrorCode::INVALID_ARGUMENT => DialFailure::Unreachable,
        _ => DialFailure::Failed,
    }
}

/// The first standalone three-digit SIP failure status in `message`.
fn sip_status_in(message: &str) -> Option<u16> {
    message
        .split(|c: char| !c.is_ascii_digit())
        .filter(|digits| digits.len() == 3)
        .filter_map(|digits| digits.parse().ok())
        .find(|status| SIP_FAILURE_STATUS_RANGE.contains(status))
}
