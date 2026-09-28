use super::*;

fn candidate(bot_id: BotId, label: &str) -> CandidateAgent {
    CandidateAgent {
        bot_id,
        label: label.to_owned(),
    }
}

#[test]
fn the_judge_is_told_what_an_image_blurb_means() {
    assert!(SYSTEM_PROMPT.contains("<this is an image of ...>"));
    assert!(SYSTEM_PROMPT.contains("<this is an image>"));
}

#[test]
fn the_judge_is_told_how_agents_are_labelled() {
    assert!(SYSTEM_PROMPT.contains("'[agent <name>]'"));
}

#[test]
fn an_image_blurb_is_part_of_the_message_being_judged() {
    let content = "see this\n<this is an image of a frog>";
    assert_eq!(
        judge_user_prompt(&["Cursor"], "", content),
        "The agents in this thread: Cursor\n\n\
         The message to judge:\nsee this\n<this is an image of a frog>"
    );
    assert_eq!(
        judge_user_prompt(&["Cursor", "Macro"], "[agent Cursor] on it\n", "see this"),
        "The agents in this thread: Cursor, Macro\n\n\
         The thread so far:\n[agent Cursor] on it\n\n\
         The message to judge:\nsee this"
    );
}

#[test]
fn a_named_label_maps_back_to_its_bot() {
    let candidates = [
        candidate(BotId::TEST_A, "Cursor"),
        candidate(BotId::TEST_B, "Macro"),
    ];
    assert_eq!(
        pick_candidate(&candidates, Some("Macro")),
        Some(BotId::TEST_B)
    );
    assert_eq!(
        pick_candidate(&candidates, Some(" cursor ")),
        Some(BotId::TEST_A)
    );
}

#[test]
fn null_or_an_unknown_label_picks_nobody() {
    let candidates = [candidate(BotId::TEST_A, "Cursor")];
    assert_eq!(pick_candidate(&candidates, None), None);
    assert_eq!(pick_candidate(&candidates, Some("Claude")), None);
    assert_eq!(pick_candidate(&candidates, Some("")), None);
}
