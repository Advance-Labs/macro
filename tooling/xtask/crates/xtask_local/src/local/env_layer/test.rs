use super::*;

#[test]
fn keeps_resolved_database_url_when_shell_has_local_default() {
    assert!(!should_overlay_process_env(
        "DATABASE_URL",
        "postgres://user:password@localhost:5432/macrodb"
    ));
    assert!(!should_overlay_process_env(
        "DATABASE_URL",
        "postgres://user:password@postgres:5432/macrodb"
    ));
}

#[test]
fn still_allows_non_local_database_overrides() {
    assert!(should_overlay_process_env(
        "DATABASE_URL",
        "postgres://user:password@dev.example.com:5432/macrodb"
    ));
}

#[test]
fn gmail_pubsub_topic_feeds_the_email_service_watch_topic() {
    let mut env = BTreeMap::from([
        (
            "GMAIL_GCP_QUEUE".to_string(),
            "gmail-gcp-queue-local".to_string(),
        ),
        (
            "GMAIL_PUBSUB_TOPIC".to_string(),
            " projects/advlabs-macro-ops/topics/gmail-watch ".to_string(),
        ),
    ]);
    apply_gmail_pubsub_topic(&mut env);
    assert_eq!(
        env["GMAIL_GCP_QUEUE"],
        "projects/advlabs-macro-ops/topics/gmail-watch"
    );
}

#[test]
fn unset_or_blank_gmail_pubsub_topic_keeps_the_resolved_queue() {
    for topic in [None, Some("  ")] {
        let mut env = BTreeMap::from([(
            "GMAIL_GCP_QUEUE".to_string(),
            "gmail-gcp-queue-local".to_string(),
        )]);
        if let Some(t) = topic {
            env.insert("GMAIL_PUBSUB_TOPIC".to_string(), t.to_string());
        }
        apply_gmail_pubsub_topic(&mut env);
        assert_eq!(env["GMAIL_GCP_QUEUE"], "gmail-gcp-queue-local");
    }
}
