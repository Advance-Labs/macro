use super::models::{Envelope, EnvelopeState, Error, Invitation, StoredEnvelope};
use async_trait::async_trait;
use uuid::Uuid;

/// Atomic aggregate storage. Every mutation compares the loaded revision.
#[async_trait]
pub trait Repository: Send + Sync + 'static {
    /// Persist a new draft and immutable source PDF.
    async fn insert(&self, value: &StoredEnvelope) -> Result<(), Error>;
    /// Bounded sender-owned listing, newest first.
    async fn list(&self, user_id: &str) -> Result<Vec<Envelope>, Error>;
    /// Load an aggregate by ID.
    async fn load(&self, id: Uuid) -> Result<StoredEnvelope, Error>;
    /// Find the envelope containing a hashed signing capability.
    async fn by_token_hash(&self, hash: &str) -> Result<StoredEnvelope, Error>;
    /// Atomically replace state at expected revision, optionally adding completed bytes.
    async fn save(
        &self,
        state: &EnvelopeState,
        expected: i64,
        completed: Option<&[u8]>,
    ) -> Result<(), Error>;
}

/// PDF inspection and immutable completion rendering.
pub trait Documents: Send + Sync + 'static {
    /// Validate PDF structure and return usable page count.
    fn inspect(&self, bytes: &[u8]) -> Result<u32, Error>;
    /// Validate that entered text can be represented in the completed PDF.
    fn validate_text(&self, text: &str) -> Result<(), Error>;
    /// Flatten adopted values and append the evidence certificate.
    fn complete(&self, bytes: &[u8], envelope: &Envelope) -> Result<Vec<u8>, Error>;
}

/// External email delivery, separate from lifecycle policy.
#[async_trait]
pub trait Invitations: Send + Sync + 'static {
    /// Send one recipient invitation. Failure is recorded as retryable, never success.
    async fn deliver(&self, invitation: Invitation) -> Result<(), Error>;
}
