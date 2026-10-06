//! Phone calls: a party on the public phone network bridged into a call.
//!
//! A phone call is a standalone call (no channel, no meeting link) whose RTC
//! room also holds one SIP participant. Everything else about the call —
//! recording, transcription, summaries, naming, sharing, the Calls list — is
//! the ordinary call pipeline. This module holds the vocabulary for the phone
//! leg itself: who is on the other end, which way the call went, and how it
//! ended.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use chrono::{DateTime, Utc};
use macro_user_id::user_id::MacroUserIdStr;
pub use phone_number::{DialablePhoneNumber, Extension, PhoneNumber, PhoneNumberError};
use uuid::Uuid;

use super::models::{CallError, CallTokenResponse};

#[cfg(test)]
mod test;

/// Prefix LiveKit gives the identity of an inbound SIP participant. Outbound
/// legs use the same convention so every phone party looks alike.
const SIP_IDENTITY_PREFIX: &str = "sip_";
/// How long an outbound call may ring before it counts as unanswered.
pub const OUTBOUND_RINGING_TIMEOUT: Duration = Duration::from_secs(45);
/// Longest phone call Macro keeps connected. RTC access tokens live for six
/// hours, so calls end before a participant's token could expire.
pub const MAX_PHONE_CALL_DURATION: Duration = Duration::from_secs(4 * 3600);
/// Pause before keying an extension: each `w` waits half a second, giving an
/// auto attendant time to start listening after it answers.
const EXTENSION_DTMF_PAUSE: &str = "wwww";
/// NANP area codes billed at premium rates to the caller (`900`, `976`).
/// Dialing them from a shared trunk is a classic toll-fraud vector.
const PREMIUM_RATE_PREFIXES: [&str; 2] = ["1900", "1976"];

/// Which side placed a phone call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "inbound", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PhoneCallDirection {
    /// A Macro user dialed out.
    Outbound,
    /// Someone dialed a Macro user's number.
    Inbound,
}

impl PhoneCallDirection {
    /// Stable storage spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Outbound => "outbound",
            Self::Inbound => "inbound",
        }
    }
}

impl fmt::Display for PhoneCallDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PhoneCallDirection {
    type Err = CallError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "outbound" => Ok(Self::Outbound),
            "inbound" => Ok(Self::Inbound),
            other => Err(CallError::Internal(anyhow::anyhow!(
                "unknown phone call direction {other:?}"
            ))),
        }
    }
}

/// Where a phone leg is in its lifecycle. The first three states are live;
/// the rest are outcomes and never change once reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "inbound", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PhoneCallStatus {
    /// An outbound call is being placed.
    Dialing,
    /// An inbound call is ringing for its Macro user.
    Ringing,
    /// Both sides are connected.
    Active,
    /// The call was answered and has since ended.
    Completed,
    /// An inbound call ended without being answered.
    Missed,
    /// The person dialed did not pick up.
    NoAnswer,
    /// The line dialed was busy.
    Busy,
    /// The call was rejected: an inbound call by its Macro user, or an
    /// outbound call by the person dialed.
    Declined,
    /// The call could not be placed, e.g. an unreachable number.
    Failed,
    /// The Macro user hung up an outbound call before it was answered.
    Cancelled,
}

impl PhoneCallStatus {
    /// Whether the leg can still change: dialing, ringing, or active.
    pub fn is_live(self) -> bool {
        matches!(self, Self::Dialing | Self::Ringing | Self::Active)
    }

    /// The outcome of a live leg that ends without a more specific reason:
    /// answered calls complete, unanswered inbound calls are missed, and
    /// unanswered outbound calls went unanswered. Outcomes are unchanged.
    pub fn concluded(self, direction: PhoneCallDirection) -> Self {
        match (self, direction) {
            (Self::Active, _) => Self::Completed,
            (Self::Dialing | Self::Ringing, PhoneCallDirection::Inbound) => Self::Missed,
            (Self::Dialing | Self::Ringing, PhoneCallDirection::Outbound) => Self::NoAnswer,
            (outcome, _) => outcome,
        }
    }

    /// The outcome when the Macro side hangs up a live leg: answered calls
    /// complete, ringing inbound calls are declined, and outbound calls
    /// still being placed are cancelled.
    pub fn hung_up(self, direction: PhoneCallDirection) -> Self {
        match (self, direction) {
            (Self::Active, _) => Self::Completed,
            (Self::Dialing | Self::Ringing, PhoneCallDirection::Inbound) => Self::Declined,
            (Self::Dialing | Self::Ringing, PhoneCallDirection::Outbound) => Self::Cancelled,
            (outcome, _) => outcome,
        }
    }

    /// Stable storage spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dialing => "dialing",
            Self::Ringing => "ringing",
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Missed => "missed",
            Self::NoAnswer => "no_answer",
            Self::Busy => "busy",
            Self::Declined => "declined",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for PhoneCallStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PhoneCallStatus {
    type Err = CallError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value {
            "dialing" => Self::Dialing,
            "ringing" => Self::Ringing,
            "active" => Self::Active,
            "completed" => Self::Completed,
            "missed" => Self::Missed,
            "no_answer" => Self::NoAnswer,
            "busy" => Self::Busy,
            "declined" => Self::Declined,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            other => {
                return Err(CallError::Internal(anyhow::anyhow!(
                    "unknown phone call status {other:?}"
                )));
            }
        })
    }
}

/// Why an outbound call did not connect, as reported by the phone network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialFailure {
    /// The line was busy (SIP 486/600).
    Busy,
    /// Nobody picked up before the ringing timeout (SIP 408/480/487).
    NoAnswer,
    /// The person dialed rejected the call (SIP 603).
    Declined,
    /// The number does not exist or cannot be reached (SIP 404/484/604).
    Unreachable,
    /// Any other failure placing the call.
    Failed,
}

impl DialFailure {
    /// Classify a final SIP response status code.
    pub fn from_sip_status(code: u16) -> Self {
        match code {
            486 | 600 => Self::Busy,
            408 | 480 | 487 => Self::NoAnswer,
            603 => Self::Declined,
            404 | 484 | 604 => Self::Unreachable,
            _ => Self::Failed,
        }
    }

    /// The leg outcome this failure produces.
    pub fn status(self) -> PhoneCallStatus {
        match self {
            Self::Busy => PhoneCallStatus::Busy,
            Self::NoAnswer => PhoneCallStatus::NoAnswer,
            Self::Declined => PhoneCallStatus::Declined,
            Self::Unreachable | Self::Failed => PhoneCallStatus::Failed,
        }
    }
}

/// The CRM contact on the other end of a phone call, as matched when the
/// call started.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "inbound", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PhoneContact {
    /// The CRM contact id.
    pub contact_id: Uuid,
    /// The contact's name, when the CRM has one.
    pub name: Option<String>,
}

/// The phone leg of a call: the party on the phone network.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "inbound", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PhoneLeg {
    /// Which side placed the call.
    pub direction: PhoneCallDirection,
    /// The external party's number.
    pub remote_number: PhoneNumber,
    /// The Macro number used: the caller id of an outbound call or the
    /// number an inbound caller dialed. `None` when the carrier chose the
    /// caller id.
    pub local_number: Option<PhoneNumber>,
    /// RTC identity of the phone participant. Transcript segments spoken on
    /// the phone use it as their speaker id.
    pub participant_identity: String,
    /// Where the leg is in its lifecycle, or how it ended.
    pub status: PhoneCallStatus,
    /// The CRM contact matched to the external number, if any.
    pub contact: Option<PhoneContact>,
    /// When the call was answered.
    pub answered_at: Option<DateTime<Utc>>,
    /// When the phone leg ended.
    pub ended_at: Option<DateTime<Utc>>,
}

impl PhoneLeg {
    /// How the external party is referred to in transcripts and summaries:
    /// the contact's name when known, otherwise their number.
    pub fn remote_party_label(&self) -> String {
        match self.contact.as_ref().and_then(|contact| contact.name.as_deref()) {
            Some(name) => name.to_string(),
            None => self.remote_number.display(),
        }
    }
}

/// RTC identity for the SIP participant of an outbound call to `number`.
pub fn outbound_participant_identity(number: &PhoneNumber) -> String {
    format!("{SIP_IDENTITY_PREFIX}{number}")
}

/// DTMF digits that reach `extension` once the callee's switchboard answers.
pub fn extension_dtmf(extension: &Extension) -> String {
    format!("{EXTENSION_DTMF_PAUSE}{}", extension.as_str())
}

/// LiveKit's report of a SIP call's progress (`sip.callStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SipCallStatus {
    /// An outbound call is waiting to be picked up.
    Dialing,
    /// An inbound call is ringing for the caller.
    Ringing,
    /// An outbound call connected and is still keying DTMF digits.
    Automation,
    /// The call is connected.
    Active,
    /// The call has been hung up.
    Hangup,
}

impl FromStr for SipCallStatus {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "dialing" => Ok(Self::Dialing),
            "ringing" => Ok(Self::Ringing),
            "automation" => Ok(Self::Automation),
            "active" => Ok(Self::Active),
            "hangup" => Ok(Self::Hangup),
            _ => Err(()),
        }
    }
}

/// A SIP participant described by an RTC webhook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SipParticipant {
    /// The participant's RTC identity.
    pub identity: String,
    /// The external party's number (`sip.phoneNumber`).
    pub phone_number: Option<PhoneNumber>,
    /// The Macro number on the trunk side (`sip.trunkPhoneNumber`): the
    /// number an inbound caller dialed.
    pub trunk_phone_number: Option<PhoneNumber>,
    /// Call progress reported by the SIP stack (`sip.callStatus`).
    pub call_status: Option<SipCallStatus>,
    /// The SIP stack's call id (`sip.callID`).
    pub sip_call_id: Option<String>,
    /// Whether a dispatch rule placed the participant (`sip.ruleID`), which
    /// only happens for inbound calls.
    pub is_inbound: bool,
}

/// Deployment settings for placing phone calls.
#[derive(Debug, Clone)]
pub struct PhoneDialingConfig {
    /// The outbound SIP trunk calls are placed through.
    pub outbound_trunk_id: String,
    /// Caller id for users who have no number of their own. `None` lets the
    /// trunk choose.
    pub default_caller_id: Option<PhoneNumber>,
    /// Country calling codes that may be dialed, e.g. `["1"]` for North
    /// America. Every destination must start with one of them.
    pub allowed_country_codes: Vec<String>,
}

impl PhoneDialingConfig {
    /// Whether `number` may be dialed from this deployment. Premium-rate
    /// numbers are never dialed, whatever the allowed countries.
    pub fn permits(&self, number: &PhoneNumber) -> Result<(), CallError> {
        if PREMIUM_RATE_PREFIXES
            .iter()
            .any(|prefix| number.starts_with_digits(prefix))
        {
            return Err(CallError::InvalidRequest(
                "Premium-rate numbers can't be called".to_string(),
            ));
        }
        if !self
            .allowed_country_codes
            .iter()
            .any(|code| number.starts_with_digits(code))
        {
            return Err(CallError::InvalidRequest(
                "Calls to this country aren't enabled for your workspace".to_string(),
            ));
        }
        Ok(())
    }
}

/// Request body for placing a phone call.
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(feature = "inbound", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DialPhoneRequest {
    /// The number to call, as typed: E.164, a formatted national number
    /// (North American numbers may omit `+1`), or a `tel:` URI, optionally
    /// with an extension (`ext. 89`).
    pub to: String,
}

/// Credentials to join a phone call's room, plus its phone leg.
#[derive(Debug, serde::Serialize)]
#[cfg_attr(feature = "inbound", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PhoneCallJoinResponse {
    /// RTC credentials, shaped like every other call join.
    pub call: CallTokenResponse,
    /// The phone leg.
    pub phone: PhoneLeg,
}

/// What the caller can do with phone calling.
#[derive(Debug, serde::Serialize)]
#[cfg_attr(feature = "inbound", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PhoneSettingsResponse {
    /// Whether this deployment can place outbound calls.
    pub dialing_enabled: bool,
    /// The number people see when the caller dials out, when known.
    pub caller_id: Option<PhoneNumber>,
    /// Numbers that ring the caller.
    pub phone_numbers: Vec<PhoneNumber>,
}

/// An inbound phone call ringing for a Macro user.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "inbound", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct IncomingPhoneCall {
    /// The call to answer or decline.
    pub call_id: Uuid,
    /// Who is calling.
    pub from: PhoneNumber,
    /// The Macro number they dialed.
    pub to: Option<PhoneNumber>,
    /// The CRM contact matched to the caller, if any.
    pub contact: Option<PhoneContact>,
    /// When the call started ringing.
    pub started_at: DateTime<Utc>,
}

/// Inbound phone calls ringing for the caller, newest first.
#[derive(Debug, serde::Serialize)]
#[cfg_attr(feature = "inbound", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct IncomingPhoneCallsResponse {
    /// The ringing calls.
    pub calls: Vec<IncomingPhoneCall>,
}

/// Request body for assigning a phone number to a user.
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(feature = "inbound", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct AssignPhoneNumberRequest {
    /// The Macro user the number rings and identifies.
    #[cfg_attr(feature = "inbound", schema(value_type = String))]
    pub user_id: MacroUserIdStr<'static>,
}

/// A phone leg as created with its call.
#[derive(Debug, Clone)]
pub struct NewPhoneLeg {
    /// Which side placed the call.
    pub direction: PhoneCallDirection,
    /// The external party's number.
    pub remote_number: PhoneNumber,
    /// The Macro number used, when known.
    pub local_number: Option<PhoneNumber>,
    /// RTC identity of the phone participant.
    pub participant_identity: String,
    /// The initial status: dialing (outbound) or ringing (inbound).
    pub status: PhoneCallStatus,
    /// The CRM contact matched to the external number, if any.
    pub contact: Option<PhoneContact>,
    /// The SIP stack's call id, when already known.
    pub sip_call_id: Option<String>,
}

/// A new phone call and the room it lives in.
#[derive(Debug, Clone)]
pub struct NewPhoneCall {
    /// Id of the call, its record, and (for outbound calls) its room.
    pub call_id: Uuid,
    /// The RTC room holding the call.
    pub room_name: String,
    /// The Macro user who owns the call: the caller of an outbound call or
    /// the owner of the number an inbound caller dialed.
    pub owner: MacroUserIdStr<'static>,
    /// The phone leg.
    pub leg: NewPhoneLeg,
}

/// A change to a live phone leg. Outcomes are final: repositories ignore
/// updates to a leg that has already ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhoneLegUpdate {
    /// The new status.
    pub status: PhoneCallStatus,
    /// When the call was answered, if this update records it.
    pub answered_at: Option<DateTime<Utc>>,
    /// When the leg ended, if this update ends it.
    pub ended_at: Option<DateTime<Utc>>,
    /// The SIP stack's call id, if newly learned.
    pub sip_call_id: Option<String>,
}

impl PhoneLegUpdate {
    /// Mark the leg answered at `at`.
    pub fn answered(at: DateTime<Utc>, sip_call_id: Option<String>) -> Self {
        Self {
            status: PhoneCallStatus::Active,
            answered_at: Some(at),
            ended_at: None,
            sip_call_id,
        }
    }

    /// End the leg at `at` with `outcome`.
    pub fn ended(outcome: PhoneCallStatus, at: DateTime<Utc>) -> Self {
        Self {
            status: outcome,
            answered_at: None,
            ended_at: Some(at),
            sip_call_id: None,
        }
    }
}

/// A request to place the SIP leg of an outbound call.
#[derive(Debug, Clone)]
pub struct SipDialRequest {
    /// The room the phone participant joins.
    pub room_name: String,
    /// The outbound trunk to dial through.
    pub trunk_id: String,
    /// The number to call.
    pub to: PhoneNumber,
    /// Caller id to present, or `None` to let the trunk choose.
    pub caller_id: Option<PhoneNumber>,
    /// RTC identity for the phone participant.
    pub participant_identity: String,
    /// Display name for the phone participant.
    pub participant_name: String,
    /// DTMF to key once the call connects, e.g. an extension.
    pub dtmf: Option<String>,
    /// How long to ring before giving up.
    pub ringing_timeout: Duration,
    /// Longest the call may last.
    pub max_call_duration: Duration,
}

/// An outbound SIP leg the callee answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SipDialAnswered {
    /// The SIP stack's call id, when reported.
    pub sip_call_id: Option<String>,
}
