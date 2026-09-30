//! RenameTable tool: retitle a tab without touching its records or columns.

use ai_toolset::{
    AsyncTool, RequestContext, ServiceContext, ToolAnnotated, ToolAnnotations, ToolCallError,
    ToolResult,
};
use async_trait::async_trait;
use entity_access::domain::ports::EntityAccessService;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{DatabasesToolContext, ToolDatabaseSchema, database_error};
use crate::domain::ports::DatabasesService;

/// Rename a table.
#[derive(Debug, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
#[schemars(
    title = "RenameTable",
    description = "\
Rename a table — what the user sees as a tab — keeping its id, records, and columns.\n\
\n\
Use it to give a new database's starter table (\"Table 1\") the name the user asked for \
instead of creating an extra tab next to it, or when the user asks to rename a tab.\n\
\n\
Requires edit access to the database. The table's `sqlName` is its display name, so use the \
refreshed schema in the response for later statements. If `database` is null, the rename \
still succeeded; call DescribeDatabase using databaseId before continuing."
)]
pub struct RenameTable {
    /// The database containing the table.
    #[schemars(description = "Id of the database containing the table, from ListDatabases.")]
    pub database_id: Uuid,

    /// The table to rename.
    #[schemars(description = "Id of the table to rename, from DescribeDatabase.")]
    pub table_id: Uuid,

    /// New display name.
    #[schemars(
        description = "New display name of the table, as the user would title the tab — e.g. \
                       \"Parties\"."
    )]
    pub name: String,
}

impl ToolAnnotated for RenameTable {
    const ANNOTATIONS: ToolAnnotations =
        ToolAnnotations::destructive("Rename table").with_idempotent();
}

/// Response from the RenameTable tool.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RenameTableResponse {
    /// Database containing the renamed table.
    pub database_id: Uuid,
    /// The renamed table's id, unchanged by the rename.
    pub table_id: Uuid,
    /// The table's display name after the rename.
    pub name: String,
    /// The database's schema after the change.
    pub database: Option<ToolDatabaseSchema>,
    /// A failed follow-up read does not undo the committed rename.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[async_trait]
impl<S, E> AsyncTool<DatabasesToolContext<S, E>> for RenameTable
where
    S: DatabasesService,
    E: EntityAccessService,
{
    type Output = RenameTableResponse;

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
        tracing::info!("Rename table");

        let user_id = &request_context.user_id;

        // The service renames only if the name it is replacing is still
        // current; read that name as the caller sees it now.
        let view_receipt = service_context
            .view_receipt(user_id, self.database_id)
            .await?;
        let detail = service_context
            .service
            .get_database(view_receipt, service_context.viewer(user_id))
            .await
            .map_err(database_error)?;
        let previous_name = detail
            .tables
            .iter()
            .find(|table| table.table.id == self.table_id)
            .map(|table| table.table.name.clone())
            .ok_or_else(|| ToolCallError {
                description: format!(
                    "Database {} has no table with id {}. Call DescribeDatabase for its tables.",
                    self.database_id, self.table_id
                ),
                internal_error: anyhow::anyhow!("rename target table not found"),
            })?;

        let receipt = service_context
            .edit_receipt(user_id, self.database_id)
            .await?;
        let table = service_context
            .service
            .rename_table(receipt, self.table_id, self.name.clone(), previous_name)
            .await
            .map_err(database_error)?;

        let (database, warning) = service_context
            .schema_after_write(user_id, self.database_id)
            .await;

        Ok(RenameTableResponse {
            database_id: self.database_id,
            table_id: table.id,
            name: table.name,
            database,
            warning,
        })
    }
}
