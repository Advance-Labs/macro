//! Which databases a viewer can reach, asked of the owning `entity_access`
//! domain. Which source ids stand for a viewer, and which grant wins, are its
//! rules; a second copy here is a permission bug waiting to happen.

#[cfg(all(test, feature = "postgres"))]
mod test;

use entity_access::domain::models::{AccessError, AccessLevel, EntityType};
use entity_access::domain::ports::{AccessibleDatabases, EntityAccessService};

use crate::domain::models::{DatabaseId, Viewer};
use crate::domain::ports::AccessDirectory;

/// The directory's error: whatever `entity_access` reports.
#[derive(Debug, thiserror::Error)]
#[error("entity access: {0}")]
pub struct EntityAccessDirectoryError(#[from] AccessError);

/// [`AccessDirectory`] over the entity access service.
#[derive(Debug, Clone)]
pub struct EntityAccessDirectory<Access> {
    access: Access,
}

impl<Access> EntityAccessDirectory<Access> {
    /// Wrap the entity access service.
    pub fn new(access: Access) -> Self {
        Self { access }
    }
}

impl<Access> AccessDirectory for EntityAccessDirectory<Access>
where
    Access: EntityAccessService + AccessibleDatabases,
{
    type Err = EntityAccessDirectoryError;

    #[tracing::instrument(err, skip(self, viewer))]
    async fn accessible_databases(
        &self,
        viewer: &Viewer,
    ) -> Result<Vec<(DatabaseId, AccessLevel)>, Self::Err> {
        Ok(self.access.accessible_databases(&viewer.user_id).await?)
    }

    #[tracing::instrument(err, skip(self, viewer))]
    async fn database_access(
        &self,
        viewer: &Viewer,
        database_id: DatabaseId,
    ) -> Result<Option<AccessLevel>, Self::Err> {
        Ok(self
            .access
            .get_access_level(
                Some(&viewer.user_id),
                &database_id.to_string(),
                EntityType::Database,
            )
            .await?)
    }
}
