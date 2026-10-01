//! The engine's ops sink on the server: a statement's ops are applied by the
//! databases service under an edit receipt for their database, minted the
//! way the HTTP routes mint it, so SQL writes nothing the viewer could not
//! write through `POST /databases/{id}/ops`.

use std::collections::HashMap;
use std::sync::Mutex;

use database_sql::run::{OpsSink, WriteError};
use databases::domain::models::{DatabaseError, TableId, TableVersion, Viewer};
use databases::domain::ports::DatabasesService;
use entity_access::domain::models::EditAccessLevel;
use entity_access::domain::ports::EntityAccessService;
use models_databases::{DatabaseOp, OpResult};
use uuid::Uuid;

use crate::service::receipt;

/// Applies a statement's writes as `viewer`, keeping the table versions
/// they produced.
pub(crate) struct ReceiptOpsSink<'a, Databases, Access> {
    pub(crate) databases: &'a Databases,
    pub(crate) entity_access: &'a Access,
    pub(crate) viewer: &'a Viewer,
    pub(crate) versions: Mutex<HashMap<TableId, TableVersion>>,
}

impl<Databases, Access> OpsSink for ReceiptOpsSink<'_, Databases, Access>
where
    Databases: DatabasesService,
    Access: EntityAccessService,
{
    async fn apply(
        &self,
        database: Uuid,
        ops: Vec<DatabaseOp>,
    ) -> Result<Vec<OpResult>, WriteError> {
        let receipt = receipt::<EditAccessLevel, _>(self.entity_access, self.viewer, database)
            .await
            .map_err(|_| WriteError(format!("the user cannot edit database {database}")))?;
        let tables: Vec<TableId> = ops.iter().map(op_table).collect();
        let results = self
            .databases
            .apply_ops(receipt, self.viewer.clone(), ops)
            .await
            .map_err(write_error)?;
        let mut versions = self.versions.lock().expect("version log");
        for (table, result) in tables.into_iter().zip(&results) {
            let (OpResult::RowsWritten { table_version, .. }
            | OpResult::ColumnTyped { table_version, .. }) = result;
            versions.insert(table, *table_version);
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

fn op_table(op: &DatabaseOp) -> TableId {
    match op {
        DatabaseOp::InsertRows { table, .. }
        | DatabaseOp::UpdateRows { table, .. }
        | DatabaseOp::DeleteRows { table, .. }
        | DatabaseOp::ChangeColumnType { table, .. } => *table,
    }
}
