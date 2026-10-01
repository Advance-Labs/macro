use crate::api::context::{ApiContext, DcsAuthorizationService, DcsChatModelAccess};
use crate::api::tool_selection::{choose_tools_prompt, service_tools};
use crate::model::stream::ToolSet;
use agent::structured_output::DynamicSchema;
use agent::types::{ChatMessage, ChatMessageContent, Role};
use agent::{AgentLoop, StreamAccumulator};
use ai_billing::BillingService;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use futures::StreamExt;
use macro_authorization::{MacroAuthorizationExtractor, UserOrInternal};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;
use utoipa::ToSchema;

mod activity;
#[cfg(test)]
mod test;
pub use activity::StructuredToolActivity;
use activity::{has_database_changes, tool_activity};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StructuredCompletionRequest {
    pub prompt: String,
    pub model: String,
    pub output_schema: DynamicSchema,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_instructions: Option<String>,
    #[serde(default)]
    pub toolset: ToolSet,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StructuredCompletionResponse {
    pub outcome: StructuredCompletionOutcome,
    /// Actual completed tools, independent of the model's claims.
    pub tool_activity: Vec<StructuredToolActivity>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum StructuredCompletionOutcome {
    /// The answer, shaped by the request's `output_schema`.
    Completed { result: serde_json::Value },
    /// The model failed after committing database changes, so there is no
    /// answer but `tool_activity` says what was saved.
    Interrupted { reason: String },
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StructuredCompletionError {
    pub error: String,
    #[serde(skip)]
    pub status: StatusCode,
    /// Stable machine-readable code for payment-required errors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl fmt::Display for StructuredCompletionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)
    }
}

impl IntoResponse for StructuredCompletionError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(self)).into_response()
    }
}

#[utoipa::path(
    post,
    path = "/structured-completion",
    request_body = StructuredCompletionRequest,
    responses(
        (status = 200, description = "Structured completion result", body = StructuredCompletionResponse),
        (status = 400, description = "Bad request", body = StructuredCompletionError),
        (status = 401, description = "Unauthorized"),
        (status = 402, description = "Payment required"),
        (status = 403, description = "No access to the requested model", body = StructuredCompletionError),
        (status = 500, description = "Internal error", body = StructuredCompletionError),
    )
)]
#[tracing::instrument(skip(state, model_access, user, request), fields(user_id = %user.authorization.user.macro_user_id), err)]
pub async fn structured_completion(
    State(state): State<ApiContext>,
    model_access: DcsChatModelAccess,
    user: MacroAuthorizationExtractor<DcsAuthorizationService, UserOrInternal>,
    Json(request): Json<StructuredCompletionRequest>,
) -> Result<Json<StructuredCompletionResponse>, StructuredCompletionError> {
    let ctx = Arc::new(state);
    if !model_access.has_access(&request.model) {
        return Err(StructuredCompletionError {
            error: format!("No access to model {}", request.model),
            status: StatusCode::FORBIDDEN,
            code: None,
        });
    }
    let model = request.model.clone();

    let user_id = user.authorization.user.macro_user_id.clone();

    // Paid users draw on a monthly AI allowance (then credits, then overage).
    // A gate failure is logged and lets the request through.
    if model_access.professional() {
        match ctx.ai_billing.check_allowance(&user_id).await {
            Ok(ai_billing::AllowanceDecision::Allow) => {}
            Ok(ai_billing::AllowanceDecision::Deny(reason)) => {
                return Err(StructuredCompletionError {
                    error: reason.message().to_string(),
                    status: StatusCode::PAYMENT_REQUIRED,
                    code: Some(reason.code().to_string()),
                });
            }
            Err(e) => {
                tracing::error!(error = ?e, user_id = %user_id, "ai billing gate failed; allowing request");
            }
        }
    }

    let tools_prompt = choose_tools_prompt(&request.toolset, &*ctx.all_tools_prompt);

    let system_prompt = match &request.additional_instructions {
        Some(instructions) => format!("{}\n{}", tools_prompt, instructions),
        None => tools_prompt.to_string(),
    };

    // Tool-free completions (for example, query proposals) must not discover
    // connectors or execute built-in tools. Omitting the tool prompt alone
    // does not remove the agent's capabilities.
    let toolset = service_tools(
        &request.toolset,
        ctx.all_tools.clone(),
        &ctx.mcp_selector,
        &user_id,
    )
    .await;

    let user_message = ChatMessage {
        role: Role::User,
        content: ChatMessageContent::Text(request.prompt.clone()),
        attachments: None,
    };
    let rig_messages = agent::to_rig_messages(&[user_message]);

    let agent_loop = AgentLoop::new(ctx.tool_service_context.recorder.clone()).with_model(&model);
    let usage_ctx =
        ai_usage::UsageContext::new(ai_usage::AiFeature::DynamicCompletionsApi, user_id.clone());
    // Carry the feature on the context so tool-spawned subagents attribute to it.
    let mut tool_context = ctx.tool_service_context.clone();
    tool_context.usage_context = usage_ctx.clone();
    let mut session = agent_loop
        .session(toolset, Arc::new(tool_context), &system_prompt, usage_ctx)
        .await;

    let mut ai_stream =
        session
            .send_message(rig_messages)
            .await
            .map_err(|e| StructuredCompletionError {
                error: format!("Agent loop failed: {e}"),
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: None,
            })?;

    let mut accumulator = StreamAccumulator::new();
    let mut stream_error = None;
    while let Some(item) = ai_stream.next().await {
        match item {
            Ok(part) => {
                accumulator.push(part);
            }
            Err(e) => {
                stream_error = Some(StructuredCompletionError {
                    error: format!("Agent loop error: {e}"),
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    code: None,
                });
                break;
            }
        }
    }
    drop(ai_stream);
    let yielded_parts = accumulator.into_parts();
    let activity = tool_activity(&yielded_parts);
    if let Some(error) = stream_error {
        return partial_completion_or_error(activity, error);
    }

    // Phase 2: Structured completion with the gathered context
    let conversation: Vec<ChatMessage> = vec![
        ChatMessage {
            role: Role::User,
            content: ChatMessageContent::Text(request.prompt),
            attachments: None,
        },
        ChatMessage {
            role: Role::Assistant,
            content: ChatMessageContent::AssistantMessageParts(yielded_parts),
            attachments: None,
        },
        ChatMessage {
            role: Role::User,
            content: ChatMessageContent::Text(
                "Based on the information gathered above, produce a structured response matching the required schema.".to_string(),
            ),
            attachments: None,
        },
    ];
    let rig_messages = agent::to_rig_messages(&conversation);

    let result = agent::structured_output::dynamic_structured_completion(
        model,
        &system_prompt,
        rig_messages,
        request.output_schema,
        ctx.tool_service_context.recorder.as_ref(),
        ai_usage::UsageContext::new(ai_usage::AiFeature::DynamicCompletionsApi, user_id),
    )
    .await;
    match result {
        Ok(result) => Ok(Json(StructuredCompletionResponse {
            outcome: StructuredCompletionOutcome::Completed { result },
            tool_activity: activity,
        })),
        Err(error) => partial_completion_or_error(
            activity,
            StructuredCompletionError {
                error: format!("Structured completion failed: {error}"),
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: None,
            },
        ),
    }
}

/// Do not conceal committed edits behind a failed follow-up model request.
fn partial_completion_or_error(
    tool_activity: Vec<StructuredToolActivity>,
    error: StructuredCompletionError,
) -> Result<Json<StructuredCompletionResponse>, StructuredCompletionError> {
    if !has_database_changes(&tool_activity) {
        return Err(error);
    }
    tracing::warn!(error = %error, "Completion interrupted after database changes");
    Ok(Json(StructuredCompletionResponse {
        outcome: StructuredCompletionOutcome::Interrupted {
            reason: error.error,
        },
        tool_activity,
    }))
}
