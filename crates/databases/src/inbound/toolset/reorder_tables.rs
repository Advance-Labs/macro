//! ReorderTables tool: set the order a database's tabs appear in.

use ai_toolset::{
    AsyncTool, RequestContext, ServiceContext, ToolAnnotated, ToolAnnotations, ToolResult,
};
use async_trait::async_trait;
use entity_access::domain::ports::EntityAccessService;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{DatabasesToolContext, ToolDatabaseSchema, database_error};
use crate::domain::ports::DatabasesService;

/// Reorder a database's tables.
#[derive(Debug, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
#[schemars(
    title = "ReorderTables",
    description = "\
Set the order a database's tables — what the user sees as tabs — appear in, left to right. \
Records, columns and names are untouched.\n\
\n\
Use it when the user asks to move a tab, put tables in a particular order, or when tables you \
created should read in a sensible sequence (e.g. \"Projects\" before \"Tasks\"). Pass every \
table id of the database exactly once, in the new order; call DescribeDatabase first for the \
current ids, since a list that misses or adds a table is refused.\n\
\n\
Requires edit access to the database. The response is the schema after the change. If \
`database` is null, the reorder still succeeded; call DescribeDatabase using databaseId \
before continuing."
)]
pub struct ReorderTables {
    /// The database whose tables to reorder.
    #[schemars(description = "Id of the database, from ListDatabases.")]
    pub database_id: Uuid,
    /// Every table id, in the new order.
    #[schemars(
        description = "Every table id of the database, exactly once, in the new left-to-right \
                       order, from DescribeDatabase."
    )]
    pub table_ids: Vec<Uuid>,
}

impl ToolAnnotated for ReorderTables {
    const ANNOTATIONS: ToolAnnotations =
        ToolAnnotations::destructive("Reorder tables").with_idempotent();
}

/// Response from the ReorderTables tool.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReorderTablesResponse {
    /// Database whose tables were reordered.
    pub database_id: Uuid,
    /// The table ids in their new order.
    pub table_ids: Vec<Uuid>,
    /// The database's schema after the change.
    pub database: Option<ToolDatabaseSchema>,
    /// A failed follow-up read does not undo the committed order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[async_trait]
impl<S, E> AsyncTool<DatabasesToolContext<S, E>> for ReorderTables
where
    S: DatabasesService,
    E: EntityAccessService,
{
    type Output = ReorderTablesResponse;

    #[tracing::instrument(skip_all, fields(
        user_id = ?request_context.user_id,
        database_id = %self.database_id,
    ), err)]
    async fn call(
        &self,
        service_context: ServiceContext<DatabasesToolContext<S, E>>,
        request_context: RequestContext,
    ) -> ToolResult<Self::Output> {
        let user_id = &request_context.user_id;
        let receipt = service_context
            .edit_receipt(user_id, self.database_id)
            .await?;
        let tables = service_context
            .service
            .reorder_tables(receipt, self.table_ids.clone())
            .await
            .map_err(database_error)?;

        let (database, warning) = service_context
            .schema_after_write(user_id, self.database_id)
            .await;
        Ok(ReorderTablesResponse {
            database_id: self.database_id,
            table_ids: tables.into_iter().map(|table| table.id).collect(),
            database,
            warning,
        })
    }
}
