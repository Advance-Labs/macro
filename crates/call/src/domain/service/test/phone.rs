use super::*;

use std::time::Duration as StdDuration;

use entity_access::domain::models::ViewAccessLevel;

use crate::domain::phone::{
    AssignPhoneNumberRequest, DialFailure, DialPhoneRequest, IncomingPhoneCall, NewPhoneCall,
    PhoneCallDirection, PhoneCallStatus, PhoneContact, PhoneDialingConfig, PhoneLeg,
    PhoneLegUpdate, PhoneNumber, SipDialAnswered, SipDialRequest, SipParticipant,
};
use crate::domain::ports::phone::{PhoneCallRepository, PhoneContactDirectory};

const OWNER: &str = "owner@example.com";
const CALLEE: &str = "+15552345678";
const OWNER_NUMBER: &str = "+15559876543";
const DEFAULT_CALLER_ID: &str = "+15552221111";
const TRUNK_ID: &str = "ST_outbound";
const INBOUND_ROOM: &str = "phone_+15552345678_abcd";
const CONTACT_ID: Uuid = Uuid::from_u128(0x0198a1b2_c3d4_7e5f_8061_000000000c01);

fn number(value: &str) -> PhoneNumber {
    PhoneNumber::parse(value).unwrap()
}

fn owner() -> MacroUserIdStr<'static> {
    user(OWNER)
}

/// In-memory phone persistence with the same guarded-update contract as the
/// Postgres adapter.
#[derive(Clone, Default)]
struct FakePhoneRepo {
    state: Arc<Mutex<FakePhoneState>>,
}

#[derive(Default)]
struct FakePhoneState {
    numbers: HashMap<PhoneNumber, MacroUserIdStr<'static>>,
    legs: HashMap<Uuid, PhoneLeg>,
    created: Vec<NewPhoneCall>,
    fail_create: bool,
}

impl FakePhoneRepo {
    fn with_number(self, phone_number: &str, owner: MacroUserIdStr<'static>) -> Self {
        self.state
            .lock()
            .unwrap()
            .numbers
            .insert(number(phone_number), owner);
        self
    }

    fn with_leg(self, call_id: Uuid, leg: PhoneLeg) -> Self {
        self.state.lock().unwrap().legs.insert(call_id, leg);
        self
    }

    fn failing_creation(self) -> Self {
        self.state.lock().unwrap().fail_create = true;
        self
    }

    fn leg(&self, call_id: &Uuid) -> Option<PhoneLeg> {
        self.state.lock().unwrap().legs.get(call_id).cloned()
    }

    fn created(&self) -> Vec<NewPhoneCall> {
        self.state.lock().unwrap().created.clone()
    }

    fn create(&self, call: NewPhoneCall) -> Result<Call, CallError> {
        let mut state = self.state.lock().unwrap();
        if state.fail_create {
            return Err(CallError::AlreadyInCall("another call".to_string()));
        }
        let leg = PhoneLeg {
            direction: call.leg.direction,
            remote_number: call.leg.remote_number.clone(),
            local_number: call.leg.local_number.clone(),
            participant_identity: call.leg.participant_identity.clone(),
            status: call.leg.status,
            contact: call.leg.contact.clone(),
            answered_at: None,
            ended_at: None,
        };
        state.legs.insert(call.call_id, leg);
        let created = Call {
            id: call.call_id,
            channel_id: None,
            room_name: call.room_name.clone(),
            created_by: call.owner.to_string(),
            created_at: archived_event_started_at(),
            egress_id: None,
        };
        state.created.push(call);
        Ok(created)
    }
}

impl PhoneCallRepository for FakePhoneRepo {
    async fn phone_numbers_for_user(
        &self,
        user_id: MacroUserIdStr<'_>,
    ) -> Result<Vec<PhoneNumber>, CallError> {
        let mut numbers: Vec<_> = self
            .state
            .lock()
            .unwrap()
            .numbers
            .iter()
            .filter(|(_, owner)| owner.as_ref() == user_id.as_ref())
            .map(|(number, _)| number.clone())
            .collect();
        numbers.sort();
        Ok(numbers)
    }

    async fn phone_number_owner(
        &self,
        number: &PhoneNumber,
    ) -> Result<Option<MacroUserIdStr<'static>>, CallError> {
        Ok(self.state.lock().unwrap().numbers.get(number).cloned())
    }

    async fn assign_phone_number(
        &self,
        number: &PhoneNumber,
        user_id: MacroUserIdStr<'_>,
    ) -> Result<(), CallError> {
        self.state
            .lock()
            .unwrap()
            .numbers
            .insert(number.clone(), user_id.into_owned());
        Ok(())
    }

    async fn release_phone_number(&self, number: &PhoneNumber) -> Result<bool, CallError> {
        Ok(self.state.lock().unwrap().numbers.remove(number).is_some())
    }

    async fn create_outbound_phone_call(&self, call: NewPhoneCall) -> Result<Call, CallError> {
        self.create(call)
    }

    async fn create_inbound_phone_call(
        &self,
        call: NewPhoneCall,
    ) -> Result<Option<Call>, CallError> {
        self.create(call).map(Some)
    }

    async fn get_live_phone_leg(&self, call_id: &Uuid) -> Result<Option<PhoneLeg>, CallError> {
        Ok(self.leg(call_id))
    }

    async fn update_phone_leg(
        &self,
        call_id: &Uuid,
        update: PhoneLegUpdate,
    ) -> Result<Option<PhoneLeg>, CallError> {
        let mut state = self.state.lock().unwrap();
        let Some(leg) = state.legs.get_mut(call_id) else {
            return Ok(None);
        };
        if !leg.status.is_live() {
            return Ok(None);
        }
        leg.status = update.status;
        leg.answered_at = update.answered_at.or(leg.answered_at);
        leg.ended_at = update.ended_at.or(leg.ended_at);
        Ok(Some(leg.clone()))
    }

    async fn list_ringing_phone_calls(
        &self,
        _user_id: MacroUserIdStr<'_>,
    ) -> Result<Vec<IncomingPhoneCall>, CallError> {
        Ok(Vec::new())
    }
}

/// A CRM that knows one contact.
#[derive(Clone, Default)]
struct FakeContacts {
    contact: Option<(PhoneNumber, PhoneContact)>,
}

impl FakeContacts {
    fn knowing(phone_number: &str, name: &str) -> Self {
        Self {
            contact: Some((
                number(phone_number),
                PhoneContact {
                    contact_id: CONTACT_ID,
                    name: Some(name.to_string()),
                },
            )),
        }
    }
}

impl PhoneContactDirectory for FakeContacts {
    async fn find_contact(
        &self,
        _user_id: MacroUserIdStr<'_>,
        number: &PhoneNumber,
    ) -> Result<Option<PhoneContact>, rootcause::Report> {
        Ok(self
            .contact
            .as_ref()
            .filter(|(known, _)| known == number)
            .map(|(_, contact)| contact.clone()))
    }
}

fn dialing() -> PhoneDialingConfig {
    PhoneDialingConfig {
        outbound_trunk_id: TRUNK_ID.to_string(),
        default_caller_id: Some(number(DEFAULT_CALLER_ID)),
        allowed_country_codes: vec!["1".to_string()],
    }
}

fn phone_service(
    repo: MockCallRepository,
    rtc: MockCallRtcClient,
    connection: RecordingConnectionService,
    phone_repo: FakePhoneRepo,
    contacts: FakeContacts,
    dialing: Option<PhoneDialingConfig>,
) -> impl CallService {
    let service: BaseWebhookCallService<RecordingConnectionService> = CallServiceImpl::new(
        repo,
        rtc,
        connection,
        NoOpEntityAccessService,
        StubNotificationIngress,
        StubRecordingStorage,
        "wss://livekit.example.com",
    );
    let service = service
        .with_event_broker(RecordingEventBroker::default())
        .with_phone(phone_repo, contacts);
    match dialing {
        Some(config) => service.with_phone_dialing(config),
        None => service,
    }
}

/// Wait for work a spawned task does after the request returned.
async fn eventually(mut done: impl FnMut() -> bool) {
    tokio::time::timeout(StdDuration::from_secs(5), async {
        while !done() {
            tokio::time::sleep(StdDuration::from_millis(5)).await;
        }
    })
    .await
    .expect("condition was not met in time");
}

fn no_other_call(repo: &mut MockCallRepository) {
    repo.expect_find_active_call_for_user()
        .returning(|_| Box::pin(async { Ok(None) }));
}

fn phone_call(call_id: Uuid, room_name: &str) -> Call {
    Call {
        id: call_id,
        channel_id: None,
        room_name: room_name.to_string(),
        created_by: owner().to_string(),
        created_at: archived_event_started_at(),
        egress_id: None,
    }
}

fn leg(direction: PhoneCallDirection, status: PhoneCallStatus) -> PhoneLeg {
    PhoneLeg {
        direction,
        remote_number: number(CALLEE),
        local_number: Some(number(OWNER_NUMBER)),
        participant_identity: format!("sip_{CALLEE}"),
        status,
        contact: None,
        answered_at: None,
        ended_at: None,
    }
}

fn call_receipt(call_id: Uuid, actor: MacroUserIdStr<'static>) -> EntityAccessReceipt<ViewAccessLevel> {
    EntityAccessReceipt::dangerously_assert_authenticated_user(
        actor,
        &call_id.to_string(),
        EntityType::Call,
    )
}

/// Expect the call to end: everyone is removed and the empty call archived.
fn expect_call_ends(repo: &mut MockCallRepository, rtc: &mut MockCallRtcClient, call_id: Uuid) {
    repo.expect_get_participants().returning(move |_| {
        Box::pin(async move {
            Ok(vec![CallParticipant {
                call_id,
                user_id: owner().to_string(),
                joined_at: archived_event_started_at(),
            }])
        })
    });
    repo.expect_remove_participant()
        .times(1)
        .returning(|_, _| Box::pin(async { Ok(()) }));
    repo.expect_get_participant_count()
        .returning(|_| Box::pin(async { Ok(0) }));
    repo.expect_archive_call_if_empty()
        .times(1)
        .returning(move |_| {
            Box::pin(async move {
                Ok(Some(ArchivedCall {
                    call_id,
                    channel_id: None,
                    created_by: owner().to_string(),
                    started_at: archived_event_started_at(),
                    ended_at: archived_event_ended_at(),
                    duration_ms: 1_000,
                    has_recording: false,
                    participant_count: 1,
                }))
            })
        });
    rtc.expect_delete_room().returning(|_| Box::pin(async { Ok(()) }));
}

#[tokio::test]
async fn dialing_needs_a_configured_trunk() {
    let service = phone_service(
        MockCallRepository::new(),
        MockCallRtcClient::new(),
        RecordingConnectionService::default(),
        FakePhoneRepo::default(),
        FakeContacts::default(),
        None,
    );
    let error = service
        .dial_phone(owner(), DialPhoneRequest { to: CALLEE.to_string() })
        .await
        .unwrap_err();
    assert!(matches!(error, CallError::Unavailable(_)), "{error:?}");
}

#[tokio::test]
async fn dialing_rejects_numbers_that_cannot_or_may_not_be_called() {
    for to in ["not a number", "911", "+33 1 42 68 53 00", "+1 900 234 5678"] {
        let service = phone_service(
            MockCallRepository::new(),
            MockCallRtcClient::new(),
            RecordingConnectionService::default(),
            FakePhoneRepo::default(),
            FakeContacts::default(),
            Some(dialing()),
        );
        let error = service
            .dial_phone(owner(), DialPhoneRequest { to: to.to_string() })
            .await
            .unwrap_err();
        assert!(matches!(error, CallError::InvalidRequest(_)), "{to}: {error:?}");
    }
}

#[tokio::test]
async fn dialing_creates_an_owned_call_rings_the_callee_and_records_the_answer() {
    let mut repo = MockCallRepository::new();
    no_other_call(&mut repo);
    let mut rtc = MockCallRtcClient::new();
    rtc.expect_generate_token()
        .returning(|room, identity| {
            let token = format!("token:{room}:{identity}");
            Box::pin(async move { Ok(token) })
        });
    rtc.expect_create_room()
        .times(1)
        .returning(|_| Box::pin(async { Ok(()) }));
    rtc.expect_dispatch_transcription_agent()
        .times(1)
        .returning(|_| Box::pin(async { Ok(()) }));
    let dialed: Arc<Mutex<Option<SipDialRequest>>> = Arc::default();
    let dialed_in_mock = dialed.clone();
    rtc.expect_dial_sip_participant()
        .times(1)
        .returning(move |request| {
            *dialed_in_mock.lock().unwrap() = Some(request);
            Box::pin(async {
                Ok(SipDialAnswered {
                    sip_call_id: Some("SCL_1".to_string()),
                })
            })
        });
    let phone_repo = FakePhoneRepo::default().with_number(OWNER_NUMBER, owner());
    let service = phone_service(
        repo,
        rtc,
        RecordingConnectionService::default(),
        phone_repo.clone(),
        FakeContacts::knowing(CALLEE, "Ada Lovelace"),
        Some(dialing()),
    );

    let response = service
        .dial_phone(
            owner(),
            DialPhoneRequest {
                to: "(555) 234-5678 ext. 89".to_string(),
            },
        )
        .await
        .unwrap();

    let call_id = response.call.call_id;
    assert_eq!(response.call.room_name, call_id.to_string());
    assert_eq!(response.call.channel_id, None);
    assert_eq!(
        response.call.token,
        format!("token:{call_id}:{}", owner().as_ref())
    );
    assert_eq!(response.phone.status, PhoneCallStatus::Dialing);
    assert_eq!(response.phone.remote_number, number(CALLEE));
    assert_eq!(response.phone.local_number, Some(number(OWNER_NUMBER)));
    assert_eq!(
        response.phone.contact.as_ref().map(|c| c.contact_id),
        Some(CONTACT_ID)
    );

    let created = phone_repo.created();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].owner.as_ref(), owner().as_ref());
    assert_eq!(created[0].leg.direction, PhoneCallDirection::Outbound);

    eventually(|| {
        phone_repo
            .leg(&call_id)
            .is_some_and(|leg| leg.status == PhoneCallStatus::Active)
    })
    .await;
    assert!(phone_repo.leg(&call_id).unwrap().answered_at.is_some());
    let dialed = dialed.lock().unwrap().clone().expect("dialed");
    assert_eq!(dialed.room_name, call_id.to_string());
    assert_eq!(dialed.trunk_id, TRUNK_ID);
    assert_eq!(dialed.to, number(CALLEE));
    assert_eq!(dialed.caller_id, Some(number(OWNER_NUMBER)));
    assert_eq!(dialed.participant_identity, format!("sip_{CALLEE}"));
    assert_eq!(dialed.participant_name, "Ada Lovelace");
    assert_eq!(dialed.dtmf.as_deref(), Some("wwww89"));
}

#[tokio::test]
async fn callers_without_a_number_use_the_default_caller_id() {
    let mut repo = MockCallRepository::new();
    no_other_call(&mut repo);
    let mut rtc = MockCallRtcClient::new();
    rtc.expect_generate_token()
        .returning(|_, _| Box::pin(async { Ok("token".to_string()) }));
    rtc.expect_create_room().returning(|_| Box::pin(async { Ok(()) }));
    rtc.expect_dispatch_transcription_agent()
        .returning(|_| Box::pin(async { Ok(()) }));
    rtc.expect_dial_sip_participant()
        .returning(|_| Box::pin(async { Ok(SipDialAnswered { sip_call_id: None }) }));
    let service = phone_service(
        repo,
        rtc,
        RecordingConnectionService::default(),
        FakePhoneRepo::default(),
        FakeContacts::default(),
        Some(dialing()),
    );
    let response = service
        .dial_phone(owner(), DialPhoneRequest { to: CALLEE.to_string() })
        .await
        .unwrap();
    assert_eq!(response.phone.local_number, Some(number(DEFAULT_CALLER_ID)));
    assert_eq!(response.phone.contact, None);
}

#[tokio::test]
async fn unconnected_dials_record_why_and_end_the_call() {
    for (failure, outcome) in [
        (DialFailure::Busy, PhoneCallStatus::Busy),
        (DialFailure::NoAnswer, PhoneCallStatus::NoAnswer),
        (DialFailure::Declined, PhoneCallStatus::Declined),
        (DialFailure::Unreachable, PhoneCallStatus::Failed),
    ] {
        let mut repo = MockCallRepository::new();
        no_other_call(&mut repo);
        let mut rtc = MockCallRtcClient::new();
        rtc.expect_generate_token()
            .returning(|_, _| Box::pin(async { Ok("token".to_string()) }));
        rtc.expect_create_room().returning(|_| Box::pin(async { Ok(()) }));
        rtc.expect_dispatch_transcription_agent()
            .returning(|_| Box::pin(async { Ok(()) }));
        rtc.expect_dial_sip_participant()
            .returning(move |_| Box::pin(async move { Err(failure) }));
        let deleted: Arc<Mutex<Vec<String>>> = Arc::default();
        let deleted_in_mock = deleted.clone();
        rtc.expect_delete_room().returning(move |room| {
            deleted_in_mock.lock().unwrap().push(room.to_string());
            Box::pin(async { Ok(()) })
        });
        let phone_repo = FakePhoneRepo::default();
        let service = phone_service(
            repo,
            rtc,
            RecordingConnectionService::default(),
            phone_repo.clone(),
            FakeContacts::default(),
            Some(dialing()),
        );
        let response = service
            .dial_phone(owner(), DialPhoneRequest { to: CALLEE.to_string() })
            .await
            .unwrap();
        let call_id = response.call.call_id;
        eventually(|| !deleted.lock().unwrap().is_empty()).await;
        assert_eq!(*deleted.lock().unwrap(), vec![call_id.to_string()]);
        let leg = phone_repo.leg(&call_id).unwrap();
        assert_eq!(leg.status, outcome, "{failure:?}");
        assert!(leg.ended_at.is_some());
    }
}

#[tokio::test]
async fn a_call_that_cannot_be_created_releases_its_room() {
    let mut repo = MockCallRepository::new();
    no_other_call(&mut repo);
    let mut rtc = MockCallRtcClient::new();
    rtc.expect_generate_token()
        .returning(|_, _| Box::pin(async { Ok("token".to_string()) }));
    rtc.expect_create_room().returning(|_| Box::pin(async { Ok(()) }));
    rtc.expect_delete_room()
        .times(1)
        .returning(|_| Box::pin(async { Ok(()) }));
    rtc.expect_dial_sip_participant().never();
    let service = phone_service(
        repo,
        rtc,
        RecordingConnectionService::default(),
        FakePhoneRepo::default().failing_creation(),
        FakeContacts::default(),
        Some(dialing()),
    );
    let error = service
        .dial_phone(owner(), DialPhoneRequest { to: CALLEE.to_string() })
        .await
        .unwrap_err();
    assert!(matches!(error, CallError::AlreadyInCall(_)), "{error:?}");
}

fn inbound_sip(phone_number: Option<&str>, trunk_number: Option<&str>) -> SipParticipant {
    SipParticipant {
        identity: format!("sip_{CALLEE}"),
        phone_number: phone_number.map(number),
        trunk_phone_number: trunk_number.map(number),
        call_status: None,
        sip_call_id: Some("SCL_inbound".to_string()),
        is_inbound: true,
    }
}

fn sip_webhook(event: &str, room: &str, sip: SipParticipant) -> MockCallRtcClient {
    let event = CallWebhookEvent {
        event: event.to_string(),
        id: "EV_sip".to_string(),
        room_name: Some(room.to_string()),
        participant_identity: None,
        guest_identity: None,
        sip_participant: Some(sip),
        egress_id: None,
        file_url: None,
        created_at: 0,
    };
    let mut rtc = MockCallRtcClient::new();
    rtc.expect_receive_webhook()
        .returning(move |_, _| Ok(event.clone()));
    rtc
}

#[tokio::test]
async fn inbound_callers_ring_the_owner_of_the_number_they_dialed() {
    let mut repo = MockCallRepository::new();
    repo.expect_get_call_by_room_name()
        .returning(|_| Box::pin(async { Ok(None) }));
    let mut rtc = sip_webhook(
        "participant_joined",
        INBOUND_ROOM,
        inbound_sip(Some(CALLEE), Some(OWNER_NUMBER)),
    );
    rtc.expect_dispatch_transcription_agent()
        .times(1)
        .returning(|_| Box::pin(async { Ok(()) }));
    rtc.expect_delete_room().never();
    let connection = RecordingConnectionService::default();
    let phone_repo = FakePhoneRepo::default().with_number(OWNER_NUMBER, owner());
    let service = phone_service(
        repo,
        rtc,
        connection.clone(),
        phone_repo.clone(),
        FakeContacts::knowing(CALLEE, "Ada Lovelace"),
        None,
    );

    service.process_webhook_event("body", "token").await.unwrap();

    let created = phone_repo.created();
    assert_eq!(created.len(), 1);
    let call = &created[0];
    assert_eq!(call.room_name, INBOUND_ROOM);
    assert_eq!(call.owner.as_ref(), owner().as_ref());
    assert_eq!(call.leg.direction, PhoneCallDirection::Inbound);
    assert_eq!(call.leg.status, PhoneCallStatus::Ringing);
    assert_eq!(call.leg.remote_number, number(CALLEE));
    assert_eq!(call.leg.local_number, Some(number(OWNER_NUMBER)));
    assert_eq!(call.leg.sip_call_id.as_deref(), Some("SCL_inbound"));

    let messages = connection.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].message_type, "phone_call_incoming");
    assert_eq!(messages[0].users, vec![owner().to_string()]);
    assert_eq!(messages[0].message["from"], json!(CALLEE));
    assert_eq!(messages[0].message["contact"]["name"], json!("Ada Lovelace"));
    assert_eq!(
        messages[0].message["callId"],
        json!(call.call_id.to_string())
    );
}

#[tokio::test]
async fn inbound_calls_without_an_owner_or_caller_id_are_rejected() {
    for sip in [
        inbound_sip(Some(CALLEE), Some(OWNER_NUMBER)),
        inbound_sip(None, Some(OWNER_NUMBER)),
        inbound_sip(Some(CALLEE), None),
    ] {
        let mut repo = MockCallRepository::new();
        repo.expect_get_call_by_room_name()
            .returning(|_| Box::pin(async { Ok(None) }));
        let mut rtc = sip_webhook("participant_joined", INBOUND_ROOM, sip);
        rtc.expect_delete_room()
            .times(1)
            .withf(|room| room == INBOUND_ROOM)
            .returning(|_| Box::pin(async { Ok(()) }));
        let connection = RecordingConnectionService::default();
        // No number is assigned, so even a complete call has nobody to ring.
        let phone_repo = FakePhoneRepo::default();
        let service = phone_service(
            repo,
            rtc,
            connection.clone(),
            phone_repo.clone(),
            FakeContacts::default(),
            None,
        );
        service.process_webhook_event("body", "token").await.unwrap();
        assert!(phone_repo.created().is_empty());
        assert!(connection.messages().is_empty());
    }
}

#[tokio::test]
async fn repeated_inbound_webhooks_do_not_ring_twice() {
    let mut repo = MockCallRepository::new();
    repo.expect_get_call_by_room_name()
        .returning(|room| {
            let call = phone_call(ARCHIVED_EVENT_CALL_ID, room);
            Box::pin(async move { Ok(Some(call)) })
        });
    let rtc = sip_webhook(
        "participant_joined",
        INBOUND_ROOM,
        inbound_sip(Some(CALLEE), Some(OWNER_NUMBER)),
    );
    let connection = RecordingConnectionService::default();
    let phone_repo = FakePhoneRepo::default().with_number(OWNER_NUMBER, owner());
    let service = phone_service(
        repo,
        rtc,
        connection.clone(),
        phone_repo.clone(),
        FakeContacts::default(),
        None,
    );
    service.process_webhook_event("body", "token").await.unwrap();
    assert!(phone_repo.created().is_empty());
    assert!(connection.messages().is_empty());
}

#[tokio::test]
async fn answering_joins_the_owner_and_stops_ringing_on_their_other_devices() {
    let call_id = Uuid::now_v7();
    let mut repo = MockCallRepository::new();
    repo.expect_get_call_by_id().returning(move |_| {
        let call = phone_call(call_id, INBOUND_ROOM);
        Box::pin(async move { Ok(Some(call)) })
    });
    no_other_call(&mut repo);
    repo.expect_add_participant()
        .times(1)
        .returning(move |_, user_id| {
            let participant = CallParticipant {
                call_id,
                user_id: user_id.to_string(),
                joined_at: Utc::now(),
            };
            Box::pin(async move { Ok(participant) })
        });
    let mut rtc = MockCallRtcClient::new();
    rtc.expect_generate_token()
        .returning(|room, _| {
            let token = format!("token:{room}");
            Box::pin(async move { Ok(token) })
        });
    let connection = RecordingConnectionService::default();
    let phone_repo = FakePhoneRepo::default().with_leg(
        call_id,
        leg(PhoneCallDirection::Inbound, PhoneCallStatus::Ringing),
    );
    let service = phone_service(
        repo,
        rtc,
        connection.clone(),
        phone_repo.clone(),
        FakeContacts::default(),
        None,
    );

    let response = service
        .answer_phone_call(call_receipt(call_id, owner()))
        .await
        .unwrap();

    assert_eq!(response.call.token, format!("token:{INBOUND_ROOM}"));
    assert_eq!(response.call.room_name, INBOUND_ROOM);
    assert_eq!(response.phone.status, PhoneCallStatus::Active);
    assert!(response.phone.answered_at.is_some());
    let messages = connection.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].message_type, "phone_call_updated");
    assert_eq!(messages[0].message["phone"]["status"], json!("active"));
}

#[tokio::test]
async fn only_the_number_owner_can_answer_a_ringing_call() {
    let call_id = Uuid::now_v7();
    let mut repo = MockCallRepository::new();
    repo.expect_get_call_by_id().returning(move |_| {
        let call = phone_call(call_id, INBOUND_ROOM);
        Box::pin(async move { Ok(Some(call)) })
    });
    repo.expect_add_participant().never();
    let service = phone_service(
        repo,
        MockCallRtcClient::new(),
        RecordingConnectionService::default(),
        FakePhoneRepo::default().with_leg(
            call_id,
            leg(PhoneCallDirection::Inbound, PhoneCallStatus::Ringing),
        ),
        FakeContacts::default(),
        None,
    );
    let error = service
        .answer_phone_call(call_receipt(call_id, user("someone@example.com")))
        .await
        .unwrap_err();
    assert!(matches!(error, CallError::Forbidden(_)), "{error:?}");
}

#[tokio::test]
async fn only_ringing_inbound_calls_can_be_answered() {
    for (direction, status, conflict) in [
        (PhoneCallDirection::Inbound, PhoneCallStatus::Active, true),
        (PhoneCallDirection::Outbound, PhoneCallStatus::Dialing, false),
    ] {
        let call_id = Uuid::now_v7();
        let mut repo = MockCallRepository::new();
        repo.expect_get_call_by_id().returning(move |_| {
            let call = phone_call(call_id, INBOUND_ROOM);
            Box::pin(async move { Ok(Some(call)) })
        });
        let service = phone_service(
            repo,
            MockCallRtcClient::new(),
            RecordingConnectionService::default(),
            FakePhoneRepo::default().with_leg(call_id, leg(direction, status)),
            FakeContacts::default(),
            None,
        );
        let error = service
            .answer_phone_call(call_receipt(call_id, owner()))
            .await
            .unwrap_err();
        if conflict {
            assert!(matches!(error, CallError::Conflict(_)), "{error:?}");
        } else {
            assert!(matches!(error, CallError::InvalidRequest(_)), "{error:?}");
        }
    }
}

#[tokio::test]
async fn hanging_up_declines_cancels_or_completes_the_call_and_ends_it() {
    for (direction, status, outcome) in [
        (
            PhoneCallDirection::Inbound,
            PhoneCallStatus::Ringing,
            PhoneCallStatus::Declined,
        ),
        (
            PhoneCallDirection::Outbound,
            PhoneCallStatus::Dialing,
            PhoneCallStatus::Cancelled,
        ),
        (
            PhoneCallDirection::Outbound,
            PhoneCallStatus::Active,
            PhoneCallStatus::Completed,
        ),
    ] {
        let call_id = Uuid::now_v7();
        let mut repo = MockCallRepository::new();
        repo.expect_get_call_by_id().returning(move |_| {
            let call = phone_call(call_id, "phone-room");
            Box::pin(async move { Ok(Some(call)) })
        });
        repo.expect_is_participant()
            .returning(|_, _| Box::pin(async { Ok(true) }));
        let mut rtc = MockCallRtcClient::new();
        expect_call_ends(&mut repo, &mut rtc, call_id);
        let connection = RecordingConnectionService::default();
        let phone_repo = FakePhoneRepo::default().with_leg(call_id, leg(direction, status));
        let service = phone_service(
            repo,
            rtc,
            connection.clone(),
            phone_repo.clone(),
            FakeContacts::default(),
            None,
        );

        let response = service
            .hang_up_phone_call(call_receipt(call_id, owner()))
            .await
            .unwrap();

        assert!(response.call_ended);
        let leg = phone_repo.leg(&call_id).unwrap();
        assert_eq!(leg.status, outcome, "{direction:?} {status:?}");
        assert!(leg.ended_at.is_some());
        assert_eq!(connection.messages()[0].message_type, "phone_call_updated");
    }
}

#[tokio::test]
async fn people_outside_a_phone_call_cannot_hang_it_up() {
    let call_id = Uuid::now_v7();
    let mut repo = MockCallRepository::new();
    repo.expect_get_call_by_id().returning(move |_| {
        let call = phone_call(call_id, "phone-room");
        Box::pin(async move { Ok(Some(call)) })
    });
    repo.expect_is_participant()
        .returning(|_, _| Box::pin(async { Ok(false) }));
    repo.expect_archive_call_if_empty().never();
    let phone_repo = FakePhoneRepo::default().with_leg(
        call_id,
        leg(PhoneCallDirection::Outbound, PhoneCallStatus::Active),
    );
    let service = phone_service(
        repo,
        MockCallRtcClient::new(),
        RecordingConnectionService::default(),
        phone_repo.clone(),
        FakeContacts::default(),
        None,
    );
    let error = service
        .hang_up_phone_call(call_receipt(call_id, user("someone@example.com")))
        .await
        .unwrap_err();
    assert!(matches!(error, CallError::Forbidden(_)), "{error:?}");
    assert_eq!(
        phone_repo.leg(&call_id).unwrap().status,
        PhoneCallStatus::Active
    );
}

#[tokio::test]
async fn the_call_ends_when_the_person_on_the_phone_hangs_up() {
    for (direction, status, outcome) in [
        (
            PhoneCallDirection::Outbound,
            PhoneCallStatus::Active,
            PhoneCallStatus::Completed,
        ),
        (
            PhoneCallDirection::Inbound,
            PhoneCallStatus::Ringing,
            PhoneCallStatus::Missed,
        ),
    ] {
        let call_id = Uuid::now_v7();
        let mut repo = MockCallRepository::new();
        repo.expect_get_call_by_room_name().returning(move |room| {
            let call = phone_call(call_id, room);
            Box::pin(async move { Ok(Some(call)) })
        });
        let mut rtc = sip_webhook(
            "participant_left",
            "phone-room",
            inbound_sip(Some(CALLEE), Some(OWNER_NUMBER)),
        );
        expect_call_ends(&mut repo, &mut rtc, call_id);
        let phone_repo = FakePhoneRepo::default().with_leg(call_id, leg(direction, status));
        let service = phone_service(
            repo,
            rtc,
            RecordingConnectionService::default(),
            phone_repo.clone(),
            FakeContacts::default(),
            None,
        );
        service.process_webhook_event("body", "token").await.unwrap();
        assert_eq!(phone_repo.leg(&call_id).unwrap().status, outcome);
    }
}

#[tokio::test]
async fn unconnected_outbound_calls_are_left_to_their_dial_outcome() {
    let call_id = Uuid::now_v7();
    let mut repo = MockCallRepository::new();
    repo.expect_get_call_by_room_name().returning(move |room| {
        let call = phone_call(call_id, room);
        Box::pin(async move { Ok(Some(call)) })
    });
    repo.expect_archive_call_if_empty().never();
    let mut rtc = sip_webhook(
        "participant_left",
        "phone-room",
        inbound_sip(Some(CALLEE), None),
    );
    rtc.expect_delete_room().never();
    let phone_repo = FakePhoneRepo::default().with_leg(
        call_id,
        leg(PhoneCallDirection::Outbound, PhoneCallStatus::Dialing),
    );
    let service = phone_service(
        repo,
        rtc,
        RecordingConnectionService::default(),
        phone_repo.clone(),
        FakeContacts::default(),
        None,
    );
    service.process_webhook_event("body", "token").await.unwrap();
    assert_eq!(
        phone_repo.leg(&call_id).unwrap().status,
        PhoneCallStatus::Dialing
    );
}

#[tokio::test]
async fn phone_settings_report_dialing_and_the_caller_id() {
    let without_number = phone_service(
        MockCallRepository::new(),
        MockCallRtcClient::new(),
        RecordingConnectionService::default(),
        FakePhoneRepo::default(),
        FakeContacts::default(),
        Some(dialing()),
    )
    .get_phone_settings(owner())
    .await
    .unwrap();
    assert!(without_number.dialing_enabled);
    assert_eq!(without_number.caller_id, Some(number(DEFAULT_CALLER_ID)));
    assert!(without_number.phone_numbers.is_empty());

    let with_number = phone_service(
        MockCallRepository::new(),
        MockCallRtcClient::new(),
        RecordingConnectionService::default(),
        FakePhoneRepo::default().with_number(OWNER_NUMBER, owner()),
        FakeContacts::default(),
        None,
    )
    .get_phone_settings(owner())
    .await
    .unwrap();
    assert!(!with_number.dialing_enabled);
    assert_eq!(with_number.caller_id, Some(number(OWNER_NUMBER)));
    assert_eq!(with_number.phone_numbers, vec![number(OWNER_NUMBER)]);
}

#[tokio::test]
async fn numbers_can_be_assigned_and_released() {
    let phone_repo = FakePhoneRepo::default();
    let service = phone_service(
        MockCallRepository::new(),
        MockCallRtcClient::new(),
        RecordingConnectionService::default(),
        phone_repo.clone(),
        FakeContacts::default(),
        None,
    );
    service
        .assign_phone_number(
            number(OWNER_NUMBER),
            AssignPhoneNumberRequest { user_id: owner() },
        )
        .await
        .unwrap();
    assert_eq!(
        phone_repo
            .phone_number_owner(&number(OWNER_NUMBER))
            .await
            .unwrap()
            .map(|owner| owner.to_string()),
        Some(owner().to_string())
    );
    service
        .release_phone_number(number(OWNER_NUMBER))
        .await
        .unwrap();
    assert!(matches!(
        service.release_phone_number(number(OWNER_NUMBER)).await,
        Err(CallError::NotFound(_))
    ));
}
