//! Model availability for chat, gated by the user's plan.
//!
//! Free (non-professional) users may use only [`FREE_MODEL`]; professional
//! users may use every model in [`CHAT_MODELS`].

/// The chat models offered to users, best-first.
///
/// The current Anthropic generation is Sonnet 5.5 and Opus 5.5. Haiku 4.5
/// stays as the fast model; older Sonnet, Opus, and Fable ids are absent.
pub const CHAT_MODELS: &[&str] = &[
    "anthropic/claude-sonnet-5-5",
    "anthropic/claude-opus-5-5",
    "anthropic/claude-haiku-4-5",
    "openai/gpt-6-astra",
    "openai/gpt-5.6",
    "openai/gpt-5.6-mini",
    "openai/gpt-5.5",
    "openai/gpt-5-mini",
    "google/gemini-3.8-flash",
];

/// The default model for professional (paid) users.
pub const PAID_DEFAULT_MODEL: &str = "anthropic/claude-sonnet-5-5";

/// The only model available to free (non-professional) users.
pub const FREE_MODEL: &str = "google/gemini-3.8-flash";

/// Model a paid composer opens on when the user has never picked one.
///
/// [`PAID_DEFAULT_MODEL`] stays the catalog default for sessions that already
/// have one. This is only the landing model for a composer with no saved pick.
pub const UPGRADE_MODEL: &str = "anthropic/claude-opus-5-5";

/// In-memory composer models that are not part of [`CHAT_MODELS`].
///
/// Kept in step with `agent_inmem`'s routed catalog. Gemini is already in
/// [`CHAT_MODELS`], so it is not repeated here.
pub const EXTRA_COMPOSER_MODELS: &[&str] = &[
    "fireworks/kimi-k3",
    "fireworks/deepseek-v4-pro-0813",
    "fireworks/muse-glimmer-30b",
    "fireworks/glm-5p3",
    "fireworks/glm-5p3-flash",
    "fireworks/qwen3p8-max",
    "fireworks/minimax-m3",
    "cerebras/gpt-oss-120b",
    "fireworks/nemotron-lightning-3p5-30b-a3b",
];

/// Whether `model_id` is a model the composer can remember.
#[must_use]
pub fn is_composer_model(model_id: &str) -> bool {
    CHAT_MODELS.contains(&model_id) || EXTRA_COMPOSER_MODELS.contains(&model_id)
}
