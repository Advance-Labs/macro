//! Small execution receipts from the agent stream, never inferred from model text.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use agent::types::AssistantMessagePart;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[cfg(test)]
mod test;

#[derive(Debug, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StructuredToolActivity {
    pub name: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes_applied: Option<u64>,
}

pub(super) fn tool_activity(parts: &[AssistantMessagePart]) -> Vec<StructuredToolActivity> {
    parts
        .iter()
        .filter_map(|part| match part {
            AssistantMessagePart::ToolCallResponseJson { name, json, .. } => {
                Some(StructuredToolActivity {
                    name: name.clone(),
                    success: true,
                    changes_applied: json
                        .get("changesApplied")
                        .and_then(serde_json::Value::as_u64),
                })
            }
            AssistantMessagePart::ToolCallErr { name, .. } => Some(StructuredToolActivity {
                name: name.clone(),
                success: false,
                changes_applied: None,
            }),
            _ => None,
        })
        .collect()
}

/// Saving a question writes no table, so it is not a database change.
const SAVE_DATABASE_QUERY_TOOL: &str = "SaveDatabaseQuery";
/// SQL that changes rows reports how many through `changesApplied`.
const QUERY_DATABASE_TOOL: &str = "QueryDatabase";

/// The database tools whose success always changes a database: every database
/// tool outside the read-only set, except saving a question.
fn database_mutation_tools() -> &'static BTreeSet<String> {
    static TOOLS: LazyLock<BTreeSet<String>> = LazyLock::new(|| {
        let read_only = ai_tools::database_read_only_tools();
        ai_tools::database_tools()
            .tools
            .into_keys()
            .filter(|name| !read_only.tools.contains_key(name) && name != SAVE_DATABASE_QUERY_TOOL)
            .collect()
    });
    &TOOLS
}

pub(super) fn has_database_changes(activity: &[StructuredToolActivity]) -> bool {
    activity.iter().any(|entry| {
        entry.success
            && (database_mutation_tools().contains(&entry.name)
                || (entry.name == QUERY_DATABASE_TOOL
                    && entry.changes_applied.is_some_and(|count| count > 0)))
    })
}
