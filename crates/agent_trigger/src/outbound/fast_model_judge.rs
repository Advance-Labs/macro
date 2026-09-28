//! Implicit-trigger judgement backed by the fast agent model.

#[cfg(test)]
mod test;

use std::sync::Arc;

use agent::structured_output::{DynamicSchema, dynamic_structured_completion};
use agent::{Message, PredefinedModel};
use agent_session::domain::error::Result;
use ai_usage::{AiFeature, UsageContext, UsageRecorder};
use bot_id::BotId;
use messages::domain::events::MessagePostedMetadata;
use serde::Deserialize;
use serde_json::json;

use crate::domain::service::{CandidateAgent, ImplicitTriggerJudge};

use super::image_caption::{ImageCaptioner, append_image_blurbs, blurbs_for_attachments};

static SYSTEM_PROMPT: &str = "\
You decide whether a channel message is addressed to one of the AI agents in a \
thread, and if so which one.

The message was posted, without mentioning anyone, in a thread where one or \
more AI agents have open sessions: each agent was asked to do work earlier in \
the thread and posts its progress there. Decide whether this new message is \
directed at one of those agents - a follow-up instruction, question, \
correction, or feedback the agent should act on - or is conversation between \
the people in the thread.

You are told the names of the agents in the thread. Name the agent the message \
is for only when the message reads as something its author expects that agent \
to respond to. When the message names or clearly refers to one agent, pick \
that one; when it follows up on one agent's work, pick that agent. Answer null \
when it is commentary about an agent or its work addressed to other people, \
unrelated discussion, or when you cannot tell which agent it is for.

You are given the thread around the agents' part in it, as lines of \
'[speaker] message' where each agent's own messages are marked \
'[agent <name>]'. Some messages may be hidden; judge on what you are shown.

Attached images are not shown to you. Each one is written into the message \
as <this is an image of ...>, a description of the picture rather than words \
the author typed. Treat that description as what the image shows. \
<this is an image> means a picture was attached but could not be described. \
Those descriptions appear on the message to judge; the thread transcript \
does not repeat them.";

#[derive(Debug, Deserialize)]
struct JudgeOutput {
    addressed_to: Option<String>,
    #[expect(dead_code, reason = "the model reasons better when asked to explain")]
    reason: String,
}

/// Judges implicit triggers with [`PredefinedModel::Fast`], recording token
/// usage against the message's sender.
pub struct FastModelTriggerJudge {
    model: PredefinedModel,
    recorder: Arc<dyn UsageRecorder>,
    images: Arc<dyn ImageCaptioner>,
}

impl FastModelTriggerJudge {
    /// Creates a judge using the fast agent model.
    ///
    /// `images` turns attached pictures into the `<this is an image of ...>`
    /// blurbs the text-only model reads. The forwarded message is unchanged.
    pub fn new(recorder: Arc<dyn UsageRecorder>, images: impl ImageCaptioner + 'static) -> Self {
        Self {
            model: PredefinedModel::Fast,
            recorder,
            images: Arc::new(images),
        }
    }
}

impl ImplicitTriggerJudge for FastModelTriggerJudge {
    async fn addressed_agent(
        &self,
        posted: &MessagePostedMetadata,
        transcript: &str,
        candidates: &[CandidateAgent],
    ) -> Result<Option<BotId>> {
        if candidates.is_empty() {
            return Ok(None);
        }
        let labels: Vec<&str> = candidates
            .iter()
            .map(|candidate| candidate.label.as_str())
            .collect();
        let schema = DynamicSchema {
            name: "ImplicitTriggerJudgeOutput".to_string(),
            description: Some(
                "Judgement for which agent in the thread, if any, a message is addressed to."
                    .to_string(),
            ),
            schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["addressed_to", "reason"],
                "properties": {
                    "addressed_to": {
                        "type": ["string", "null"],
                        "enum": labels.iter().map(|label| json!(label)).chain([json!(null)]).collect::<Vec<_>>(),
                        "description": "The name of the agent the message is directed at, exactly as listed, or null when it is directed at none of them."
                    },
                    "reason": {
                        "type": "string",
                        "description": "A concise reason for the judgement."
                    }
                }
            }),
        };
        // The judge runs on behalf of whoever posted the message, so their
        // account carries the tokens; a non-user sender never reaches this
        // point, but attribute it to the system rather than fail if one does.
        let ctx = match posted.sender.as_user() {
            Some(user) => UsageContext::new(AiFeature::Automation, user.clone()),
            None => UsageContext::system(AiFeature::Automation),
        };

        let blurbs =
            blurbs_for_attachments(self.images.as_ref(), &posted.attachments, ctx.clone()).await;
        let content = append_image_blurbs(&posted.content, &blurbs);
        let prompt = judge_user_prompt(&labels, transcript, &content);

        let value = dynamic_structured_completion(
            self.model,
            SYSTEM_PROMPT,
            vec![Message::user(prompt)],
            schema,
            self.recorder.as_ref(),
            ctx,
        )
        .await?;

        let output: JudgeOutput = serde_json::from_value(value).map_err(|error| {
            anyhow::anyhow!("implicit trigger judge returned malformed output: {error}")
        })?;
        Ok(pick_candidate(candidates, output.addressed_to.as_deref()))
    }
}

/// The candidate the judge named, or `None` for `null` or a label that
/// matches none of them. Matched case-insensitively and ignoring surrounding
/// whitespace: the model is asked for the label verbatim, but a stray change
/// of case should not turn a clear pick into silence.
pub(crate) fn pick_candidate(candidates: &[CandidateAgent], picked: Option<&str>) -> Option<BotId> {
    let picked = picked?.trim();
    candidates
        .iter()
        .find(|candidate| candidate.label.eq_ignore_ascii_case(picked))
        .map(|candidate| candidate.bot_id)
}

/// The user prompt the judge scores. Image blurbs are already part of `content`.
pub(crate) fn judge_user_prompt(labels: &[&str], transcript: &str, content: &str) -> String {
    let agents = labels.join(", ");
    if transcript.is_empty() {
        format!("The agents in this thread: {agents}\n\nThe message to judge:\n{content}")
    } else {
        format!(
            "The agents in this thread: {agents}\n\nThe thread so far:\n{transcript}\nThe message to judge:\n{content}"
        )
    }
}
