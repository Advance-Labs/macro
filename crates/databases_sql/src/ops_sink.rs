//! The engine's ops sink on the server: a statement's ops are applied by the
//! databases service under the edit receipt minted for its database.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use database_sql::run::{OpsSink, WriteError};
use databases::domain::models::{DatabaseError, DatabaseId, TableId, TableVersion, Viewer};
use databases::domain::ports::DatabasesService;
use entity_access::domain::models::{EditAccessLevel, EntityAccessReceipt};
use models_databases::{DatabaseOp, OpResult};
use uuid::Uuid;

/// Applies a statement's writes as `viewer`, keeping the table versions
/// they produced.
pub(crate) struct ReceiptOpsSink<'a, Databases> {
    pub(crate) databases: &'a Databases,
    /// The database the statement writes and the edit receipt minted for it
    /// before the statement ran; `None` for a read.
    pub(crate) receipt: Option<(DatabaseId, EntityAccessReceipt<EditAccessLevel>)>,
    pub(crate) viewer: &'a Viewer,
    pub(crate) versions: Mutex<HashMap<TableId, TableVersion>>,
}

impl<Databases> OpsSink for ReceiptOpsSink<'_, Databases>
where
    Databases: DatabasesService,
{
    async fn apply(
        &self,
        database: Uuid,
        ops: Vec<DatabaseOp>,
    ) -> Result<Vec<OpResult>, WriteError> {
        let receipt = match &self.receipt {
            Some((authorized, receipt)) if *authorized == database => receipt.clone(),
            _ => {
                tracing::error!(%database, "a statement wrote a database it was not authorized for");
                return Err(WriteError(format!(
                    "the statement was not authorized to write database {database}"
                )));
            }
        };
        let tables: Vec<TableId> = ops.iter().map(DatabaseOp::table).collect();
        let results = self
            .databases
            .apply_ops(receipt, self.viewer.clone(), ops)
            .await
            .map_err(write_error)?;
        // The lock only guards single inserts, so a poisoned map is still whole.
        let mut versions = self.versions.lock().unwrap_or_else(PoisonError::into_inner);
        for (table, result) in tables.into_iter().zip(&results) {
            versions.insert(table, result.table_version());
        }
        Ok(results)
    }
}

/// A refused write in words a model can act on.
fn write_error(error: DatabaseError) -> WriteError {
    WriteError(match error {
        DatabaseError::InvalidOp(refusal) => match refusal.row {
            Some(row) => format!("row {}: {}", row + 1, refusal.reason),
            None => refusal.reason,
        },
        DatabaseError::VersionConflict => {
            "the table changed while the statement ran; run it again".into()
        }
        DatabaseError::NotFound => "the table is gone".into(),
        other => {
            tracing::error!(error = ?other, "a statement's write failed");
            "the write could not be applied".into()
        }
    })
}
