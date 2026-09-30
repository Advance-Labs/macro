//! RenameColumn tool: relabel a column, keeping its values and id.

use ai_toolset::{
    AsyncTool, RequestContext, ServiceContext, ToolAnnotated, ToolAnnotations, ToolResult,
};
use async_trait::async_trait;
use entity_access::domain::ports::EntityAccessService;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    DatabasesToolContext, ToolDatabaseSchema, column_label, column_of, database_error, table_of,
};
use crate::domain::ports::DatabasesService;

/// Rename a column.
#[derive(Debug, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
#[schemars(
    title = "RenameColumn",
    description = "\
Rename a column, keeping its id, type, and values. SQL refers to the column by its new name \
afterwards, so use the refreshed schema in the response for later statements.\n\
\n\
Requires edit access to the database. If `database` is null, the rename still succeeded; \
call DescribeDatabase using databaseId before continuing."
)]
pub struct RenameColumn {
    /// The database containing the column.
    #[schemars(description = "Id of the database containing the column, from ListDatabases.")]
    pub database_id: Uuid,
    /// The table containing the column.
    #[schemars(description = "Id of the table containing the column, from DescribeDatabase.")]
    pub table_id: Uuid,
    /// The column to rename.
    #[schemars(description = "Id of the column to rename, from DescribeDatabase.")]
    pub column_id: Uuid,
    /// New display name.
    #[schemars(description = "New display name of the column, e.g. \"Dietary Needs\".")]
    pub name: String,
}

impl ToolAnnotated for RenameColumn {
    const ANNOTATIONS: ToolAnnotations =
        ToolAnnotations::destructive("Rename column").with_idempotent();
}

/// Response from the RenameColumn tool.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RenameColumnResponse {
    /// Database containing the column.
    pub database_id: Uuid,
    /// Table containing the column.
    pub table_id: Uuid,
    /// The renamed column's id, unchanged by the rename.
    pub column_id: Uuid,
    /// The column's display name after the rename.
    pub name: String,
    /// The database's schema after the change.
    pub database: Option<ToolDatabaseSchema>,
    /// A failed follow-up read does not undo the committed rename.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[async_trait]
impl<S, E> AsyncTool<DatabasesToolContext<S, E>> for RenameColumn
where
    S: DatabasesService,
    E: EntityAccessService,
{
    type Output = RenameColumnResponse;

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
        // The service renames only if the label it replaces is still current.
        let detail = service_context
            .current_schema(user_id, self.database_id)
            .await?;
        let previous_name = column_label(column_of(
            table_of(&detail, self.table_id)?,
            self.column_id,
        )?);

        let receipt = service_context
            .edit_receipt(user_id, self.database_id)
            .await?;
        let outcome = service_context
            .service
            .rename_column(
                receipt,
                self.table_id,
                self.column_id,
                self.name.clone(),
                previous_name,
            )
            .await
            .map_err(database_error)?;

        let (database, warning) = service_context
            .schema_after_write(user_id, self.database_id)
            .await;
        Ok(RenameColumnResponse {
            database_id: self.database_id,
            table_id: self.table_id,
            column_id: outcome.column.id,
            // The service stores the name trimmed.
            name: self.name.trim().to_string(),
            database,
            warning,
        })
    }
}
