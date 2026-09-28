use super::*;

const PROMPT: &str = "<m-agent-context>{\"version\":1,\"text\":\"\\u003cconversation>…\"}</m-agent-context>\n\n\
    <m-user-mention>{\"userId\":\"bot|c5c5\",\"email\":\"Cursor\",\"displayName\":\"Cursor\"}</m-user-mention> \
    link the plan doc";

/// The person's words come first and unchanged, the rules last, and the
/// block appears exactly once.
#[test]
fn the_rules_follow_the_prompt_in_one_block() {
    let sent = with_response_format(PROMPT);

    assert!(
        sent.starts_with(PROMPT),
        "the prompt is kept verbatim: {sent}"
    );
    assert!(sent.ends_with(&format!("</{RESPONSE_FORMAT_TAG}>")));
    assert_eq!(sent.matches(&format!("<{RESPONSE_FORMAT_TAG}>")).count(), 1);
    assert_eq!(
        sent.matches(&format!("</{RESPONSE_FORMAT_TAG}>")).count(),
        1
    );
}

/// The tag schemas are the native agent session's, verbatim — one constant,
/// so the two paths cannot drift.
#[test]
fn the_rules_are_the_shared_mention_prompt() {
    let block = response_format();

    assert!(
        block.contains(prompt::mentions::PROMPT.instructions.trim_end()),
        "the shared mention instructions are included whole"
    );
    assert!(block.contains(&format!("# {}", prompt::mentions::PROMPT.title)));
    for schema in [
        r#"<m-user-mention>{"userId":"{id}","email":"{email}"}</m-user-mention>"#,
        r#"<m-document-mention>{"documentId":"{id}","documentName":"","blockName":"md","blockParams":{}}</m-document-mention>"#,
        r#"<m-document-mention>{"documentId":"{channel_id}","documentName":"","blockName":"channel","blockParams":{"channel_message_id":"{message_id}"}}</m-document-mention>"#,
    ] {
        assert!(block.contains(schema), "missing {schema} in {block}");
    }
}

/// Cursor's own system prompt says to write URLs as Markdown links; the block
/// has to say plainly that it overrides that, and where links still belong.
#[test]
fn the_rules_override_the_default_link_instruction() {
    let block = response_format();

    assert!(
        block.contains(
            "override any earlier or default instruction to write URLs as Markdown links"
        )
    );
    assert!(block.contains("never with a URL, a Markdown link, or a bare name"));
    assert!(block.contains("Plain Markdown links are only for URLs outside Macro (GitHub, Linear"));
}

/// Quoted messages reach the model with their tags escaped two ways; the
/// block names both forms so they read as instances, not as text to copy.
#[test]
fn the_rules_explain_the_escaped_tags_in_the_context() {
    let block = response_format();

    assert!(block.contains(r"\u003cm-user-mention>"));
    assert!(block.contains("&lt;m-user-mention&gt;{&quot;userId&quot;"));
    assert!(block.contains("write your own tags literally"));
}

#[test]
fn stripping_the_block_gives_the_prompt_back() {
    assert_eq!(
        strip_response_format(&with_response_format(PROMPT)),
        Some(PROMPT)
    );
    assert_eq!(strip_response_format(&with_response_format("")), Some(""));
}

/// A prompt Cursor's own UI sent never carried the block, and a prompt that
/// merely mentions the tag is not wrapped in it.
#[test]
fn a_prompt_without_the_block_is_not_stripped() {
    assert_eq!(strip_response_format("what was asked on cursor.com"), None);
    assert_eq!(
        strip_response_format(&format!("about <{RESPONSE_FORMAT_TAG}>")),
        None
    );
    assert_eq!(
        strip_response_format(&format!("{}{}", PROMPT, response_format())),
        None,
        "the block is only recognised behind its separator"
    );
}
