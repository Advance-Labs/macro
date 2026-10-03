use super::{
    models::*,
    ports::{Documents, Invitations, Repository},
};
use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use uuid::Uuid;

#[cfg(test)]
mod test;

/// Envelope use cases. Authorization and transitions live here, never in adapters.
pub struct Service<R, D, N> {
    repo: R,
    documents: D,
    invitations: N,
}

/// SHA-256 fingerprint for document and capability evidence.
pub fn fingerprint(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn conflict(message: &str) -> Error {
    Error::Conflict(message.into())
}
fn audit(envelope: &mut Envelope, action: AuditAction, actor: String, detail: String) {
    envelope.audit.push(AuditEvent {
        at: Utc::now(),
        action,
        actor,
        detail,
    });
}
fn ready(envelope: &Envelope, recipient: &Recipient) -> bool {
    envelope
        .recipients
        .iter()
        .all(|other| other.order >= recipient.order || other.signed_at.is_some())
}
fn validate_revision(envelope: &Envelope, expected: i64) -> Result<(), Error> {
    if envelope.revision != expected {
        return Err(conflict("This envelope changed. Refresh and try again."));
    }
    Ok(())
}
fn active(envelope: &Envelope) -> Result<(), Error> {
    if matches!(envelope.status, Status::Voided | Status::Declined)
        || envelope
            .expires_at
            .is_some_and(|expires| expires <= Utc::now())
    {
        return Err(Error::Gone);
    }
    Ok(())
}

impl<R: Repository, D: Documents, N: Invitations> Service<R, D, N> {
    /// Compose replaceable capabilities.
    pub fn new(repo: R, documents: D, invitations: N) -> Self {
        Self {
            repo,
            documents,
            invitations,
        }
    }

    /// Create an account-private draft with an immutable PDF snapshot.
    pub async fn create(
        &self,
        user_id: &str,
        title: String,
        filename: String,
        source: Vec<u8>,
    ) -> Result<Envelope, Error> {
        if title.trim().is_empty()
            || title.len() > 200
            || filename.is_empty()
            || filename.len() > 255
            || source.len() > 10 * 1024 * 1024
        {
            return Err(invalid("Provide a title and a PDF up to 10 MB."));
        }
        self.documents.validate_text(&title)?;
        self.documents.validate_text(&filename)?;
        let page_count = self.documents.inspect(&source)?;
        let mut envelope = Envelope {
            id: Uuid::now_v7(),
            title: title.trim().into(),
            message: String::new(),
            filename,
            page_count,
            source_sha256: fingerprint(&source),
            completed_sha256: None,
            status: Status::Draft,
            revision: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: None,
            recipients: vec![],
            fields: vec![],
            audit: vec![],
        };
        let source_hash = envelope.source_sha256.clone();
        audit(
            &mut envelope,
            AuditAction::Created,
            user_id.into(),
            source_hash,
        );
        let stored = StoredEnvelope {
            state: EnvelopeState {
                user_id: user_id.into(),
                envelope: envelope.clone(),
                grants: vec![],
            },
            source,
            completed: None,
        };
        self.repo.insert(&stored).await?;
        Ok(envelope)
    }

    /// List only the authenticated account's envelopes.
    pub async fn list(&self, user_id: &str) -> Result<Vec<Envelope>, Error> {
        self.repo.list(user_id).await
    }

    async fn owned(&self, user_id: &str, id: Uuid) -> Result<StoredEnvelope, Error> {
        let stored = self.repo.load(id).await?;
        if stored.state.user_id != user_id {
            return Err(Error::NotFound);
        }
        Ok(stored)
    }

    /// Read a sender-owned envelope.
    pub async fn get(&self, user_id: &str, id: Uuid) -> Result<Envelope, Error> {
        Ok(self.owned(user_id, id).await?.state.envelope)
    }

    /// Fetch a sender-owned source or completed PDF.
    pub async fn document(
        &self,
        user_id: &str,
        id: Uuid,
        completed: bool,
    ) -> Result<Vec<u8>, Error> {
        let stored = self.owned(user_id, id).await?;
        if completed {
            stored
                .completed
                .ok_or_else(|| conflict("The envelope is not complete yet."))
        } else {
            Ok(stored.source)
        }
    }

    /// Replace draft recipients and fields; reject injected timestamps and values.
    pub async fn update(&self, user_id: &str, id: Uuid, draft: Draft) -> Result<Envelope, Error> {
        let mut stored = self.owned(user_id, id).await?;
        let envelope = &mut stored.state.envelope;
        validate_revision(envelope, draft.revision)?;
        if envelope.status != Status::Draft {
            return Err(conflict(
                "Sent envelopes are frozen. Create a new envelope to change the document or recipients.",
            ));
        }
        if draft.title.trim().is_empty()
            || draft.title.len() > 200
            || draft.message.len() > 5000
            || draft.recipients.len() > 20
            || draft.fields.len() > 200
        {
            return Err(invalid(
                "Check the title, message, recipient count, and fields.",
            ));
        }
        self.documents.validate_text(&draft.title)?;
        self.documents.validate_text(&draft.message)?;
        let mut ids = HashSet::new();
        let mut emails = HashSet::new();
        for recipient in &draft.recipients {
            self.documents.validate_text(&recipient.name)?;
            if recipient.id.is_nil()
                || !ids.insert(recipient.id)
                || recipient.name.trim().is_empty()
                || recipient.name.len() > 200
                || recipient.email.len() > 254
                || !email_validator::is_valid_email(&recipient.email)
                || !emails.insert(recipient.email.to_lowercase())
                || recipient.order == 0
                || recipient.order > 20
            {
                return Err(invalid(
                    "Each recipient needs a unique email, name, and signing order from 1 to 20.",
                ));
            }
        }
        let mut field_ids = HashSet::new();
        for field in &draft.fields {
            if field.id.is_nil()
                || !field_ids.insert(field.id)
                || !ids.contains(&field.recipient_id)
                || field.page == 0
                || field.page > envelope.page_count
                || ![field.x, field.y, field.width, field.height]
                    .iter()
                    .all(|n| n.is_finite())
                || field.x < 0.0
                || field.y < 0.0
                || field.width < 0.05
                || field.height < 0.02
                || field.x + field.width > 1.0
                || field.y + field.height > 1.0
            {
                return Err(invalid(
                    "Fields must fit on a document page and belong to a recipient.",
                ));
            }
        }
        envelope.title = draft.title.trim().into();
        envelope.message = draft.message;
        envelope.recipients = draft
            .recipients
            .into_iter()
            .map(|mut r| {
                r.email = r.email.trim().to_lowercase();
                r.signed_at = None;
                r.delivered_at = None;
                r
            })
            .collect();
        envelope.fields = draft
            .fields
            .into_iter()
            .map(|mut f| {
                f.value = None;
                f
            })
            .collect();
        envelope.revision += 1;
        envelope.updated_at = Utc::now();
        audit(
            envelope,
            AuditAction::Updated,
            user_id.into(),
            "Draft updated".into(),
        );
        self.repo.save(&stored.state, draft.revision, None).await?;
        Ok(stored.state.envelope)
    }

    /// Freeze a validated draft and invite the first recipient group.
    pub async fn send(&self, user_id: &str, id: Uuid, revision: i64) -> Result<Envelope, Error> {
        let mut stored = self.owned(user_id, id).await?;
        let envelope = &mut stored.state.envelope;
        validate_revision(envelope, revision)?;
        if envelope.status != Status::Draft {
            return Err(conflict("Only drafts can be sent."));
        }
        if envelope.recipients.is_empty()
            || envelope.recipients.iter().any(|r| {
                !envelope
                    .fields
                    .iter()
                    .any(|f| f.recipient_id == r.id && f.kind == FieldKind::Signature && f.required)
            })
        {
            return Err(invalid(
                "Add at least one recipient and a required signature field for every signer.",
            ));
        }
        envelope.status = Status::Sent;
        envelope.expires_at = Some(Utc::now() + Duration::days(30));
        envelope.revision += 1;
        envelope.updated_at = Utc::now();
        let hash = envelope.source_sha256.clone();
        audit(
            envelope,
            AuditAction::Sent,
            user_id.into(),
            format!("Source SHA-256: {hash}; links expire in 30 days"),
        );
        self.repo.save(&stored.state, revision, None).await?;
        // The send transition is durable; a delivery interruption stays visible and retryable.
        let _ = self.invite_ready(id, false).await;
        self.get(user_id, id).await
    }

    /// Retry invitations, rotating only active unsigned recipient capabilities.
    pub async fn resend(&self, user_id: &str, id: Uuid) -> Result<Envelope, Error> {
        let stored = self.owned(user_id, id).await?;
        active(&stored.state.envelope)?;
        if stored.state.envelope.status != Status::Sent {
            return Err(conflict("Only pending envelopes can be resent."));
        }
        self.invite_ready(id, true).await?;
        self.get(user_id, id).await
    }

    async fn invite_ready(&self, id: Uuid, resend: bool) -> Result<(), Error> {
        let mut stored = self.repo.load(id).await?;
        if stored.state.envelope.status != Status::Sent {
            return Ok(());
        }
        active(&stored.state.envelope)?;
        let recipients: Vec<_> = stored
            .state
            .envelope
            .recipients
            .iter()
            .filter(|r| {
                r.signed_at.is_none()
                    && ready(&stored.state.envelope, r)
                    && (resend
                        || !stored
                            .state
                            .grants
                            .iter()
                            .any(|grant| grant.recipient_id == r.id))
            })
            .cloned()
            .collect();
        if recipients.is_empty() {
            return Ok(());
        }
        let mut deliveries = vec![];
        for recipient in recipients {
            let random: [u8; 32] = rand::random();
            let token = random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            stored
                .state
                .grants
                .retain(|g| g.recipient_id != recipient.id);
            stored.state.grants.push(Grant {
                recipient_id: recipient.id,
                token_hash: fingerprint(token.as_bytes()),
            });
            if let Some(r) = stored
                .state
                .envelope
                .recipients
                .iter_mut()
                .find(|r| r.id == recipient.id)
            {
                r.delivered_at = None;
            }
            deliveries.push((
                recipient.id,
                Invitation {
                    title: stored.state.envelope.title.clone(),
                    message: stored.state.envelope.message.clone(),
                    name: recipient.name,
                    email: recipient.email,
                    token,
                },
            ));
        }
        let revision = stored.state.envelope.revision;
        stored.state.envelope.revision += 1;
        self.repo.save(&stored.state, revision, None).await?;
        for (recipient_id, invitation) in deliveries {
            // A failed delivery remains visibly pending and is recoverable with Resend.
            if self.invitations.deliver(invitation).await.is_err() {
                continue;
            }
            let mut latest = self.repo.load(id).await?;
            if latest.state.envelope.status != Status::Sent {
                continue;
            }
            let revision = latest.state.envelope.revision;
            if let Some(r) = latest
                .state
                .envelope
                .recipients
                .iter_mut()
                .find(|r| r.id == recipient_id)
            {
                r.delivered_at = Some(Utc::now());
            }
            audit(
                &mut latest.state.envelope,
                AuditAction::Delivered,
                recipient_id.to_string(),
                "Signing invitation delivered".into(),
            );
            latest.state.envelope.revision += 1;
            self.repo.save(&latest.state, revision, None).await?;
        }
        Ok(())
    }

    /// Revoke all signing capabilities without deleting the evidence.
    pub async fn void(
        &self,
        user_id: &str,
        id: Uuid,
        revision: i64,
        reason: String,
    ) -> Result<Envelope, Error> {
        let mut stored = self.owned(user_id, id).await?;
        let envelope = &mut stored.state.envelope;
        validate_revision(envelope, revision)?;
        if !matches!(envelope.status, Status::Draft | Status::Sent) {
            return Err(conflict("This envelope is already closed."));
        }
        if reason.trim().is_empty() || reason.len() > 1000 {
            return Err(invalid("Provide a reason up to 1000 characters."));
        }
        envelope.status = Status::Voided;
        envelope.revision += 1;
        envelope.updated_at = Utc::now();
        audit(envelope, AuditAction::Voided, user_id.into(), reason);
        self.repo.save(&stored.state, revision, None).await?;
        Ok(stored.state.envelope)
    }

    async fn authorized(&self, token: &str) -> Result<(StoredEnvelope, Uuid), Error> {
        if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::NotFound);
        }
        let hash = fingerprint(token.as_bytes());
        let stored = self.repo.by_token_hash(&hash).await?;
        active(&stored.state.envelope)?;
        let recipient_id = stored
            .state
            .grants
            .iter()
            .find(|g| g.token_hash == hash)
            .ok_or(Error::NotFound)?
            .recipient_id;
        Ok((stored, recipient_id))
    }

    /// Open a signing session with only this recipient's fields and identity.
    pub async fn session(&self, token: &str) -> Result<SigningSession, Error> {
        let (stored, recipient_id) = self.authorized(token).await?;
        let envelope = stored.state.envelope;
        let recipient = envelope
            .recipients
            .iter()
            .find(|r| r.id == recipient_id)
            .ok_or(Error::NotFound)?
            .clone();
        let can_sign = envelope.status == Status::Sent
            && recipient.signed_at.is_none()
            && ready(&envelope, &recipient);
        Ok(SigningSession {
            id: envelope.id,
            title: envelope.title,
            message: envelope.message,
            filename: envelope.filename,
            source_sha256: envelope.source_sha256,
            page_count: envelope.page_count,
            status: envelope.status,
            revision: envelope.revision,
            expires_at: envelope.expires_at,
            recipient,
            can_sign,
            fields: envelope
                .fields
                .into_iter()
                .filter(|f| f.recipient_id == recipient_id)
                .collect(),
        })
    }

    /// Recipient document read; a completed capability can download the final PDF.
    pub async fn signing_document(&self, token: &str, completed: bool) -> Result<Vec<u8>, Error> {
        let (stored, _) = self.authorized(token).await?;
        if completed {
            stored
                .completed
                .ok_or_else(|| conflict("This envelope is not complete yet."))
        } else {
            Ok(stored.source)
        }
    }

    /// Apply recipient values only after consent, ownership, routing, and replay checks.
    pub async fn sign(
        &self,
        token: &str,
        submission: SignatureSubmission,
        user_agent: String,
    ) -> Result<SigningSession, Error> {
        let (mut stored, recipient_id) = self.authorized(token).await?;
        let envelope = &mut stored.state.envelope;
        validate_revision(envelope, submission.revision)?;
        let recipient = envelope
            .recipients
            .iter()
            .find(|r| r.id == recipient_id)
            .ok_or(Error::NotFound)?
            .clone();
        if envelope.status != Status::Sent || recipient.signed_at.is_some() {
            return Err(conflict(
                "This recipient has already signed or the envelope is closed.",
            ));
        }
        if !ready(envelope, &recipient) {
            return Err(conflict("Waiting for an earlier recipient to sign."));
        }
        if !submission.consent {
            return Err(invalid(
                "Consent to electronic records and signatures is required.",
            ));
        }
        let mut seen = HashSet::new();
        for value in &submission.values {
            self.documents.validate_text(&value.value)?;
            if !seen.insert(value.field_id)
                || value.value.len() > 500
                || value.value.chars().any(|c| c.is_control())
            {
                return Err(invalid("Check your field values."));
            }
            let field = envelope
                .fields
                .iter_mut()
                .find(|f| f.id == value.field_id && f.recipient_id == recipient_id)
                .ok_or_else(|| invalid("You can only fill your assigned fields."))?;
            field.value = Some(if field.kind == FieldKind::Date {
                Utc::now().format("%Y-%m-%d").to_string()
            } else {
                value.value.trim().into()
            });
        }
        for field in envelope
            .fields
            .iter_mut()
            .filter(|f| f.recipient_id == recipient_id)
        {
            if field.kind == FieldKind::Date {
                field.value = Some(Utc::now().format("%Y-%m-%d").to_string());
            }
            if field.required && field.value.as_ref().is_none_or(|v| v.is_empty()) {
                return Err(invalid("Complete all required fields before signing."));
            }
        }
        let signed_at = Utc::now();
        envelope
            .recipients
            .iter_mut()
            .find(|r| r.id == recipient_id)
            .ok_or(Error::NotFound)?
            .signed_at = Some(signed_at);
        audit(
            envelope,
            AuditAction::Signed,
            format!("{} <{}>", recipient.name, recipient.email),
            format!(
                "Consented to electronic records and signatures; source SHA-256: {}; user agent: {}",
                envelope.source_sha256,
                user_agent.chars().take(500).collect::<String>()
            ),
        );
        envelope.revision += 1;
        envelope.updated_at = signed_at;
        let completed = if envelope.recipients.iter().all(|r| r.signed_at.is_some()) {
            envelope.status = Status::Completed;
            audit(
                envelope,
                AuditAction::Completed,
                "Macro Legal".into(),
                "All recipients signed; completed PDF includes certificate of completion".into(),
            );
            let bytes = self.documents.complete(&stored.source, envelope)?;
            envelope.completed_sha256 = Some(fingerprint(&bytes));
            Some(bytes)
        } else {
            None
        };
        self.repo
            .save(&stored.state, submission.revision, completed.as_deref())
            .await?;
        // Signing is committed even when subsequent mail delivery needs retrying.
        let _ = self.invite_ready(stored.state.envelope.id, false).await;
        self.session(token).await
    }

    /// Decline an active invitation, revoking the envelope for every recipient.
    pub async fn decline(&self, token: &str, revision: i64, reason: String) -> Result<(), Error> {
        let (mut stored, recipient_id) = self.authorized(token).await?;
        let envelope = &mut stored.state.envelope;
        validate_revision(envelope, revision)?;
        let recipient = envelope
            .recipients
            .iter()
            .find(|r| r.id == recipient_id)
            .ok_or(Error::NotFound)?;
        if envelope.status != Status::Sent
            || recipient.signed_at.is_some()
            || !ready(envelope, recipient)
        {
            return Err(conflict("This invitation cannot be declined."));
        }
        if reason.trim().is_empty() || reason.len() > 1000 {
            return Err(invalid("Provide a decline reason up to 1000 characters."));
        }
        let actor = format!("{} <{}>", recipient.name, recipient.email);
        envelope.status = Status::Declined;
        envelope.revision += 1;
        envelope.updated_at = Utc::now();
        audit(envelope, AuditAction::Declined, actor, reason);
        self.repo.save(&stored.state, revision, None).await
    }
}
