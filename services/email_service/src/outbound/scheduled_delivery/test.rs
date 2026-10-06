use email::domain::scheduled_delivery::{PreparationError, SubmissionError};
use email_api_client::domain::models::{EmailApiError, RateLimitOrigin, SentIds};
use models_email::service::message::MessageToSend;
use uuid::Uuid;

use super::{apply_sent_ids, classify_preparation_error, classify_submission_error};

#[test]
fn response_decode_failure_is_ambiguous_only_after_submission() {
    let error = EmailApiError::Permanent {
        message: "invalid provider response".into(),
    };
    assert!(matches!(
        classify_preparation_error(error.clone().into()),
        PreparationError::Failed(_)
    ));
    assert!(matches!(
        classify_submission_error(error),
        SubmissionError::Uncertain(_)
    ));
}

#[test]
fn transient_failure_can_retry_only_before_submission() {
    let error = EmailApiError::Transient {
        message: "request timed out".into(),
    };
    assert!(matches!(
        classify_preparation_error(error.clone().into()),
        PreparationError::Retry(_)
    ));
    assert!(matches!(
        classify_submission_error(error),
        SubmissionError::Uncertain(_)
    ));
}

#[test]
fn explicit_provider_refusal_is_safe_to_restore() {
    for error in [
        EmailApiError::AuthRequired,
        EmailApiError::Forbidden,
        EmailApiError::NotFound,
        EmailApiError::Conflict,
        EmailApiError::SendRejected {
            message: "message exceeded the provider size limit".into(),
        },
        EmailApiError::RateLimited {
            retry_after: None,
            origin: RateLimitOrigin::Provider,
        },
    ] {
        assert!(matches!(
            classify_submission_error(error),
            SubmissionError::Rejected(_)
        ));
    }
}

#[test]
fn sent_ids_are_written_back_for_existing_persistence_flow() {
    let mut message = message_to_send();

    apply_sent_ids(
        &mut message,
        SentIds {
            provider_message_id: "provider-message".to_string(),
            provider_thread_id: "provider-thread".to_string(),
        },
    );

    assert_eq!(message.provider_id.as_deref(), Some("provider-message"));
    assert_eq!(
        message.provider_thread_id.as_deref(),
        Some("provider-thread")
    );
}

fn message_to_send() -> MessageToSend {
    MessageToSend {
        db_id: Some(Uuid::new_v4()),
        provider_id: None,
        replying_to_id: None,
        provider_thread_id: None,
        thread_db_id: Some(Uuid::new_v4()),
        link_id: Uuid::new_v4(),
        subject: "subject".to_string(),
        to: None,
        cc: None,
        bcc: None,
        body_text: Some("body".to_string()),
        body_html: None,
        body_macro: None,
        attachments: None,
        headers_json: None,
        send_time: None,
    }
}
