//! DeleteColumn tool: remove a column and its values.

use ai_toolset::{
    AsyncTool, RequestContext, ServiceContext, ToolAnnotated, ToolAnnotations, ToolResult,
};
use async_trait::async_trait;
use entity_access::domain::ports::EntityAccessService;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{DatabasesToolContext, ToolDatabaseSchema, column_of, database_error, table_of};
use crate::domain::ports::DatabasesService;

/// Delete a column.
#[derive(Debug, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
#[schemars(
    title = "DeleteColumn",
    description = "\
Delete a column and every value in it. This cannot be undone, so only do it when the user \
asked for that column to go. Deleting a relation column also removes the relationships it \
held. A column that a lookup reads through cannot be deleted until the lookup is.\n\
\n\
Requires edit access. The response is the schema after the change."
)]
pub struct DeleteColumn {
    /// The database containing the column.
    #[schemars(description = "Id of the database containing the column, from ListDatabases.")]
    pub database_id: Uuid,
    /// The table containing the column.
    #[schemars(description = "Id of the table containing the column, from DescribeDatabase.")]
    pub table_id: Uuid,
    /// The column to delete.
    #[schemars(description = "Id of the column to delete, from DescribeDatabase.")]
    pub column_id: Uuid,
}

impl ToolAnnotated for DeleteColumn {
    const ANNOTATIONS: ToolAnnotations = ToolAnnotations::destructive("Delete column");
}

/// Response from the DeleteColumn tool.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteColumnResponse {
    /// Database the column was deleted from.
    pub database_id: Uuid,
    /// Table the column was deleted from.
    pub table_id: Uuid,
    /// The deleted column's id.
    pub column_id: Uuid,
    /// The database's schema after the change.
    pub database: Option<ToolDatabaseSchema>,
    /// A failed follow-up read does not undo the committed delete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[async_trait]
impl<S, E> AsyncTool<DatabasesToolContext<S, E>> for DeleteColumn
where
    S: DatabasesService,
    E: EntityAccessService,
{
    type Output = DeleteColumnResponse;

    #[tracing::instrument(skip_all, fields(
        user_id = ?request_context.user_id,
        database_id = %self.database_id,
        table_id = %self.table_id,
        column_id = %self.column_id,
    ), err)]
    async fn call(
        &self,
        service_context: ServiceContext<DatabasesToolContext<S, E>>,
        request_context: RequestContext,
    ) -> ToolResult<Self::Output> {
        let user_id = &request_context.user_id;
        let detail = service_context
            .current_schema(user_id, self.database_id)
            .await?;
        let table = table_of(&detail, self.table_id)?;
        column_of(table, self.column_id)?;
        let base_version = table.table.version;

        let receipt = service_context
            .edit_receipt(user_id, self.database_id)
            .await?;
        service_context
            .service
            .delete_column(receipt, self.table_id, self.column_id, base_version)
            .await
            .map_err(database_error)?;

        let (database, warning) = service_context
            .schema_after_write(user_id, self.database_id)
            .await;
        Ok(DeleteColumnResponse {
            database_id: self.database_id,
            table_id: self.table_id,
            column_id: self.column_id,
            database,
            warning,
        })
    }
}
