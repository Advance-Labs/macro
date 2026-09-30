//! ChangeColumnType tool: retype a column, converting its values.

use ai_toolset::{
    AsyncTool, RequestContext, ServiceContext, ToolAnnotated, ToolAnnotations, ToolCallError,
    ToolResult,
};
use async_trait::async_trait;
use entity_access::domain::ports::EntityAccessService;
use models_properties::shared::DataType;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    ColumnType, DatabasesToolContext, ToolDatabaseSchema, ToolEntityType, column_of,
    database_error, table_of, viewer_of,
};
use crate::domain::models::{AddColumnOptions, ChangeColumnType as ChangeColumnTypeCommand};
use crate::domain::ports::DatabasesService;

/// Change a column's type.
#[derive(Debug, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
#[schemars(
    title = "ChangeColumnType",
    description = "\
Change a column's type, converting every existing value. The column keeps its id and name.\n\
\n\
Conversion is all or nothing: if any value cannot become the new type without losing \
information (\"soon\" as a number), nothing changes and the error says why. Converting to \
`select` or `tag` turns the distinct existing values into the column's options; pass \
`options` to add labels no row has yet.\n\
\n\
- `entity` needs `specificEntityType` (e.g. `USER` for people, `DOCUMENT`).\n\
- `linkToTableId` makes it a relation to rows of another table of this database; pass \
`dataType: entity` with it. Relations are always multi-valued, and the column must be empty.\n\
- `tag` columns are always multi-valued.\n\
\n\
Requires edit access. The response is the schema after the change."
)]
pub struct ChangeColumnType {
    /// The database containing the column.
    #[schemars(description = "Id of the database containing the column, from ListDatabases.")]
    pub database_id: Uuid,
    /// The table containing the column.
    #[schemars(description = "Id of the table containing the column, from DescribeDatabase.")]
    pub table_id: Uuid,
    /// The column to change.
    #[schemars(description = "Id of the column to change, from DescribeDatabase.")]
    pub column_id: Uuid,
    /// The new value type.
    #[schemars(
        description = "The new type: text, number, boolean, date, link (a URL), select, \
                       select_number, tag, or entity."
    )]
    pub data_type: ColumnType,
    /// Whether a cell may hold several values.
    #[schemars(
        description = "True if a cell can hold several values (select, select_number, entity, \
                       link). Defaults to false."
    )]
    #[serde(default)]
    pub is_multi_select: bool,
    /// Extra labels for a select or tag column.
    #[schemars(
        description = "For select, select_number, or tag: labels to accept beyond the values \
                       the rows already have. Omit for other types."
    )]
    #[serde(default)]
    pub options: Option<Vec<String>>,
    /// Entity kind for an entity column.
    #[schemars(
        description = "Required for dataType entity without linkToTableId: what the ids \
                       reference, e.g. USER or DOCUMENT."
    )]
    #[serde(default)]
    pub specific_entity_type: Option<ToolEntityType>,
    /// Relation target.
    #[schemars(
        description = "Id of a table of this database to relate to; makes the column a \
                       relation holding row ids. Requires dataType entity."
    )]
    #[serde(default)]
    pub link_to_table_id: Option<Uuid>,
}

impl ToolAnnotated for ChangeColumnType {
    const ANNOTATIONS: ToolAnnotations = ToolAnnotations::destructive("Change column type");
}

/// Response from the ChangeColumnType tool.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChangeColumnTypeResponse {
    /// Database containing the column.
    pub database_id: Uuid,
    /// Table containing the column.
    pub table_id: Uuid,
    /// The changed column's id, unchanged by the conversion.
    pub column_id: Uuid,
    /// The database's schema after the change.
    pub database: Option<ToolDatabaseSchema>,
    /// Follow-up guidance if part of the change or the schema refresh failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[async_trait]
impl<S, E> AsyncTool<DatabasesToolContext<S, E>> for ChangeColumnType
where
    S: DatabasesService,
    E: EntityAccessService,
{
    type Output = ChangeColumnTypeResponse;

    #[tracing::instrument(skip_all, fields(
        user_id = ?request_context.user_id,
        database_id = %self.database_id,
        table_id = %self.table_id,
        column_id = %self.column_id,
        data_type = ?self.data_type,
    ), err)]
    async fn call(
        &self,
        service_context: ServiceContext<DatabasesToolContext<S, E>>,
        request_context: RequestContext,
    ) -> ToolResult<Self::Output> {
        if self.link_to_table_id.is_some() && self.data_type != ColumnType::Entity {
            return Err(ToolCallError {
                description: "A relation column has dataType `entity`; pass it with \
                              linkToTableId."
                    .into(),
                internal_error: anyhow::anyhow!("relation requested with a non-entity type"),
            });
        }
        let user_id = &request_context.user_id;
        let detail = service_context
            .current_schema(user_id, self.database_id)
            .await?;
        let table = table_of(&detail, self.table_id)?;
        column_of(table, self.column_id)?;
        let base_version = table.table.version;

        let data_type = DataType::from(self.data_type);
        let receipt = service_context
            .edit_receipt(user_id, self.database_id)
            .await?;
        service_context
            .service
            .change_column_type(
                receipt,
                viewer_of(user_id),
                ChangeColumnTypeCommand {
                    table_id: self.table_id,
                    column_id: self.column_id,
                    data_type,
                    is_multi_select: self.is_multi_select
                        || self.link_to_table_id.is_some()
                        || data_type == DataType::Tag,
                    specific_entity_type: self.specific_entity_type.map(Into::into),
                    relation: self
                        .link_to_table_id
                        .map(|table_id| (self.database_id, table_id)),
                    base_version,
                },
            )
            .await
            .map_err(database_error)?;

        // The type change committed; a failure adding options is reported
        // alongside it rather than as a failed conversion.
        let mut warnings = Vec::new();
        if let Some(labels) = self.options.as_ref().filter(|labels| !labels.is_empty()) {
            let added = async {
                let receipt = service_context
                    .edit_receipt(user_id, self.database_id)
                    .await?;
                service_context
                    .service
                    .add_column_options(
                        receipt,
                        viewer_of(user_id),
                        AddColumnOptions {
                            table_id: self.table_id,
                            column_id: self.column_id,
                            labels: labels.clone(),
                        },
                    )
                    .await
                    .map_err(database_error)
            }
            .await;
            if let Err(error) = added {
                warnings.push(format!(
                    "The type changed, but the extra options were not added: {} Retry them \
                     with AddColumnOptions.",
                    error.description
                ));
            }
        }

        let (database, refresh_warning) = service_context
            .schema_after_write(user_id, self.database_id)
            .await;
        warnings.extend(refresh_warning);
        Ok(ChangeColumnTypeResponse {
            database_id: self.database_id,
            table_id: self.table_id,
            column_id: self.column_id,
            database,
            warning: (!warnings.is_empty()).then(|| warnings.join(" ")),
        })
    }
}
