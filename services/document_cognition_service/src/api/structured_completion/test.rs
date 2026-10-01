use super::activity::{DatabaseChange, ToolOutcome};
use super::*;
use serde_json::json;

#[test]
fn completion_failure_after_committed_changes_is_interrupted() {
    let response = partial_completion_or_error(
        vec![StructuredToolActivity {
            name: "AddColumn".into(),
            outcome: ToolOutcome::Succeeded {
                changes: DatabaseChange::Schema,
            },
        }],
        StructuredCompletionError {
            error: "provider interrupted".into(),
            status: StatusCode::BAD_GATEWAY,
            code: None,
        },
    )
    .unwrap()
    .0;
    assert_eq!(
        serde_json::to_value(&response).unwrap(),
        json!({
            "outcome": {"status": "interrupted", "reason": "provider interrupted"},
            "toolActivity": [
                {
                    "name": "AddColumn",
                    "outcome": {"status": "succeeded", "changes": {"kind": "schema"}},
                },
            ],
        })
    );
}

#[test]
fn completion_failure_without_changes_preserves_error_status() {
    let error = partial_completion_or_error(
        vec![StructuredToolActivity {
            name: "QueryDatabase".into(),
            outcome: ToolOutcome::Succeeded {
                changes: DatabaseChange::None,
            },
        }],
        StructuredCompletionError {
            error: "provider interrupted".into(),
            status: StatusCode::BAD_GATEWAY,
            code: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.status, StatusCode::BAD_GATEWAY);
    assert_eq!(error.error, "provider interrupted");
}

#[test]
fn completed_answers_carry_the_caller_schema_result() {
    let response = StructuredCompletionResponse {
        outcome: StructuredCompletionOutcome::Completed {
            result: json!({"answerable": true, "sql": "SELECT * FROM \"Guests\""}),
        },
        tool_activity: vec![StructuredToolActivity {
            name: "QueryDatabase".into(),
            outcome: ToolOutcome::Succeeded {
                changes: DatabaseChange::Rows { count: 2 },
            },
        }],
    };
    assert_eq!(
        serde_json::to_value(&response).unwrap(),
        json!({
            "outcome": {
                "status": "completed",
                "result": {"answerable": true, "sql": "SELECT * FROM \"Guests\""},
            },
            "toolActivity": [
                {
                    "name": "QueryDatabase",
                    "outcome": {"status": "succeeded", "changes": {"kind": "rows", "count": 2}},
                },
            ],
        })
    );
}
