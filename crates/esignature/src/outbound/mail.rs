use crate::domain::{
    models::{Error, Invitation},
    ports::Invitations,
};
use async_trait::async_trait;
use ses_client::SesClient;

/// Invitation delivery through the existing SES/local SMTP transport.
#[derive(Clone)]
pub struct Mail {
    client: SesClient,
    frontend: String,
}
impl Mail {
    /// Use a deployment-controlled frontend URL, never request-supplied origins.
    pub fn new(client: SesClient, frontend: String) -> Self {
        Self {
            client,
            frontend: frontend.trim_end_matches('/').into(),
        }
    }
}
#[async_trait]
impl Invitations for Mail {
    async fn deliver(&self, invitation: Invitation) -> Result<(), Error> {
        let escape = html_escape::encode_safe;
        let url = format!("{}/app/sign#{}", self.frontend, invitation.token);
        let html = format!(
            "<h2>Your signature is requested</h2><p>Hello {},</p><p>{}</p><p>{}</p><p><a href=\"{}\">Review and sign</a></p><p>This private link is for you only and expires in 30 days. Do not forward it.</p>",
            escape(&invitation.name),
            escape(&invitation.title),
            escape(&invitation.message),
            escape(&url)
        );
        self.client
            .send_email(
                "legal@macro.com",
                &invitation.email,
                &format!("Signature requested: {}", invitation.title),
                &html,
            )
            .await
            .map_err(|_| Error::Delivery)
    }
}
