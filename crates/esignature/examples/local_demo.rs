//! Isolated loopback demo using the production routers, Postgres/PDF adapters,
//! and real SMTP delivery to Mailpit. Never deploy this example.
//! Run with SMTP_HOST=localhost SMTP_PORT=11025 and features inbound,postgres,pdf,mail.
use esignature::{
    domain::service::Service,
    inbound::router::{RouterState, management_router, signing_router},
    outbound::{mail::Mail, pdf::Pdf, postgres::Postgres},
};
use macro_authorization::{
    InternalAuthConfig, JwtValidator, MacroAuthorizationError, MacroAuthorizationServiceImpl,
    MacroAuthorizationState, NoBotAuthorizer, NoUserApiKeyAuthorizer, ValidatedIdentity,
};
use rootcause::Report;
use std::sync::Arc;
#[derive(Clone)]
struct DemoCredential;
impl JwtValidator for DemoCredential {
    fn validate(&self, jwt: &str) -> Result<ValidatedIdentity, Report<MacroAuthorizationError>> {
        if jwt != "legal-local-demo" {
            return Err(Report::new(MacroAuthorizationError::InvalidCredentials));
        }
        Ok(ValidatedIdentity {
            user_id: "macro|legal-demo@example.com".into(),
            fusion_user_id: "legal-demo".into(),
            organization_id: None,
            permissions: None,
        })
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect("postgres://user:password@localhost:55432/macrodb")
        .await?;
    let config = aws_sdk_sesv2::config::Builder::new()
        .region(aws_sdk_sesv2::config::Region::new("us-east-1"))
        .behavior_version_latest()
        .build();
    let mail = Mail::new(
        ses_client::SesClient::from_env(aws_sdk_sesv2::Client::from_conf(config), "local"),
        "http://localhost:3003".into(),
    );
    let service = Arc::new(Service::new(Postgres::new(pool), Pdf, mail));
    let auth = Arc::new(MacroAuthorizationServiceImpl::new(
        DemoCredential,
        InternalAuthConfig {
            api_key: "unused-local-demo-internal".into(),
            default_user_id: None,
        },
        NoBotAuthorizer,
        NoUserApiKeyAuthorizer,
    ));
    let app = axum::Router::new()
        .nest(
            "/legal",
            management_router(RouterState {
                service: service.clone(),
                authorization_state: MacroAuthorizationState::new(auth),
            }),
        )
        .nest("/legal/signing", signing_router(service));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8118").await?;
    eprintln!("Local Legal demo listening on 127.0.0.1:8118");
    axum::serve(listener, app).await?;
    Ok(())
}
