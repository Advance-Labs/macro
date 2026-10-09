//! FusionAuth adapter for the MCP OAuth broker.

use anyhow::Context;
use std::sync::Arc;
use tracing::Instrument;

#[cfg(test)]
mod test;

use crate::domain::{
    models::{RefreshToken, UpstreamTokens},
    ports::{OAuthProvider, UpstreamTokensFuture},
};

/// FusionAuth-backed OAuth provider for the MCP auth proxy.
#[derive(Clone)]
pub struct FusionAuthOAuthProvider {
    client: Arc<fusionauth::FusionAuthClient>,
    /// `None` when FusionAuth has no `google_gmail` identity provider (a
    /// local stack without Google credentials). The service still starts;
    /// only the MCP OAuth authorize step fails.
    google_idp_id: Option<String>,
}

impl FusionAuthOAuthProvider {
    /// Creates a provider and resolves the Google identity provider ID.
    ///
    /// A missing `google_gmail` provider is not fatal: FusionAuth only has it
    /// when Google credentials are configured. Any other lookup error is.
    #[tracing::instrument(skip(client), err)]
    pub async fn new(client: fusionauth::FusionAuthClient) -> anyhow::Result<Self> {
        let google_idp_id = match client
            .get_identity_provider_id_by_name("google_gmail")
            .await
        {
            Ok(id) => {
                tracing::debug!(google_idp_id = %id, "resolved Google IDP ID from FusionAuth");
                Some(id)
            }
            Err(fusionauth::error::FusionAuthClientError::NoIdentityProviderFound) => {
                tracing::warn!(
                    "no Google Gmail identity provider in FusionAuth; MCP OAuth login is disabled"
                );
                None
            }
            Err(err) => {
                return Err(anyhow::Error::from(err)
                    .context("failed to look up Google Gmail identity provider in FusionAuth"));
            }
        };

        Ok(Self::with_google_idp_id(client, google_idp_id))
    }

    fn with_google_idp_id(
        client: fusionauth::FusionAuthClient,
        google_idp_id: Option<String>,
    ) -> Self {
        Self {
            client: Arc::new(client),
            google_idp_id,
        }
    }
}

impl OAuthProvider for FusionAuthOAuthProvider {
    #[tracing::instrument(skip(self), err)]
    fn construct_authorize_url(&self, state: &str) -> anyhow::Result<String> {
        let google_idp_id = self
            .google_idp_id
            .as_deref()
            .context("Google identity provider is not configured in FusionAuth")?;
        self.client
            .construct_oauth2_authorize_url(google_idp_id, None, Some(state.to_owned()))
    }

    fn exchange_authorization_code<'a>(&'a self, code: &'a str) -> UpstreamTokensFuture<'a> {
        let span = tracing::debug_span!("FusionAuthOAuthProvider::exchange_authorization_code");
        Box::pin(
            async move {
                let grant = self
                    .client
                    .complete_authorization_code_grant(code)
                    .await
                    .map_err(anyhow::Error::from)?;

                Ok(UpstreamTokens {
                    access_token: grant.access_token.into(),
                    refresh_token: grant.refresh_token.into(),
                    expires_in: grant.expires_in,
                })
            }
            .instrument(span),
        )
    }

    fn refresh_access_token<'a>(
        &'a self,
        refresh_token: &'a RefreshToken,
    ) -> UpstreamTokensFuture<'a> {
        let span = tracing::debug_span!("FusionAuthOAuthProvider::refresh_access_token");
        Box::pin(
            async move {
                let grant = self
                    .client
                    .complete_refresh_token_grant(refresh_token.as_str())
                    .await
                    .map_err(anyhow::Error::from)?;

                Ok(UpstreamTokens {
                    access_token: grant.access_token.into(),
                    refresh_token: grant.refresh_token.into(),
                    expires_in: grant.expires_in,
                })
            }
            .instrument(span),
        )
    }
}
