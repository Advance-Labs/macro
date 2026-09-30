//! Sharing uses the same explicit channel grants as other collaboration
//! entities, over the shared `SharePermissionV2` wire shape. Databases have
//! no share link and no team share yet: both read back as `null`, and a
//! request to turn either on is refused.

use entity_access::domain::models::{EntityAccessReceipt, OwnerAccessLevel};
use models_permissions::share_permission::channel_share_permission::{
    ChannelSharePermission, UpdateChannelSharePermission,
};
use models_permissions::share_permission::{SharePermissionV2, UpdateSharePermissionRequestV2};

use super::models::{DatabaseError, DatabaseId};

/// Persistence of direct channel grants, implemented through the owning access crate.
pub trait DatabaseSharingRepo: Send + Sync + 'static {
    /// Persistence failure.
    type Err: std::error::Error + Send + Sync + 'static;

    /// Read the direct channel grants for a database.
    fn channel_grants(
        &self,
        database_id: DatabaseId,
    ) -> impl Future<Output = Result<Vec<ChannelSharePermission>, Self::Err>> + Send;

    /// Change channel grants only while the database remains live.
    fn update_channel_grants(
        &self,
        database_id: DatabaseId,
        grants: &[UpdateChannelSharePermission],
    ) -> impl Future<Output = Result<bool, Self::Err>> + Send;
}

/// Owners control the database's recipients and each recipient's access level.
pub trait DatabaseSharingService: Send + Sync + 'static {
    /// Read sharing details after proving ownership.
    fn share_permissions(
        &self,
        receipt: EntityAccessReceipt<OwnerAccessLevel>,
    ) -> impl Future<Output = Result<SharePermissionV2, DatabaseError>> + Send;

    /// Update explicit channel grants without modifying ownership.
    fn update_share_permissions(
        &self,
        receipt: EntityAccessReceipt<OwnerAccessLevel>,
        request: UpdateSharePermissionRequestV2,
    ) -> impl Future<Output = Result<SharePermissionV2, DatabaseError>> + Send;
}
