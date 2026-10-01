//! The engine's ops sink on the server: a statement's ops are applied by the
//! databases service under the edit receipt minted for its database.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use database_sql::run::OpsSink;
use databases::domain::models::{DatabaseError, DatabaseId, TableId, TableVersion, Viewer};
use databases::domain::ports::DatabasesService;
use entity_access::domain::models::{EditAccessLevel, EntityAccessReceipt};
use models_databases::{DatabaseOp, OpResult};
use uuid::Uuid;

/// Applies a statement's writes as `viewer`, keeping the table versions
/// they produced.
pub(crate) struct ReceiptOpsSink<'statement, Databases> {
    pub(crate) databases: &'statement Databases,
    /// The database the statement writes and the edit receipt minted for it
    /// before the statement ran; `None` for a read.
    pub(crate) receipt: Option<(DatabaseId, EntityAccessReceipt<EditAccessLevel>)>,
    pub(crate) viewer: &'statement Viewer,
    pub(crate) versions: Mutex<HashMap<TableId, TableVersion>>,
}

/// Why a statement's ops did not land.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ReceiptWriteError {
    /// The engine wrote a database the statement was not authorized for.
    #[error("the statement was not authorized to write database {database}")]
    UnauthorizedDatabase {
        /// The database written.
        database: Uuid,
    },
    /// The databases service refused or failed the ops.
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

impl<Databases> OpsSink for ReceiptOpsSink<'_, Databases>
where
    Databases: DatabasesService,
{
    type Error = ReceiptWriteError;

    async fn apply(
        &self,
        database: Uuid,
        ops: Vec<DatabaseOp>,
    ) -> Result<Vec<OpResult>, Self::Error> {
        let receipt = match &self.receipt {
            Some((authorized, receipt)) if *authorized == database => receipt.clone(),
            _ => return Err(ReceiptWriteError::UnauthorizedDatabase { database }),
        };
        let tables: Vec<TableId> = ops.iter().map(DatabaseOp::table).collect();
        let results = self
            .databases
            .apply_ops(receipt, self.viewer.clone(), ops)
            .await?;
        // The lock only guards single inserts, so a poisoned map is still whole.
        let mut versions = self.versions.lock().unwrap_or_else(PoisonError::into_inner);
        for (table, result) in tables.into_iter().zip(&results) {
            versions.insert(table, result.table_version());
        }
        Ok(results)
    }
}
