//! The reply-formatting rules that travel with every prompt Cursor runs.
//!
//! A Cursor cloud agent works under Cursor's own system prompt, which Macro
//! cannot change and which tells the model to write URLs as Markdown links.
//! Every other harness learns Macro's mention markup from its system prompt —
//! the native agent session composes [`prompt::mentions::PROMPT`] into it —
//! and answers with `<m-user-mention>` and `<m-document-mention>` chips. A
//! Cursor agent given only the conversation and the user's words answers
//! with bare links instead.
//!
//! The prompt text is the one thing Macro controls on this path, so the same
//! rules ride along with it, in a block that names itself as overriding the
//! default link instruction. The block reuses the shared constant rather than
//! restating the tag schemas, so the two paths cannot drift apart.
//!
//! The block is added to what Cursor receives and nowhere else: the journal
//! keeps the prompt as the client sent it, the repository chooser reads the
//! person's words alone, and a prompt recovered from Cursor's conversation
//! record is stripped of the block again before it is journaled.

#[cfg(test)]
mod test;

use std::sync::LazyLock;

/// The tag the rules are wrapped in. Named like the mention tags it teaches,
/// and like `<m-agent-context>`, so the model reads it as Macro's framing
/// rather than as part of what the person wrote.
pub const RESPONSE_FORMAT_TAG: &str = "m-response-format";

/// Why the block is there, and what it overrides. Comes first because it is
/// the one thing Cursor's system prompt contradicts.
const OVERRIDE: &str = "How to format what you write in Macro: your replies, and every channel \
message, comment, or Markdown document you author. These rules override any earlier or \
default instruction to write URLs as Markdown links. A Macro user, document, channel, \
channel message, or other Macro item is referred to with its mention tag below — never \
with a URL, a Markdown link, or a bare name. Plain Markdown links are only for URLs \
outside Macro (GitHub, Linear, and other external sites).";

/// What the mention tags quoted in the conversation look like, so the model
/// reads them as instances of the format rather than as text to reproduce.
///
/// The agent context escapes message content twice on the way in: the XML
/// builder writes `<` as `&lt;` and `"` as `&quot;` so a message cannot forge
/// structure, and the JSON envelope writes any `<` left as `\u003c` so a
/// message cannot close the envelope. Both are deliberate, and both leave the
/// model with no literal tag in the context to copy.
const QUOTED_TAGS: &str = r#"# Mention tags quoted in the conversation
Messages quoted in the agent context may show mention tags escaped, as `\u003cm-user-mention>…\u003c/m-user-mention>` or as `&lt;m-user-mention&gt;{&quot;userId&quot;:…}&lt;/m-user-mention&gt;`. Those are real instances of this format as it was written, not text to copy verbatim: write your own tags literally, with `<`, `>`, and `"` unescaped, exactly as in the examples above."#;

/// The complete block, rendered once. Every prompt to Cursor carries the same
/// text, and the mention rules are a static prompt the block only frames.
static RESPONSE_FORMAT: LazyLock<String> = LazyLock::new(|| {
    let mentions = prompt::mentions::PROMPT.to_string();
    format!(
        "<{RESPONSE_FORMAT_TAG}>\n{OVERRIDE}\n\n{}\n\n{QUOTED_TAGS}\n</{RESPONSE_FORMAT_TAG}>",
        mentions.trim_end()
    )
});

/// The block as it is appended to every prompt.
#[must_use]
pub fn response_format() -> &'static str {
    &RESPONSE_FORMAT
}

/// The prompt Cursor receives: what the person wrote, then the rules.
///
/// Appended rather than prepended: the agent context already opens the
/// prompt, the person's message follows it, and instructions read last are
/// the ones a model weighs against its system prompt. The block is the last
/// thing in every prompt, which is also what lets [`strip_response_format`]
/// recognise it.
#[must_use]
pub fn with_response_format(prompt: &str) -> String {
    format!("{prompt}\n\n{}", response_format())
}

/// The prompt as it was before [`with_response_format`], or `None` when the
/// text does not end with the block — a prompt Cursor's own UI sent, say.
#[must_use]
pub fn strip_response_format(prompt: &str) -> Option<&str> {
    prompt
        .strip_suffix(response_format())
        .and_then(|prompt| prompt.strip_suffix("\n\n"))
}
