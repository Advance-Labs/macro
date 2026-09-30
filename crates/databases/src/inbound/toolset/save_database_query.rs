//! SaveDatabaseQuery tool: save a question and hand back the live block that
//! renders its answer wherever it is pasted.

use std::collections::HashSet;

use ai_toolset::{
    AsyncTool, RequestContext, ServiceContext, ToolAnnotated, ToolAnnotations, ToolCallError,
    ToolResult,
};
use async_trait::async_trait;
use entity_access::domain::ports::EntityAccessService;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{DatabasesToolContext, QueryDatabaseDisplay, query_error, sql_guide, viewer_of};
use crate::domain::models::QueryDefinition;
use crate::domain::ports::DatabasesService;

/// Most series one chart plots, matching what the document node accepts.
const MAX_CHART_SERIES: usize = 5;

/// Save a question as a live query block.
#[derive(Debug, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
#[schemars(
    title = "SaveDatabaseQuery",
    description = concat!(
        "\
Save a read-only SELECT as a live question and get back the block that shows its answer. \
Paste the returned `markdown` verbatim — into your reply, or into a document with \
CreateDocument/EditDocument — and it renders as a live number, table, or chart that re-runs \
for whoever views it, with their permissions, so it stays current as the data changes.\n\
\n\
Use it whenever the user asks a question about their data or asks for a chart. Run the SELECT \
with QueryDatabase first to check it returns what you expect, then save exactly that SQL.\n\
\n\
- `displayMode`: `scalar` for one number (a single COUNT/SUM/AVG), `table` for rows, `bar` to \
compare categories, `line` for a trend over an ordered column, `pie` for shares of a whole.\n\
- `chart` (bar/line/pie): `x` is the label column and `y` the numeric result columns, named \
exactly as the result columns are — alias aggregates (`COUNT(*) AS invites`) so they have \
stable names.\n\
- Pass `databaseId` so the question resolves against that database's tables.\n\
\n\
Saved questions never change. To change one, save a new one and use its new block.\n\
\n\
## Dialect\n\
\n",
        sql_guide!(),
    )
)]
pub struct SaveDatabaseQuery {
    /// The database the question is about.
    #[schemars(
        description = "Id of the database the question is about, from ListDatabases. Its \
                       tables win when another database has a table of the same name."
    )]
    #[serde(default)]
    pub database_id: Option<Uuid>,
    /// The SELECT to save.
    #[schemars(description = "The SELECT to save, exactly as it ran with QueryDatabase.")]
    pub sql: String,
    /// Heading shown above the answer.
    #[schemars(description = "Short heading shown with the answer, e.g. \"Invites per party\".")]
    pub title: String,
    /// How the answer is shown.
    pub display_mode: QueryDatabaseDisplay,
    /// Chart configuration for bar, line, and pie.
    #[serde(default)]
    pub chart: Option<ToolChart>,
    /// The question in the user's words.
    #[schemars(
        description = "The question as the user asked it. Defaults to the title; shown when \
                       someone edits the question."
    )]
    #[serde(default)]
    pub prompt: Option<String>,
}

/// Which result columns a chart plots.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolChart {
    /// The label column.
    #[schemars(description = "Result column holding the labels (the x axis or pie slices).")]
    pub x: String,
    /// The value columns.
    #[schemars(description = "One to five numeric result columns to plot, none equal to x.")]
    pub y: Vec<String>,
    /// Chart heading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl ToolAnnotated for SaveDatabaseQuery {
    // It stores a question, never data, and every save is a new row.
    const ANNOTATIONS: ToolAnnotations = ToolAnnotations::additive("Save database query");
}

/// Response from the SaveDatabaseQuery tool.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SaveDatabaseQueryResponse {
    /// The saved question's id.
    pub query_id: Uuid,
    /// The block to paste verbatim where the answer should appear.
    pub markdown: String,
}

/// The document node's payload, in the key order the node writes it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryBlock<'a> {
    query_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    database_id: Option<Uuid>,
    title: &'a str,
    prompt: &'a str,
    display_mode: QueryDatabaseDisplay,
    #[serde(skip_serializing_if = "Option::is_none")]
    chart: Option<&'a ToolChart>,
}

/// The block's markdown: compact JSON between the node's tags.
fn query_block_markdown(
    query_id: Uuid,
    database_id: Option<Uuid>,
    title: &str,
    prompt: &str,
    display_mode: QueryDatabaseDisplay,
    chart: Option<&ToolChart>,
) -> String {
    let json = serde_json::to_string(&QueryBlock {
        query_id,
        database_id,
        title,
        prompt,
        display_mode,
        chart,
    })
    .expect("a query block always serializes");
    // `<` only occurs inside strings; escaping it keeps a title such as
    // "</m-db-query>" from closing the tag early.
    format!("<m-db-query>{}</m-db-query>", json.replace('<', "\\u003c"))
}

fn invalid(description: impl Into<String>) -> ToolCallError {
    let description = description.into();
    ToolCallError {
        internal_error: anyhow::anyhow!(description.clone()),
        description,
    }
}

fn validate_chart(chart: &ToolChart) -> Result<(), ToolCallError> {
    if chart.x.trim().is_empty() {
        return Err(invalid(
            "chart.x must name the result column holding the labels.",
        ));
    }
    if chart.y.is_empty() || chart.y.len() > MAX_CHART_SERIES {
        return Err(invalid(format!(
            "chart.y must name one to {MAX_CHART_SERIES} numeric result columns."
        )));
    }
    if chart.y.iter().any(|name| name.trim().is_empty()) {
        return Err(invalid("chart.y must not contain an empty column name."));
    }
    if chart.y.iter().collect::<HashSet<_>>().len() != chart.y.len() {
        return Err(invalid("chart.y must not name a column twice."));
    }
    if chart.y.contains(&chart.x) {
        return Err(invalid(
            "chart.y must not include the label column chart.x.",
        ));
    }
    Ok(())
}

#[async_trait]
impl<S, E> AsyncTool<DatabasesToolContext<S, E>> for SaveDatabaseQuery
where
    S: DatabasesService,
    E: EntityAccessService,
{
    type Output = SaveDatabaseQueryResponse;

    #[tracing::instrument(skip_all, fields(
        user_id = ?request_context.user_id,
        database_id = ?self.database_id,
        display_mode = ?self.display_mode,
    ), err)]
    async fn call(
        &self,
        service_context: ServiceContext<DatabasesToolContext<S, E>>,
        request_context: RequestContext,
    ) -> ToolResult<Self::Output> {
        let title = self.title.trim();
        if title.is_empty() {
            return Err(invalid("title must not be empty."));
        }
        if let Some(chart) = &self.chart {
            validate_chart(chart)?;
        }
        let prompt = self
            .prompt
            .as_deref()
            .map(str::trim)
            .filter(|prompt| !prompt.is_empty())
            .unwrap_or(title);

        let saved = service_context
            .service
            .save_query(
                viewer_of(&request_context.user_id),
                self.database_id,
                QueryDefinition::V1 {
                    query: self.sql.clone(),
                },
            )
            .await
            .map_err(query_error)?;

        Ok(SaveDatabaseQueryResponse {
            query_id: saved.id,
            markdown: query_block_markdown(
                saved.id,
                saved.database_id,
                title,
                prompt,
                self.display_mode,
                self.chart.as_ref(),
            ),
        })
    }
}
