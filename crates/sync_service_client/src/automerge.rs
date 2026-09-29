//! Native Automerge service contracts. Legacy callers keep their existing Loro
//! protocol types until their editor, snapshot seeds, and caches are migrated.
use serde::Serialize;
use uuid::Uuid;

use crate::{
    SyncServiceClient,
    surface::{SnapshotProof, SurfaceOperationId, SurfaceSnapshot, SurfaceSyncError},
};

/// Sorted hexadecimal Automerge change hashes (the causal heads of a document).
pub type Heads = Vec<String>;
/// A receipt for a native Automerge surface snapshot.
pub type AutomergeSnapshotProof = SnapshotProof<Heads>;
/// Frozen native Automerge snapshot with its verified receipt.
pub type AutomergeSurfaceSnapshot = SurfaceSnapshot<Heads>;

impl SyncServiceClient {
    /// Copy the current state, or a historical state identified by Automerge heads.
    pub async fn copy_automerge_document(
        &self,
        source: &str,
        target: &str,
        heads: Option<&[String]>,
    ) -> Result<(), SurfaceSyncError> {
        #[derive(Serialize)]
        struct Copy<'a> {
            target_document_id: &'a str,
            version_id: Option<&'a [String]>,
        }
        let response = self
            .client
            .post(format!("{}/document/{source}/copy", self.url))
            .json(&Copy {
                target_document_id: target,
                version_id: heads,
            })
            .send()
            .await
            .map_err(SurfaceSyncError::Transport)?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(SurfaceSyncError::Rejected(response.status()));
        }
        Ok(())
    }

    /// Seed an isolated Automerge surface with an idempotent operation identity.
    pub async fn initialize_automerge_surface(
        &self,
        id: Uuid,
        operation_id: SurfaceOperationId,
        snapshot: &[u8],
    ) -> Result<AutomergeSnapshotProof, SurfaceSyncError> {
        #[derive(Serialize)]
        struct Initialize<'a> {
            operation_id: SurfaceOperationId,
            snapshot: &'a [u8],
        }
        self.surface_json(
            &format!("/surface/{id}/initialize_verified"),
            &Initialize {
                operation_id,
                snapshot,
            },
        )
        .await
    }

    /// Freeze an Automerge document before moving it to an isolated surface.
    pub async fn freeze_automerge_surface(
        &self,
        id: Uuid,
        operation_id: SurfaceOperationId,
    ) -> Result<AutomergeSurfaceSnapshot, SurfaceSyncError> {
        #[derive(Serialize)]
        struct Freeze {
            operation_id: SurfaceOperationId,
        }
        self.surface_json(
            &format!("/document/{id}/migration/freeze"),
            &Freeze { operation_id },
        )
        .await
    }

    /// Import a verified Automerge snapshot into a non-writable surface.
    pub async fn import_automerge_surface(
        &self,
        id: Uuid,
        snapshot: &AutomergeSurfaceSnapshot,
    ) -> Result<AutomergeSnapshotProof, SurfaceSyncError> {
        self.surface_json(&format!("/surface/{id}/import"), snapshot)
            .await
    }

    /// Read back and verify an Automerge surface before activation.
    pub async fn verify_automerge_surface(
        &self,
        id: Uuid,
        proof: &AutomergeSnapshotProof,
    ) -> Result<AutomergeSnapshotProof, SurfaceSyncError> {
        self.surface_json(&format!("/surface/{id}/verify"), proof)
            .await
    }

    /// Seal the source and activate the target after the caller records commit intent.
    pub async fn activate_automerge_surface(
        &self,
        id: Uuid,
        proof: &AutomergeSnapshotProof,
    ) -> Result<AutomergeSnapshotProof, SurfaceSyncError> {
        self.surface_json(&format!("/surface/{id}/activate"), proof)
            .await
    }

    /// Restore the original source before committing activation.
    pub async fn thaw_automerge_surface(
        &self,
        id: Uuid,
        proof: &AutomergeSnapshotProof,
    ) -> Result<AutomergeSnapshotProof, SurfaceSyncError> {
        self.surface_json(&format!("/document/{id}/migration/thaw"), proof)
            .await
    }

    /// Retire a sealed source after activating its native Automerge surface.
    pub async fn retire_automerge_surface(
        &self,
        id: Uuid,
        proof: &AutomergeSnapshotProof,
    ) -> Result<AutomergeSnapshotProof, SurfaceSyncError> {
        self.surface_json(&format!("/document/{id}/migration/retire"), proof)
            .await
    }
}
