//! ReorderColumns tool: set the display order of a table's columns.

use ai_toolset::{
    AsyncTool, RequestContext, ServiceContext, ToolAnnotated, ToolAnnotations, ToolResult,
};
use async_trait::async_trait;
use entity_access::domain::ports::EntityAccessService;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{DatabasesToolContext, ToolDatabaseSchema, database_error, table_of};
use crate::domain::ports::DatabasesService;

/// Reorder a table's columns.
#[derive(Debug, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
#[schemars(
    title = "ReorderColumns",
    description = "\
Set the order columns appear in, left to right. Pass every column id of the table exactly \
once, in the new order; values and names are untouched.\n\
\n\
Requires edit access. The response is the schema after the change."
)]
pub struct ReorderColumns {
    /// The database containing the table.
    #[schemars(description = "Id of the database containing the table, from ListDatabases.")]
    pub database_id: Uuid,
    /// The table whose columns to reorder.
    #[schemars(description = "Id of the table, from DescribeDatabase.")]
    pub table_id: Uuid,
    /// Every column id, in the new order.
    #[schemars(
        description = "Every column id of the table, exactly once, in the new left-to-right \
                       order."
    )]
    pub column_ids: Vec<Uuid>,
}

impl ToolAnnotated for ReorderColumns {
    const ANNOTATIONS: ToolAnnotations =
        ToolAnnotations::destructive("Reorder columns").with_idempotent();
}

/// Response from the ReorderColumns tool.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReorderColumnsResponse {
    /// Database containing the table.
    pub database_id: Uuid,
    /// The reordered table.
    pub table_id: Uuid,
    /// The database's schema after the change.
    pub database: Option<ToolDatabaseSchema>,
    /// A failed follow-up read does not undo the committed order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[async_trait]
impl<S, E> AsyncTool<DatabasesToolContext<S, E>> for ReorderColumns
where
    S: DatabasesService,
    E: EntityAccessService,
{
    type Output = ReorderColumnsResponse;

    #[tracing::instrument(skip_all, fields(
        user_id = ?request_context.user_id,
        database_id = %self.database_id,
        table_id = %self.table_id,
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
        let base_version = table_of(&detail, self.table_id)?.table.version;

        let receipt = service_context
            .edit_receipt(user_id, self.database_id)
            .await?;
        service_context
            .service
            .reorder_columns(
                receipt,
                self.table_id,
                self.column_ids.clone(),
                base_version,
            )
            .await
            .map_err(database_error)?;

        let (database, warning) = service_context
            .schema_after_write(user_id, self.database_id)
            .await;
        Ok(ReorderColumnsResponse {
            database_id: self.database_id,
            table_id: self.table_id,
            database,
            warning,
        })
    }
}
