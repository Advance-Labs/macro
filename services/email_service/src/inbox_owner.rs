//! Selects the owner of an OAuth-authorized mailbox before provisioning or delegation.

use macro_user_id::user_id::MacroUserIdStr;
use rootcause::Report;
use std::future::Future;
use uuid::Uuid;

#[cfg(test)]
mod test;

/// A Macro profile and the FusionAuth identity that holds its mailbox grant.
#[derive(Clone, Debug)]
pub struct InboxOwner {
    /// The profile that owns the mailbox; delegates receive access to one link only.
    pub macro_id: MacroUserIdStr<'static>,
    /// The existing Google sign-in identity. Connecting a mailbox must not move it.
    pub fusionauth_id: Uuid,
}

/// Account and mailbox facts used to select the provisioning path.
pub trait InboxOwnerRepository: Send + Sync {
    /// Looks up the Macro profile registered with this mailbox address.
    fn by_email(
        &self,
        email: &str,
    ) -> impl Future<Output = Result<Option<InboxOwner>, Report>> + Send;
    /// Whether a mailbox already exists, in which case the existing sharing flow owns it.
    fn has_inbox(&self, email: &str) -> impl Future<Output = Result<bool, Report>> + Send;
    /// Whether the requester already has an edge to this mailbox on this grant owner.
    fn already_delegated(
        &self,
        email: &str,
        grant_owner: Uuid,
        requester: Uuid,
    ) -> impl Future<Output = Result<bool, Report>> + Send;
    /// Resolves the primary Macro profile of a verified grant owner.
    fn by_fusionauth_id(&self, id: Uuid)
    -> impl Future<Output = Result<InboxOwner, Report>> + Send;
}

/// Resolves ownership without treating an authorized secondary email as a login migration.
#[derive(Clone)]
pub struct InboxOwnerService<R> {
    /// The account repository.
    pub repo: R,
}

impl<R: InboxOwnerRepository> InboxOwnerService<R> {
    /// The grant owner must come from the server's completed OAuth record, never the client.
    #[tracing::instrument(skip(self, email), err)]
    pub async fn resolve(
        &self,
        email: &str,
        verified_grant_owner: Option<Uuid>,
        requester: Uuid,
    ) -> Result<Option<InboxOwner>, Report> {
        if let Some(owner) = self.repo.by_email(email).await? {
            if let Some(grant_owner) = verified_grant_owner
                && owner.fusionauth_id != grant_owner
            {
                return Err(rootcause::report!(
                    "Mailbox profile does not match Google grant owner"
                ));
            }
            return Ok(Some(owner));
        }
        // Old callbacks have no owner metadata and retain their existing behavior.
        // Existing inboxes retain the explicit shared-inbox confirmation/promotion flow.
        match verified_grant_owner {
            Some(owner)
                if owner != requester
                    && (!self.repo.has_inbox(email).await?
                        || self.repo.already_delegated(email, owner, requester).await?) =>
            {
                self.repo.by_fusionauth_id(owner).await.map(Some)
            }
            _ => Ok(None),
        }
    }
}
