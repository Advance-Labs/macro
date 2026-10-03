use super::*;
use async_trait::async_trait;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
#[derive(Clone, Default)]
struct Memory(Arc<Mutex<HashMap<Uuid, StoredEnvelope>>>);
#[async_trait]
impl Repository for Memory {
    async fn insert(&self, value: &StoredEnvelope) -> Result<(), Error> {
        self.0
            .lock()
            .unwrap()
            .insert(value.state.envelope.id, value.clone());
        Ok(())
    }
    async fn list(&self, user: &str) -> Result<Vec<Envelope>, Error> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .values()
            .filter(|e| e.state.user_id == user)
            .map(|e| e.state.envelope.clone())
            .collect())
    }
    async fn load(&self, id: Uuid) -> Result<StoredEnvelope, Error> {
        self.0
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or(Error::NotFound)
    }
    async fn by_token_hash(&self, hash: &str) -> Result<StoredEnvelope, Error> {
        self.0
            .lock()
            .unwrap()
            .values()
            .find(|e| e.state.grants.iter().any(|g| g.token_hash == hash))
            .cloned()
            .ok_or(Error::NotFound)
    }
    async fn save(
        &self,
        state: &EnvelopeState,
        expected: i64,
        completed: Option<&[u8]>,
    ) -> Result<(), Error> {
        let mut rows = self.0.lock().unwrap();
        let row = rows.get_mut(&state.envelope.id).ok_or(Error::NotFound)?;
        if row.state.envelope.revision != expected {
            return Err(conflict("Concurrent mutation"));
        }
        row.state = state.clone();
        if let Some(bytes) = completed {
            row.completed = Some(bytes.into());
        }
        Ok(())
    }
}
struct FakePdf;
impl Documents for FakePdf {
    fn validate_text(&self, _: &str) -> Result<(), Error> {
        Ok(())
    }
    fn inspect(&self, bytes: &[u8]) -> Result<u32, Error> {
        if bytes.starts_with(b"%PDF") {
            Ok(2)
        } else {
            Err(invalid("Not a PDF"))
        }
    }
    fn complete(&self, bytes: &[u8], _: &Envelope) -> Result<Vec<u8>, Error> {
        let mut out = bytes.to_vec();
        out.extend_from_slice(b"signed with certificate");
        Ok(out)
    }
}
#[derive(Clone, Default)]
struct Mail(Arc<Mutex<Vec<Invitation>>>, Arc<Mutex<bool>>);
#[async_trait]
impl Invitations for Mail {
    async fn deliver(&self, invitation: Invitation) -> Result<(), Error> {
        if *self.1.lock().unwrap() {
            return Err(Error::Delivery);
        }
        self.0.lock().unwrap().push(invitation);
        Ok(())
    }
}
type TestService = Service<Memory, FakePdf, Mail>;
async fn setup(orders: &[u32]) -> (TestService, Memory, Mail, Envelope) {
    let repo = Memory::default();
    let mail = Mail::default();
    let service = Service::new(repo.clone(), FakePdf, mail.clone());
    let envelope = service
        .create(
            "owner",
            "NDA".into(),
            "NDA.pdf".into(),
            b"%PDF source".to_vec(),
        )
        .await
        .unwrap();
    let recipients: Vec<_> = orders
        .iter()
        .enumerate()
        .map(|(i, order)| Recipient {
            id: Uuid::now_v7(),
            name: format!("Signer {i}"),
            email: format!("signer{i}@example.com"),
            order: *order,
            signed_at: None,
            delivered_at: None,
        })
        .collect();
    let fields = recipients
        .iter()
        .map(|r| Field {
            id: Uuid::now_v7(),
            recipient_id: r.id,
            kind: FieldKind::Signature,
            page: 1,
            x: 0.1,
            y: 0.7,
            width: 0.3,
            height: 0.05,
            required: true,
            value: None,
        })
        .collect();
    let envelope = service
        .update(
            "owner",
            envelope.id,
            Draft {
                title: "NDA".into(),
                message: "Please sign".into(),
                revision: 0,
                recipients,
                fields,
            },
        )
        .await
        .unwrap();
    (service, repo, mail, envelope)
}
fn submission(session: &SigningSession) -> SignatureSubmission {
    SignatureSubmission {
        revision: session.revision,
        consent: true,
        values: session
            .fields
            .iter()
            .map(|f| FieldValue {
                field_id: f.id,
                value: "Alex Morgan".into(),
            })
            .collect(),
    }
}
#[tokio::test]
async fn ordered_signing_completes_and_never_exposes_other_fields() {
    let (service, repo, mail, envelope) = setup(&[1, 2]).await;
    let sent = service
        .send("owner", envelope.id, envelope.revision)
        .await
        .unwrap();
    assert_eq!(sent.status, Status::Sent);
    assert_eq!(mail.0.lock().unwrap().len(), 1);
    let first = mail.0.lock().unwrap()[0].token.clone();
    assert!(
        !serde_json::to_string(&repo.load(envelope.id).await.unwrap().state)
            .unwrap()
            .contains(&first)
    );
    let session = service.session(&first).await.unwrap();
    assert_eq!(session.fields.len(), 1);
    let signed = service
        .sign(&first, submission(&session), "browser".into())
        .await
        .unwrap();
    assert!(signed.recipient.signed_at.is_some());
    assert_eq!(signed.status, Status::Sent);
    assert_eq!(mail.0.lock().unwrap().len(), 2);
    let second = mail.0.lock().unwrap()[1].token.clone();
    let session = service.session(&second).await.unwrap();
    assert_ne!(session.recipient.id, signed.recipient.id);
    let completed = service
        .sign(&second, submission(&session), "browser".into())
        .await
        .unwrap();
    assert_eq!(completed.status, Status::Completed);
    assert!(
        service
            .signing_document(&first, true)
            .await
            .unwrap()
            .ends_with(b"signed with certificate")
    );
    assert!(
        service
            .get("owner", envelope.id)
            .await
            .unwrap()
            .completed_sha256
            .is_some()
    );
}
#[tokio::test]
async fn another_account_cannot_read_edit_void_or_download() {
    let (service, _, _, envelope) = setup(&[1]).await;
    assert!(matches!(
        service.get("attacker", envelope.id).await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        service.document("attacker", envelope.id, false).await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        service
            .send("attacker", envelope.id, envelope.revision)
            .await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        service
            .void("attacker", envelope.id, envelope.revision, "bad".into())
            .await,
        Err(Error::NotFound)
    ));
    assert!(service.list("attacker").await.unwrap().is_empty());
}
#[tokio::test]
async fn consent_required_fields_and_field_ownership_are_enforced() {
    let (service, _, mail, envelope) = setup(&[1, 1]).await;
    service
        .send("owner", envelope.id, envelope.revision)
        .await
        .unwrap();
    let token = mail.0.lock().unwrap()[0].token.clone();
    let other = mail.0.lock().unwrap()[1].token.clone();
    let session = service.session(&token).await.unwrap();
    let mut input = submission(&session);
    input.consent = false;
    assert!(matches!(
        service.sign(&token, input, String::new()).await,
        Err(Error::Invalid(_))
    ));
    let mut input = submission(&session);
    input.values.clear();
    assert!(matches!(
        service.sign(&token, input, String::new()).await,
        Err(Error::Invalid(_))
    ));
    let mut input = submission(&session);
    input.values[0].field_id = service.session(&other).await.unwrap().fields[0].id;
    assert!(matches!(
        service.sign(&token, input, String::new()).await,
        Err(Error::Invalid(_))
    ));
    let mut input = submission(&session);
    input.values.push(FieldValue {
        field_id: input.values[0].field_id,
        value: "duplicate".into(),
    });
    assert!(matches!(
        service.sign(&token, input, String::new()).await,
        Err(Error::Invalid(_))
    ));
}
#[tokio::test]
async fn stale_versions_and_replayed_signatures_cannot_mutate_state() {
    let (service, _, mail, envelope) = setup(&[1]).await;
    assert!(matches!(
        service.send("owner", envelope.id, 0).await,
        Err(Error::Conflict(_))
    ));
    service
        .send("owner", envelope.id, envelope.revision)
        .await
        .unwrap();
    let token = mail.0.lock().unwrap()[0].token.clone();
    let session = service.session(&token).await.unwrap();
    service
        .sign(&token, submission(&session), String::new())
        .await
        .unwrap();
    assert!(matches!(
        service
            .sign(&token, submission(&session), String::new())
            .await,
        Err(Error::Conflict(_))
    ));
    let session = service.session(&token).await.unwrap();
    assert!(matches!(
        service
            .sign(&token, submission(&session), String::new())
            .await,
        Err(Error::Conflict(_))
    ));
}
#[tokio::test]
async fn sent_envelopes_are_frozen_and_void_revokes_every_capability() {
    let (service, _, mail, envelope) = setup(&[1]).await;
    let sent = service
        .send("owner", envelope.id, envelope.revision)
        .await
        .unwrap();
    let token = mail.0.lock().unwrap()[0].token.clone();
    assert!(matches!(
        service
            .update(
                "owner",
                envelope.id,
                Draft {
                    title: "changed".into(),
                    message: String::new(),
                    revision: sent.revision,
                    recipients: vec![],
                    fields: vec![]
                }
            )
            .await,
        Err(Error::Conflict(_))
    ));
    service
        .void(
            "owner",
            envelope.id,
            sent.revision,
            "Replaced by new agreement".into(),
        )
        .await
        .unwrap();
    assert!(matches!(service.session(&token).await, Err(Error::Gone)));
    assert!(matches!(
        service.signing_document(&token, false).await,
        Err(Error::Gone)
    ));
}
#[tokio::test]
async fn expiration_and_decline_close_signing() {
    let (service, repo, mail, envelope) = setup(&[1]).await;
    service
        .send("owner", envelope.id, envelope.revision)
        .await
        .unwrap();
    let token = mail.0.lock().unwrap()[0].token.clone();
    repo.0
        .lock()
        .unwrap()
        .get_mut(&envelope.id)
        .unwrap()
        .state
        .envelope
        .expires_at = Some(Utc::now() - Duration::seconds(1));
    assert!(matches!(service.session(&token).await, Err(Error::Gone)));
    repo.0
        .lock()
        .unwrap()
        .get_mut(&envelope.id)
        .unwrap()
        .state
        .envelope
        .expires_at = Some(Utc::now() + Duration::days(1));
    let session = service.session(&token).await.unwrap();
    service
        .decline(&token, session.revision, "Need revised terms".into())
        .await
        .unwrap();
    assert_eq!(
        service.get("owner", envelope.id).await.unwrap().status,
        Status::Declined
    );
    assert!(matches!(service.session(&token).await, Err(Error::Gone)));
}
#[tokio::test]
async fn resend_rotates_capabilities_and_mail_failure_is_visible() {
    let (service, _, mail, envelope) = setup(&[1]).await;
    *mail.1.lock().unwrap() = true;
    let sent = service
        .send("owner", envelope.id, envelope.revision)
        .await
        .unwrap();
    assert!(sent.recipients[0].delivered_at.is_none());
    *mail.1.lock().unwrap() = false;
    let resent = service.resend("owner", envelope.id).await.unwrap();
    assert!(resent.recipients[0].delivered_at.is_some());
    let old = mail.0.lock().unwrap()[0].token.clone();
    service.resend("owner", envelope.id).await.unwrap();
    assert!(matches!(service.session(&old).await, Err(Error::NotFound)));
    let new = mail.0.lock().unwrap()[1].token.clone();
    assert!(service.session(&new).await.unwrap().can_sign);
}
#[tokio::test]
async fn concurrent_signing_has_one_winner_and_one_certificate() {
    let (service, repo, mail, envelope) = setup(&[1]).await;
    service
        .send("owner", envelope.id, envelope.revision)
        .await
        .unwrap();
    let token = mail.0.lock().unwrap()[0].token.clone();
    let session = service.session(&token).await.unwrap();
    let (a, b) = tokio::join!(
        service.sign(&token, submission(&session), "a".into()),
        service.sign(&token, submission(&session), "b".into())
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let stored = repo.load(envelope.id).await.unwrap();
    assert_eq!(
        stored
            .state
            .envelope
            .audit
            .iter()
            .filter(|e| matches!(e.action, AuditAction::Signed))
            .count(),
        1
    );
}
#[tokio::test]
async fn invalid_fields_and_missing_signatures_cannot_be_sent() {
    let (service, _, _, envelope) = setup(&[1]).await;
    let mut fields = envelope.fields.clone();
    fields[0].x = 0.99;
    assert!(matches!(
        service
            .update(
                "owner",
                envelope.id,
                Draft {
                    title: envelope.title.clone(),
                    message: String::new(),
                    revision: envelope.revision,
                    recipients: envelope.recipients.clone(),
                    fields
                }
            )
            .await,
        Err(Error::Invalid(_))
    ));
    let draft = service
        .update(
            "owner",
            envelope.id,
            Draft {
                title: envelope.title,
                message: String::new(),
                revision: envelope.revision,
                recipients: envelope.recipients,
                fields: vec![],
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        service.send("owner", draft.id, draft.revision).await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        service.session("short").await,
        Err(Error::NotFound)
    ));
}
