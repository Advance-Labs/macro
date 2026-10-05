//! Replay-safe admission and cancellation of an approved email snapshot.

use chrono::{DateTime, Utc};
use macro_user_id::user_id::MacroUserIdStr;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::models::{CreateDraftInput, EmailErr, ResolvedDraftInput, ThreadRow, UpsertedContacts};

/// Unique identity for one explicit Send action, retained across retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendAttemptId(pub Uuid);

/// Content approved at Send. Attachment references name completed uploads only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendSnapshot {
    /// Full message state, with client handles where server IDs are unknown.
    pub message: CreateDraftInput,
    /// Uploaded draft attachment IDs.
    pub attachment_ids: Vec<Uuid>,
    /// Original attachment IDs for forwarded attachments.
    pub forwarded_attachment_ids: Vec<Uuid>,
    /// Original editor HTML, encoded like the message HTML, without the watermark.
    pub restore_body_html: Option<String>,
    /// Original editor text.
    pub restore_body_text: Option<String>,
    /// Original editor document.
    pub restore_body_macro: Option<String>,
}

/// Authoritative state of an admitted or cancelled send attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SendAttemptStatus {
    /// Committed and awaiting the delivery worker.
    Accepted,
    /// The worker owns delivery; cancellation is too late.
    Sending,
    /// Provider delivery was recorded successfully.
    Sent,
    /// Delivery authority was revoked, including before admission.
    Cancelled,
}

/// Stable admission result, independent of response hydration and SQS availability.
#[derive(Debug, Clone)]
pub struct SendAttempt {
    /// Whether this invocation changed delivery authority (for event publication).
    pub transitioned: bool,
    /// Fully hydrated current message, loaded only after authorization.
    pub message: Option<super::models::Message>,
    /// The caller's attempt identity.
    pub attempt_id: SendAttemptId,
    /// Current authoritative delivery state.
    pub status: SendAttemptStatus,
    /// Message identity, absent for cancellation before admission.
    pub message_id: Option<Uuid>,
    /// Conversation identity, absent before admission.
    pub thread_id: Option<Uuid>,
    /// Deadline assigned at first admission, never extended by replay.
    pub send_time: Option<DateTime<Utc>>,
}

/// Validated data for atomic admission. Preparation performs no delivery writes.
pub struct PreparedSend {
    /// Undo delay measured at atomic admission, after acquiring message locks.
    pub undo_delay_secs: u32,
    /// Original immutable request, for detecting mismatched retries.
    pub snapshot: SendSnapshot,
    /// Sanitized message and server-generated candidate identities.
    pub message: ResolvedDraftInput,
    /// Resolved recipients.
    pub contacts: UpsertedContacts,
    /// New conversation, if necessary.
    pub new_thread: Option<ThreadRow>,
    /// Sanitized restoration HTML.
    pub restore_html: Option<String>,
}

/// Persistence operations serialize admission/cancellation and message delivery locks.
pub trait EmailSendRepo: Send + Sync {
    /// Fetch the authorized attempt's current message for response hydration.
    fn send_message_row(
        &self,
        link_id: Uuid,
        message_id: Uuid,
    ) -> impl Future<Output = Result<Option<super::models::MessageRow>, EmailErr>> + Send;
    /// Read an attempt, optionally verifying that its approved content matches.
    fn read_send_attempt(
        &self,
        actor: &MacroUserIdStr<'_>,
        link_id: Uuid,
        attempt_id: SendAttemptId,
        snapshot: Option<&SendSnapshot>,
    ) -> impl Future<Output = Result<Option<SendAttempt>, EmailErr>> + Send;
    /// Commit the attempt, message and schedule together, or return the prior attempt.
    fn admit_send(
        &self,
        actor: &MacroUserIdStr<'_>,
        link_id: Uuid,
        attempt_id: SendAttemptId,
        prepared: PreparedSend,
    ) -> impl Future<Output = Result<SendAttempt, EmailErr>> + Send;
    /// Persist cancellation, even before admission; never cancel another attempt.
    fn cancel_send(
        &self,
        actor: &MacroUserIdStr<'_>,
        link_id: Uuid,
        attempt_id: SendAttemptId,
    ) -> impl Future<Output = Result<SendAttempt, EmailErr>> + Send;
}

/// Authenticated user-facing delivery capability.
pub trait EmailSendService: Send + Sync {
    /// Authorize the selected inbox and durably admit the approved snapshot.
    fn send_email(
        &self,
        actor: MacroUserIdStr<'static>,
        link_id: Uuid,
        attempt_id: SendAttemptId,
        snapshot: SendSnapshot,
    ) -> impl Future<Output = Result<SendAttempt, EmailErr>> + Send;
    /// Cancel this attempt or report that delivery already started.
    fn cancel_email_send(
        &self,
        actor: MacroUserIdStr<'static>,
        link_id: Uuid,
        attempt_id: SendAttemptId,
    ) -> impl Future<Output = Result<SendAttempt, EmailErr>> + Send;
    /// Read status after navigation/restart without reissuing a send.
    fn email_send_status(
        &self,
        actor: MacroUserIdStr<'static>,
        link_id: Uuid,
        attempt_id: SendAttemptId,
    ) -> impl Future<Output = Result<Option<SendAttempt>, EmailErr>> + Send;
}
