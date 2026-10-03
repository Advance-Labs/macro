use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Envelope lifecycle; terminal envelopes cannot be edited or signed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Editable and not sent.
    Draft,
    /// Awaiting recipients.
    Sent,
    /// All recipients signed.
    Completed,
    /// A recipient declined.
    Declined,
    /// Sender revoked the request.
    Voided,
}

/// Supported signing field types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    /// Adopted typed signature.
    Signature,
    /// Adopted typed initials.
    Initials,
    /// Server-recorded signing date.
    Date,
    /// Recipient-entered text.
    Text,
}

/// One recipient in an envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recipient {
    /// Stable recipient ID.
    pub id: Uuid,
    /// Display name.
    pub name: String,
    /// Invitation destination.
    pub email: String,
    /// Signing order; equal values sign in parallel.
    pub order: u32,
    /// Signing timestamp, if complete.
    pub signed_at: Option<DateTime<Utc>>,
    /// Latest successful invitation delivery.
    pub delivered_at: Option<DateTime<Utc>>,
}

/// Field positioned relative to a PDF page, with normalized top-left coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    /// Stable field ID.
    pub id: Uuid,
    /// Assigned recipient.
    pub recipient_id: Uuid,
    /// Field type.
    pub kind: FieldKind,
    /// One-based page number.
    pub page: u32,
    /// Left position, normalized to page width.
    pub x: f64,
    /// Top position, normalized to page height.
    pub y: f64,
    /// Width, normalized to page width.
    pub width: f64,
    /// Height, normalized to page height.
    pub height: f64,
    /// Whether signing requires a value.
    pub required: bool,
    /// Frozen recipient value.
    pub value: Option<String>,
}

/// Audit evidence recorded by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    /// Event time.
    pub at: DateTime<Utc>,
    /// Event category.
    pub action: AuditAction,
    /// Sender or recipient display identity.
    pub actor: String,
    /// Additional evidence, e.g. source fingerprint or decline reason.
    pub detail: String,
}

/// Closed set of audit categories.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    /// Sender created the draft.
    Created,
    /// Sender changed the draft.
    Updated,
    /// Sender froze and sent the envelope.
    Sent,
    /// Invitation delivered.
    Delivered,
    /// Recipient consented and signed.
    Signed,
    /// Envelope completed.
    Completed,
    /// Recipient declined.
    Declined,
    /// Sender voided the envelope.
    Voided,
}

/// Public sender-facing envelope metadata. No recipient capabilities are returned.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    /// Envelope ID.
    pub id: Uuid,
    /// Subject/title.
    pub title: String,
    /// Note to recipients.
    pub message: String,
    /// Uploaded filename.
    pub filename: String,
    /// Verified page count.
    pub page_count: u32,
    /// SHA-256 of the immutable source PDF.
    pub source_sha256: String,
    /// SHA-256 of the completed PDF.
    pub completed_sha256: Option<String>,
    /// Lifecycle state.
    pub status: Status,
    /// Revision for concurrency control.
    pub revision: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Most recent change time.
    pub updated_at: DateTime<Utc>,
    /// Signing expiration, set when sent.
    pub expires_at: Option<DateTime<Utc>>,
    /// Signers.
    pub recipients: Vec<Recipient>,
    /// Positioned fields.
    pub fields: Vec<Field>,
    /// Append-only lifecycle evidence.
    pub audit: Vec<AuditEvent>,
}

/// Secret-free persisted recipient authorization grant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grant {
    /// Recipient authorized by this capability.
    pub recipient_id: Uuid,
    /// SHA-256 of the random capability; plaintext is never stored.
    pub token_hash: String,
}

/// Persisted aggregate; the owning account is independent of recipient identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeState {
    /// Sender's authenticated account ID.
    pub user_id: String,
    /// Public metadata.
    pub envelope: Envelope,
    /// Recipient capability hashes.
    pub grants: Vec<Grant>,
}

/// Loaded immutable source and optional completed file.
#[derive(Clone)]
pub struct StoredEnvelope {
    /// Envelope state.
    pub state: EnvelopeState,
    /// Immutable source PDF bytes.
    pub source: Vec<u8>,
    /// Flattened completed PDF with certificate.
    pub completed: Option<Vec<u8>>,
}

/// Draft replacement command; values and timestamps are always server-owned.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    /// Subject.
    pub title: String,
    /// Recipient message.
    pub message: String,
    /// Expected aggregate version.
    pub revision: i64,
    /// Recipient definitions.
    pub recipients: Vec<Recipient>,
    /// Fields to place.
    pub fields: Vec<Field>,
}

/// Recipient field input.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldValue {
    /// Field being filled.
    pub field_id: Uuid,
    /// Adopted or entered value.
    pub value: String,
}

/// Explicit consent and signature submission.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureSubmission {
    /// Expected envelope version.
    pub revision: i64,
    /// Explicit agreement to electronic records/signatures.
    pub consent: bool,
    /// Assigned field values.
    pub values: Vec<FieldValue>,
}

/// Minimal signing session returned to the authorized recipient.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SigningSession {
    /// Envelope identity.
    pub id: Uuid,
    /// Subject.
    pub title: String,
    /// Sender's note.
    pub message: String,
    /// Source filename.
    pub filename: String,
    /// Source fingerprint.
    pub source_sha256: String,
    /// PDF pages.
    pub page_count: u32,
    /// Current lifecycle.
    pub status: Status,
    /// Concurrency version.
    pub revision: i64,
    /// Expiry.
    pub expires_at: Option<DateTime<Utc>>,
    /// Authorized signer only.
    pub recipient: Recipient,
    /// Whether preceding recipients are complete.
    pub can_sign: bool,
    /// Fields assigned to this signer only.
    pub fields: Vec<Field>,
}

/// Invitation delivery payload; never log or persist the plaintext token.
#[derive(Clone)]
pub struct Invitation {
    /// Envelope subject.
    pub title: String,
    /// Message.
    pub message: String,
    /// Recipient name.
    pub name: String,
    /// Recipient email.
    pub email: String,
    /// Random one-recipient capability.
    pub token: String,
}

/// Domain errors, translated at the HTTP boundary.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Invalid command.
    #[error("{0}")]
    Invalid(String),
    /// Unknown envelope, inaccessible account, or invalid capability.
    #[error("Envelope or signing link not found")]
    NotFound,
    /// A stale command or invalid lifecycle transition.
    #[error("{0}")]
    Conflict(String),
    /// Capability is expired or revoked.
    #[error("This signing request is no longer available")]
    Gone,
    /// Storage failure.
    #[error("Envelope storage failed")]
    Storage(String),
    /// PDF could not be processed.
    #[error("{0}")]
    Pdf(String),
    /// Outbound delivery failure; signing remains retryable.
    #[error("Invitation delivery failed. Use Resend to try again.")]
    Delivery,
}
