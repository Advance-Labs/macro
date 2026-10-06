//! Phone call use cases: dialing out, ringing a number's owner for an
//! inbound call, answering, hanging up, and following the phone leg through
//! RTC webhooks.
//!
//! A phone call lives and dies with its phone party. Hanging up ends the call
//! for everyone, and so does the person on the phone hanging up; the archive
//! that follows records the call like any other.

use chrono::Utc;
use tracing::Instrument;

use super::meetings::start_meeting_recording;
use super::*;
use crate::domain::phone::{
    AssignPhoneNumberRequest, DialFailure, DialPhoneRequest, DialablePhoneNumber,
    IncomingPhoneCall, IncomingPhoneCallsResponse, MAX_PHONE_CALL_DURATION, NewPhoneCall,
    NewPhoneLeg, OUTBOUND_RINGING_TIMEOUT, PhoneCallDirection, PhoneCallJoinResponse,
    PhoneCallStatus, PhoneCallUpdated, PhoneContact, PhoneLeg, PhoneLegUpdate, PhoneNumber,
    PhoneSettingsResponse, SipDialAnswered, SipDialRequest, SipParticipant, extension_dtmf,
    outbound_participant_identity,
};

/// Websocket event telling a number's owner that a phone call is ringing.
const PHONE_CALL_INCOMING_EVENT: &str = "phone_call_incoming";
/// Websocket event telling a phone call's owner its leg changed state.
const PHONE_CALL_UPDATED_EVENT: &str = "phone_call_updated";

impl<R, C, Cn, E, N, S, Sm, V, Vr, B, Ph, Pd>
    CallServiceImpl<R, C, Cn, E, N, S, Sm, V, Vr, B, Ph, Pd>
where
    R: CallRepository + Clone,
    C: CallRtcClient,
    Cn: ConnectionService,
    E: EntityAccessService,
    N: NotificationIngress,
    S: RecordingStorage,
    Sm: CallSummarizer + Clone,
    V: VoipPushSender,
    Vr: VoiceRepository + Clone,
    B: MacroEventBroker + Clone,
    Ph: PhoneCallRepository + Clone,
    Pd: PhoneContactDirectory + Clone,
{
    #[tracing::instrument(err, skip(self))]
    pub(super) async fn phone_settings(
        &self,
        actor: MacroUserIdStr<'_>,
    ) -> Result<PhoneSettingsResponse, CallError> {
        let phone_numbers = self.phone_repo.phone_numbers_for_user(actor).await?;
        let dialing = self.phone_dialing.as_ref();
        let caller_id = phone_numbers
            .first()
            .cloned()
            .or_else(|| dialing.and_then(|config| config.default_caller_id.clone()));
        Ok(PhoneSettingsResponse {
            dialing_enabled: dialing.is_some(),
            caller_id,
            phone_numbers,
        })
    }

    #[tracing::instrument(err, skip(self, request))]
    pub(super) async fn dial(
        &self,
        actor: MacroUserIdStr<'_>,
        request: DialPhoneRequest,
    ) -> Result<PhoneCallJoinResponse, CallError> {
        let dialing = self.phone_dialing.as_ref().ok_or_else(|| {
            CallError::Unavailable("Phone calling isn't set up for your workspace".to_string())
        })?;
        let dialable = DialablePhoneNumber::parse(&request.to)
            .map_err(|error| CallError::InvalidRequest(error.to_string()))?;
        dialing.permits(&dialable.number)?;

        let caller_id = self
            .phone_repo
            .phone_numbers_for_user(actor.copied())
            .await?
            .into_iter()
            .next()
            .or_else(|| dialing.default_caller_id.clone());
        let contact = self
            .find_phone_contact(actor.copied(), &dialable.number)
            .await;

        let call_id = Uuid::now_v7();
        let room_name = call_id.to_string();
        // Signing is local; mint before creating anything that would need
        // cleaning up if it failed.
        let token = self
            .rtc_client
            .generate_token(&room_name, actor.copied())
            .await
            .map_err(CallError::Internal)?;
        self.leave_other_active_call(actor.copied(), call_id)
            .await?;
        self.rtc_client
            .create_room(&room_name)
            .await
            .map_err(CallError::Internal)?;

        let participant_identity = outbound_participant_identity(&dialable.number);
        let leg = PhoneLeg {
            direction: PhoneCallDirection::Outbound,
            remote_number: dialable.number.clone(),
            local_number: caller_id.clone(),
            participant_identity: participant_identity.clone(),
            status: PhoneCallStatus::Dialing,
            contact: contact.clone(),
            answered_at: None,
            ended_at: None,
        };
        let created = self
            .phone_repo
            .create_outbound_phone_call(NewPhoneCall {
                call_id,
                room_name: room_name.clone(),
                owner: actor.copied().into_owned(),
                leg: NewPhoneLeg {
                    direction: leg.direction,
                    remote_number: leg.remote_number.clone(),
                    local_number: leg.local_number.clone(),
                    participant_identity: participant_identity.clone(),
                    status: leg.status,
                    contact,
                    sip_call_id: None,
                },
            })
            .await;
        let call = match created {
            Ok(call) => call,
            Err(error) => {
                self.rtc_client
                    .delete_room(&room_name)
                    .await
                    .inspect_err(
                        |e| tracing::warn!(error=?e, "failed to delete unused phone call room"),
                    )
                    .ok();
                return Err(error);
            }
        };
        self.publish_call_event(&CallMacroEvent::started(CallStartedMetadata {
            call_id,
            channel_id: None,
            created_by: actor.copied().into_owned(),
            created_at: call.created_at,
            recording_enabled: self.egress_s3_config.is_some(),
        }));

        self.spawn_outbound_phone_leg(
            call_id,
            actor.copied().into_owned(),
            SipDialRequest {
                room_name: room_name.clone(),
                trunk_id: dialing.outbound_trunk_id.clone(),
                to: dialable.number,
                caller_id,
                participant_identity,
                participant_name: leg.remote_party_label(),
                dtmf: dialable.extension.as_ref().map(extension_dtmf),
                ringing_timeout: OUTBOUND_RINGING_TIMEOUT,
                max_call_duration: MAX_PHONE_CALL_DURATION,
            },
        );

        Ok(PhoneCallJoinResponse {
            call: CallTokenResponse {
                participant_id: actor.to_string(),
                share_token: None,
                call_id,
                channel_id: None,
                token,
                room_name,
                server_url: self.server_url.clone(),
            },
            phone: leg,
        })
    }

    /// Start recording and transcription, then place the SIP leg and record
    /// how it went. The caller is already in the room, so they hear it ring.
    fn spawn_outbound_phone_leg(
        &self,
        call_id: Uuid,
        owner: MacroUserIdStr<'static>,
        dial: SipDialRequest,
    ) {
        let rtc = self.rtc_client.clone();
        let repo = self.repo.clone();
        let phone_repo = self.phone_repo.clone();
        let connection_service = self.connection_service.clone();
        let egress = self.egress_s3_config.clone();
        tokio::spawn(
            async move {
                let room_name = dial.room_name.clone();
                // Start capture before dialing so the callee's first words
                // are recorded and transcribed.
                let transcription = async {
                    rtc.dispatch_transcription_agent(&room_name)
                        .await
                        .inspect_err(|e| tracing::error!(error=?e, "failed to dispatch phone call transcription agent"))
                        .ok();
                };
                let recording =
                    start_meeting_recording(&repo, rtc.as_ref(), call_id, &room_name, egress.as_ref());
                tokio::join!(transcription, recording);
                let outcome = rtc.dial_sip_participant(dial).await;
                let leg = record_dial_outcome(&phone_repo, call_id, outcome).await;
                if let Some(leg) = &leg {
                    send_phone_call_updated(connection_service.as_ref(), owner, call_id, leg).await;
                    // A call that never connected is ended so the caller is
                    // not left alone in the room; the archive then keeps the
                    // outcome (busy, declined, …) on the call record.
                    if !leg.status.is_live() {
                        rtc.delete_room(&room_name)
                            .await
                            .inspect_err(|e| tracing::error!(error=?e, "failed to end unconnected phone call"))
                            .ok();
                    }
                }
            }
            .instrument(tracing::info_span!("dial_phone_leg", %call_id)),
        );
    }

    #[tracing::instrument(err, skip(self, receipt))]
    pub(super) async fn answer_inbound(
        &self,
        receipt: EntityAccessReceipt<ViewAccessLevel>,
    ) -> Result<PhoneCallJoinResponse, CallError> {
        let (call_id, actor) = phone_call_receipt(&receipt)?;
        let call = self.live_call(&call_id).await?;
        let leg = self.live_phone_leg(&call_id).await?;
        if leg.direction != PhoneCallDirection::Inbound {
            return Err(CallError::InvalidRequest(
                "Only incoming phone calls can be answered".to_string(),
            ));
        }
        if leg.status != PhoneCallStatus::Ringing {
            return Err(CallError::Conflict(
                "This call was already answered".to_string(),
            ));
        }
        if call.created_by != actor.as_ref() {
            return Err(CallError::Forbidden(
                "Only the owner of the number can answer this call".to_string(),
            ));
        }

        let token = self
            .rtc_client
            .generate_token(&call.room_name, actor.copied())
            .await
            .map_err(CallError::Internal)?;
        self.leave_other_active_call(actor.copied(), call.id)
            .await?;
        match self.repo.add_participant(&call.id, actor.copied()).await {
            Ok(_) => {}
            Err(AddParticipantError::UserAlreadyActive) => {
                return Err(CallError::AlreadyInCall("another call".to_string()));
            }
            Err(AddParticipantError::Repository(error)) => return Err(CallError::Internal(error)),
        }
        // The SIP call itself connects once the answering client publishes
        // its microphone; until then the caller keeps hearing it ring.
        let Some(leg) = self
            .phone_repo
            .update_phone_leg(&call.id, PhoneLegUpdate::answered(Utc::now(), None))
            .await?
        else {
            self.repo
                .remove_participant(&call.id, actor.copied())
                .await
                .map_err(|e| CallError::Internal(e.into()))?;
            return Err(CallError::NotFound("This call has ended".to_string()));
        };
        self.send_phone_call_updated(actor.copied(), call.id, &leg)
            .await;
        Ok(PhoneCallJoinResponse {
            call: CallTokenResponse {
                participant_id: actor.to_string(),
                share_token: None,
                call_id: call.id,
                channel_id: None,
                token,
                room_name: call.room_name,
                server_url: self.server_url.clone(),
            },
            phone: leg,
        })
    }

    #[tracing::instrument(err, skip(self, receipt))]
    pub(super) async fn hang_up(
        &self,
        receipt: EntityAccessReceipt<ViewAccessLevel>,
    ) -> Result<LeaveCallResponse, CallError> {
        let (call_id, actor) = phone_call_receipt(&receipt)?;
        let call = self.live_call(&call_id).await?;
        let leg = self.live_phone_leg(&call_id).await?;
        let is_participant = self
            .repo
            .is_participant(&call.id, actor.as_ref())
            .await
            .map_err(|e| CallError::Internal(e.into()))?;
        if call.created_by != actor.as_ref() && !is_participant {
            return Err(CallError::Forbidden(
                "Only people on this call can hang it up".to_string(),
            ));
        }
        let outcome = leg.status.hung_up(leg.direction);
        let updated = self
            .phone_repo
            .update_phone_leg(&call.id, PhoneLegUpdate::ended(outcome, Utc::now()))
            .await?;
        if let Some(leg) = &updated {
            self.notify_phone_call_owner(&call, leg).await;
        }
        self.end_phone_call(&call).await?;
        Ok(LeaveCallResponse { call_ended: true })
    }

    #[tracing::instrument(err, skip(self))]
    pub(super) async fn incoming_calls(
        &self,
        actor: MacroUserIdStr<'_>,
    ) -> Result<IncomingPhoneCallsResponse, CallError> {
        Ok(IncomingPhoneCallsResponse {
            calls: self.phone_repo.list_ringing_phone_calls(actor).await?,
        })
    }

    #[tracing::instrument(err, skip(self, request))]
    pub(super) async fn assign_number(
        &self,
        number: PhoneNumber,
        request: AssignPhoneNumberRequest,
    ) -> Result<(), CallError> {
        self.phone_repo
            .assign_phone_number(&number, request.user_id)
            .await
    }

    #[tracing::instrument(err, skip(self))]
    pub(super) async fn release_number(&self, number: PhoneNumber) -> Result<(), CallError> {
        if self.phone_repo.release_phone_number(&number).await? {
            Ok(())
        } else {
            Err(CallError::NotFound(format!("{number} is not assigned")))
        }
    }

    /// A SIP participant joined a room. Outbound legs are already known; a
    /// caller placed by an inbound dispatch rule becomes a new call that
    /// rings the owner of the number they dialed.
    #[tracing::instrument(err, skip(self, sip), fields(identity = %sip.identity))]
    pub(super) async fn handle_sip_participant_joined(
        &self,
        room_name: &str,
        sip: &SipParticipant,
    ) -> Result<(), CallError> {
        if self
            .repo
            .get_call_by_room_name(room_name)
            .await
            .map_err(|e| CallError::Internal(e.into()))?
            .is_some()
        {
            return Ok(());
        }
        if !sip.is_inbound {
            tracing::warn!("SIP participant joined a room that has no call; ignoring");
            return Ok(());
        }
        let (Some(caller), Some(dialed)) = (&sip.phone_number, &sip.trunk_phone_number) else {
            // Withheld caller ids cannot be matched, returned, or shown.
            tracing::info!("rejecting inbound phone call without caller and dialed numbers");
            self.reject_inbound_call(room_name).await;
            return Ok(());
        };
        let Some(owner) = self.phone_repo.phone_number_owner(dialed).await? else {
            tracing::warn!(%dialed, "rejecting inbound phone call to an unassigned number");
            self.reject_inbound_call(room_name).await;
            return Ok(());
        };

        let contact = self.find_phone_contact(owner.copied(), caller).await;
        let call_id = Uuid::now_v7();
        let Some(call) = self
            .phone_repo
            .create_inbound_phone_call(NewPhoneCall {
                call_id,
                room_name: room_name.to_string(),
                owner: owner.clone(),
                leg: NewPhoneLeg {
                    direction: PhoneCallDirection::Inbound,
                    remote_number: caller.clone(),
                    local_number: Some(dialed.clone()),
                    participant_identity: sip.identity.clone(),
                    status: PhoneCallStatus::Ringing,
                    contact: contact.clone(),
                    sip_call_id: sip.sip_call_id.clone(),
                },
            })
            .await?
        else {
            return Ok(());
        };
        self.publish_call_event(&CallMacroEvent::started(CallStartedMetadata {
            call_id: call.id,
            channel_id: None,
            created_by: owner.clone(),
            created_at: call.created_at,
            recording_enabled: self.egress_s3_config.is_some(),
        }));
        // Neither the recorder nor the transcriber publishes audio, and an
        // inbound SIP call is only answered once someone in the room does, so
        // the caller keeps hearing it ring until the owner picks up.
        self.spawn_inbound_phone_media(call.id, call.room_name.clone());

        let incoming = IncomingPhoneCall {
            call_id: call.id,
            from: caller.clone(),
            to: Some(dialed.clone()),
            contact,
            started_at: call.created_at,
        };
        self.connection_service
            .send_channel_message(
                &[owner],
                PHONE_CALL_INCOMING_EVENT,
                serde_json::to_value(&incoming).map_err(|e| CallError::Internal(e.into()))?,
            )
            .await
            .inspect_err(|e| tracing::error!(error=?e, "failed to ring phone number owner"))
            .ok();
        Ok(())
    }

    fn spawn_inbound_phone_media(&self, call_id: Uuid, room_name: String) {
        let rtc = self.rtc_client.clone();
        let repo = self.repo.clone();
        let egress = self.egress_s3_config.clone();
        tokio::spawn(
            async move {
                let transcription = async {
                    rtc.dispatch_transcription_agent(&room_name)
                        .await
                        .inspect_err(|e| tracing::error!(error=?e, "failed to dispatch phone call transcription agent"))
                        .ok();
                };
                let recording =
                    start_meeting_recording(&repo, rtc.as_ref(), call_id, &room_name, egress.as_ref());
                tokio::join!(transcription, recording);
            }
            .instrument(tracing::info_span!("start_phone_call_media", %call_id)),
        );
    }

    /// A SIP participant left its room: the person on the phone hung up, or
    /// an inbound call stopped ringing. The call ends with its phone party.
    #[tracing::instrument(err, skip(self, sip), fields(identity = %sip.identity))]
    pub(super) async fn handle_sip_participant_left(
        &self,
        room_name: &str,
        sip: &SipParticipant,
    ) -> Result<(), CallError> {
        let Some(call) = self
            .repo
            .get_call_by_room_name(room_name)
            .await
            .map_err(|e| CallError::Internal(e.into()))?
        else {
            return Ok(());
        };
        let Some(leg) = self.phone_repo.get_live_phone_leg(&call.id).await? else {
            return Ok(());
        };
        if leg.participant_identity != sip.identity {
            return Ok(());
        }
        // An outbound call that never connected is concluded by its dial,
        // which knows why it failed (busy, declined, …) and ends the call.
        if leg.direction == PhoneCallDirection::Outbound && leg.status == PhoneCallStatus::Dialing {
            return Ok(());
        }
        let updated = self
            .phone_repo
            .update_phone_leg(
                &call.id,
                PhoneLegUpdate::ended(leg.status.concluded(leg.direction), Utc::now()),
            )
            .await?;
        if let Some(leg) = &updated {
            self.notify_phone_call_owner(&call, leg).await;
        }
        self.end_phone_call(&call).await?;
        Ok(())
    }

    /// End a phone call for everyone and archive it.
    async fn end_phone_call(&self, call: &Call) -> Result<(), CallError> {
        let participants = self
            .repo
            .get_participants(&call.id)
            .await
            .map_err(|e| CallError::Internal(e.into()))?;
        for participant in participants {
            let user_id = MacroUserIdStr::parse_from_str(&participant.user_id)
                .map_err(|e| CallError::Internal(e.into()))?;
            self.repo
                .remove_participant(&call.id, user_id)
                .await
                .map_err(|e| CallError::Internal(e.into()))?;
        }
        // Archiving deletes the room, disconnecting everyone still in it,
        // including the phone party.
        if !self.finish_empty_call(call).await? {
            self.rtc_client
                .delete_room(&call.room_name)
                .await
                .inspect_err(|e| tracing::error!(error=?e, "failed to delete phone call room"))
                .ok();
        }
        Ok(())
    }

    async fn reject_inbound_call(&self, room_name: &str) {
        self.rtc_client
            .delete_room(room_name)
            .await
            .inspect_err(|e| tracing::error!(error=?e, "failed to reject inbound phone call"))
            .ok();
    }

    /// Best-effort CRM lookup of the person at `number` as `user_id` sees them.
    async fn find_phone_contact(
        &self,
        user_id: MacroUserIdStr<'_>,
        number: &PhoneNumber,
    ) -> Option<PhoneContact> {
        self.phone_contacts
            .find_contact(user_id, number)
            .await
            .inspect_err(|error| tracing::warn!(error=?error, "failed to look up phone contact"))
            .ok()
            .flatten()
    }

    async fn live_call(&self, call_id: &Uuid) -> Result<Call, CallError> {
        self.repo
            .get_call_by_id(call_id)
            .await
            .map_err(|e| CallError::Internal(e.into()))?
            .ok_or_else(|| CallError::NotFound("This call has ended".to_string()))
    }

    async fn live_phone_leg(&self, call_id: &Uuid) -> Result<PhoneLeg, CallError> {
        self.phone_repo
            .get_live_phone_leg(call_id)
            .await?
            .ok_or_else(|| CallError::NotFound("This isn't a phone call".to_string()))
    }

    async fn notify_phone_call_owner(&self, call: &Call, leg: &PhoneLeg) {
        match MacroUserIdStr::parse_from_str(&call.created_by) {
            Ok(owner) => self.send_phone_call_updated(owner, call.id, leg).await,
            Err(error) => tracing::error!(
                error=?error, call_id=%call.id, "failed to parse stored phone call owner"
            ),
        }
    }

    async fn send_phone_call_updated(
        &self,
        user_id: MacroUserIdStr<'_>,
        call_id: Uuid,
        leg: &PhoneLeg,
    ) {
        send_phone_call_updated(self.connection_service.as_ref(), user_id, call_id, leg).await;
    }
}

/// Tell every device of `user_id` that a phone leg changed, so a ringing
/// call stops ringing once it is answered elsewhere or ends, and a caller
/// learns why a call did not connect.
async fn send_phone_call_updated<Cn: ConnectionService>(
    connection_service: &Cn,
    user_id: MacroUserIdStr<'_>,
    call_id: Uuid,
    leg: &PhoneLeg,
) {
    connection_service
        .send_channel_message(
            &[user_id],
            PHONE_CALL_UPDATED_EVENT,
            serde_json::json!(PhoneCallUpdated {
                call_id,
                phone: leg
            }),
        )
        .await
        .inspect_err(|e| tracing::error!(error=?e, "failed to send phone call update"))
        .ok();
}

/// The call id and acting user of a receipt addressing a call.
fn phone_call_receipt(
    receipt: &EntityAccessReceipt<ViewAccessLevel>,
) -> Result<(Uuid, MacroUserIdStr<'static>), CallError> {
    let entity = receipt.entity();
    if entity.entity_type != EntityType::Call {
        return Err(CallError::Internal(anyhow::anyhow!(
            "expected Call entity in receipt, got {:?}",
            entity.entity_type
        )));
    }
    let call_id = Uuid::parse_str(&entity.entity_id)
        .map_err(|_| CallError::Internal(anyhow::anyhow!("invalid call_id in receipt")))?;
    let actor = receipt
        .get_authenticated_user()
        .map_err(|_| CallError::Auth)?
        .clone();
    Ok((call_id, actor))
}

/// Record how an outbound SIP leg went, returning the updated leg. `None`
/// means the leg had already ended — the caller hung up first and the call
/// is being archived without the dial's help.
async fn record_dial_outcome<Ph: PhoneCallRepository>(
    phone_repo: &Ph,
    call_id: Uuid,
    outcome: Result<SipDialAnswered, DialFailure>,
) -> Option<PhoneLeg> {
    let update = match outcome {
        Ok(answered) => PhoneLegUpdate::answered(Utc::now(), answered.sip_call_id),
        Err(failure) => {
            tracing::info!(?failure, "phone call did not connect");
            PhoneLegUpdate::ended(failure.status(), Utc::now())
        }
    };
    phone_repo
        .update_phone_leg(&call_id, update)
        .await
        .inspect_err(|e| tracing::error!(error=?e, "failed to record phone call outcome"))
        .ok()
        .flatten()
}
