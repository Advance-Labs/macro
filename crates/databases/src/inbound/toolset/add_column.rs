//! AddColumn tool: add a typed column to a table.

use ai_toolset::{
    AsyncTool, RequestContext, ServiceContext, ToolAnnotated, ToolAnnotations, ToolResult,
};
use async_trait::async_trait;
use entity_access::domain::ports::EntityAccessService;
use models_databases::{ColumnId, DatabaseId, TableId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    ColumnType, DatabasesToolContext, SchemaAfterWrite, ToolDatabaseSchema, WriteWarnings,
    database_error,
};
use crate::domain::models::{ColumnBinding, ColumnConfig, CreateColumn};
use crate::domain::ports::DatabasesService;

/// Add a column to a table.
#[derive(Debug, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
#[schemars(
    title = "AddColumn",
    description = "\
Add a column to a table in one of the user's databases. Type it by what its values are, not \
by how they were typed at you:\n\
\n\
- a person (host, owner, assignee, attendee, author): a person column, `entity` then \
`USER` as below;\n\
- a Macro document, task, company, call, channel or project: `entity` with that kind;\n\
- a row of another table in this database: a relation, `entity` with `linkToTableId`;\n\
- a status, stage or category (\"Going / Maybe / Declined\"): `select`, with \
`isMultiSelect` or `tag` for several;\n\
- money, counts and scores (\"$1,200\"): `number`; dates (\"Aug 13\"): `date`; yes/no: \
`boolean`; URLs: `link`;\n\
- free text only: `text`. Never text for people or Macro items.\n\
\n\
An `entity` column is created without a kind: give it one straight away, while it is empty, \
with ChangeColumnType (`dataType: entity`, `specificEntityType: USER` for people). A \
relation holds row ids of the target table and is written as a list of them. Select and \
tag columns accept only the labels in `options`, so list every value the data has; add \
more later with AddColumnOptions.\n\
\n\
Requires edit access. The response is the database's schema after the change, with the new \
column's exact `sqlName`. If `database` is null, the column was still created: call \
DescribeDatabase with databaseId before continuing, and do not repeat AddColumn."
)]
pub struct AddColumn {
    /// The database the table belongs to.
    #[schemars(description = "Id of the database the table belongs to, from ListDatabases.")]
    pub database_id: DatabaseId,

    /// The table to add the column to.
    #[schemars(
        description = "Id of the table to add the column to, from DescribeDatabase or \
                       CreateTable. It must belong to databaseId."
    )]
    pub table_id: TableId,

    /// Display name of the new column.
    #[schemars(
        description = "Display name of the column, as the user would head it — e.g. \"Dietary \
                       Needs\". SQL refers to it by this name, quoted."
    )]
    pub name: String,

    /// The value type of the column.
    #[schemars(
        description = "The kind of value the column holds: text, number, boolean, date, link \
                       (a URL), select (a fixed set of text labels), select_number, tag, or \
                       entity (a reference to a Macro person, document, task, or other item)."
    )]
    pub data_type: ColumnType,

    /// Whether the column holds several values at once.
    #[schemars(
        description = "True if a cell can hold several values at once. Defaults to false. \
                       Multi-valued cells are written as lists in SQL; test membership \
                       with `col HAS 'x'`."
    )]
    #[serde(default)]
    pub is_multi_select: bool,

    /// The labels a select or tag column accepts.
    #[schemars(
        description = "For a select, select_number, or tag column, the allowed labels — e.g. \
                       [\"Going\", \"Maybe\", \"Declined\"]. SQL writes and reads these labels \
                       verbatim, and anything else is rejected by the statement, so list every \
                       value the data actually has. A select_number column's labels must be \
                       numbers. Omit for other column types; add more later with \
                       AddColumnOptions."
    )]
    #[serde(default)]
    pub options: Option<Vec<String>>,

    /// Make this a link column targeting another table.
    #[schemars(
        description = "Id of another table of this database, making this a relation whose \
                       cells hold that table's row ids. Omit for any other column."
    )]
    #[serde(default)]
    pub link_to_table_id: Option<TableId>,
}

impl ToolAnnotated for AddColumn {
    const ANNOTATIONS: ToolAnnotations = ToolAnnotations::additive("Add column");
}

/// Response from the AddColumn tool.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AddColumnResponse {
    /// Database containing the committed column.
    pub database_id: DatabaseId,
    /// Table containing the committed column.
    pub table_id: TableId,
    /// The new column placement's id.
    pub column_id: ColumnId,
    /// The database's schema after the change.
    pub database: Option<ToolDatabaseSchema>,
    /// Follow-up guidance if schema refresh failed after the column was saved.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub warning: Option<WriteWarnings>,
}

#[async_trait]
impl<Service, EntityAccess> AsyncTool<DatabasesToolContext<Service, EntityAccess>> for AddColumn
where
    Service: DatabasesService,
    EntityAccess: EntityAccessService,
{
    type Output = AddColumnResponse;

    #[tracing::instrument(skip_all, fields(
        user_id = ?request_context.user_id,
        database_id = %self.database_id,
        table_id = %self.table_id,
        data_type = ?self.data_type,
    ), err)]
    async fn call(
        &self,
        service_context: ServiceContext<DatabasesToolContext<Service, EntityAccess>>,
        request_context: RequestContext,
    ) -> ToolResult<Self::Output> {
        let user_id = &request_context.user_id;
        let receipt = service_context
            .edit_receipt(user_id, self.database_id)
            .await?;

        // A link target in another database is expressible but not something
        // this tool takes: the receipt covers one database, so the config
        // names the same one.
        let config = self.link_to_table_id.map(|table_id| ColumnConfig::Link {
            database_id: self.database_id,
            table_id,
        });

        let column_id = service_context
            .service
            .create_column(
                receipt,
                service_context.viewer(user_id),
                CreateColumn {
                    infer_type: false,
                    table_id: self.table_id,
                    binding: ColumnBinding::NewDefinition {
                        name: self.name.clone(),
                        data_type: self.data_type.into(),
                        is_multi_select: self.is_multi_select,
                        options: self.options.clone().unwrap_or_default(),
                    },
                    config,
                },
            )
            .await
            .map_err(database_error)?;

        let SchemaAfterWrite { database, warning } = service_context
            .schema_after_write(user_id, self.database_id)
            .await;

        Ok(AddColumnResponse {
            column_id,
            database_id: self.database_id,
            table_id: self.table_id,
            database,
            warning,
        })
    }
}
