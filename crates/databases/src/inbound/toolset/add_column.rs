//! AddColumn tool: add a typed column to a table.

use ai_toolset::{
    AsyncTool, RequestContext, ServiceContext, ToolAnnotated, ToolAnnotations, ToolResult,
};
use async_trait::async_trait;
use entity_access::domain::ports::EntityAccessService;
use models_databases::{ColumnId, DatabaseId, DatabaseOp, NewColumn, NewOption, OptionId, TableId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    ColumnType, DatabasesToolContext, SchemaAfterWrite, ToolDatabaseSchema, ToolEntityType,
    WriteWarnings, column_kind,
};
use crate::domain::models::OpBatch;
use crate::domain::ports::DatabasesService;

/// Add a column to a table.
#[derive(Debug, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
#[schemars(
    title = "AddColumn",
    description = "\
Add a column to a table in one of the user's databases. Columns are typed, and the type is \
what makes the data useful later — a `date` column sorts and filters by time, a `number` \
column sums, a `select` column constrains what can be written to it.\n\
\n\
Pick the type from what the values actually are, not from how they were typed at you: \"Going \
/ Maybe / Declined\" is a `select`, not `text`; \"$1,200\" is a `number`; \"Aug 13\" is a \
`date`. Use `text` only when the values really are free-form.\n\
\n\
- `isMultiSelect: true` makes the column hold several values at once. In SQL it is written \
as a list (`['a', 'b']`) and `col HAS 'x'` tests membership.\n\
- `linkToTableId` makes it a **relation column** pointing at another table, so rows on one \
side reference rows on the other by row id. Write it as a list of row ids and join through \
it (`JOIN guests g ON i.guest = g.row_id`). The response's relation metadata gives the \
target table.\n\
- `entity` columns hold references to Macro things (people, documents); say which with \
`specificEntityType`. Their values are typed ids such as `macro|sam@example.com`.\n\
\n\
Select and tag columns take their options as **explicit schema**: pass every label the column \
should accept in `options`. SQL only accepts those labels — a select column created with no \
options accepts nothing — and more can be added later with AddColumnOptions.\n\
\n\
Requires edit access. The response is the table's database schema after the change, including \
the new column's exact `sqlName`. If `database` is null, the column was still created; \
call DescribeDatabase using databaseId before continuing, without repeating AddColumn."
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
                       entity (a reference to a Macro person or document)."
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

    /// What an entity column's ids reference.
    #[schemars(
        description = "Required for dataType entity without linkToTableId, and refused \
                       otherwise: what the ids reference, e.g. USER for a person column or \
                       DOCUMENT."
    )]
    #[serde(default)]
    pub specific_entity_type: Option<ToolEntityType>,

    /// Make this a link column targeting another table.
    #[schemars(
        description = "Id of another table to link to, making this a link column whose rows \
                       reference rows over there. Omit for an ordinary column. The target \
                       table must be one the user can reach."
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
        // A link target in another database is expressible but not something
        // this tool takes: the receipt covers one database, so the relation
        // names the same one.
        let kind = column_kind(
            self.data_type,
            self.is_multi_select,
            self.specific_entity_type,
            self.link_to_table_id
                .map(|table_id| (self.database_id, table_id)),
        )?;
        let column_id = ColumnId::new();
        service_context
            .apply(
                user_id,
                self.database_id,
                OpBatch::from(vec![DatabaseOp::CreateColumn {
                    table: self.table_id,
                    id: column_id,
                    definition: NewColumn::New {
                        name: self.name.clone(),
                        kind,
                        options: self
                            .options
                            .iter()
                            .flatten()
                            .map(|label| NewOption {
                                id: OptionId::new(),
                                label: label.clone(),
                            })
                            .collect(),
                        infer_type: false,
                    },
                    after: None,
                }]),
            )
            .await?;

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
