use super::*;
use agent_client_protocol::schema::v1::{ResourceLink, TextContent};

fn text(value: &str) -> ContentBlock {
    ContentBlock::Text(TextContent::new(value))
}

#[test]
fn a_pasted_image_link_is_taken_whole() {
    let url = "https://encrypted-tbn0.gstatic.com/images?q=tbn:ANd9GcQ4G_j3ulyJepZSNOD8TeEs_ZOvjgALc0aQ_lneOFUMgS86F_iCZcnR8PtQ&s=10";
    let blocks = vec![text(&format!("look at {url} please"))];
    assert_eq!(linked_image_urls(&blocks), vec![url.to_owned()]);
}

#[test]
fn surrounding_punctuation_is_not_part_of_the_link() {
    let blocks = vec![text("see (https://cdn.example/a.png).")];
    assert_eq!(
        linked_image_urls(&blocks),
        vec!["https://cdn.example/a.png".to_owned()]
    );
}

#[test]
fn https_is_not_read_as_a_later_http_link() {
    let blocks = vec![text(
        "https://cdn.example/a.png and then http://cdn.example/b.jpg",
    )];
    assert_eq!(
        linked_image_urls(&blocks),
        vec![
            "https://cdn.example/a.png".to_owned(),
            "http://cdn.example/b.jpg".to_owned(),
        ]
    );
}

#[test]
fn a_link_already_carried_as_an_image_is_not_fetched_again() {
    let image = CursorPromptImage {
        data: "aW1n".to_owned(),
        mime_type: "image/png".to_owned(),
        source_url: Some("https://cdn.example/a.png".to_owned()),
    };
    let blocks = vec![text("https://cdn.example/a.png"), image.to_content_block()];
    assert!(linked_image_urls(&blocks).is_empty());
}

#[test]
fn an_svg_resource_link_is_not_fetched() {
    let blocks = vec![ContentBlock::ResourceLink(
        ResourceLink::new("icon.svg", "https://cdn.example/icon.svg").mime_type("image/svg+xml"),
    )];
    assert!(linked_image_urls(&blocks).is_empty());
}

#[test]
fn a_png_resource_link_is_fetched() {
    let blocks = vec![ContentBlock::ResourceLink(
        ResourceLink::new("shot.png", "https://cdn.example/shot.png").mime_type("image/png"),
    )];
    assert_eq!(
        linked_image_urls(&blocks),
        vec!["https://cdn.example/shot.png".to_owned()]
    );
}

#[test]
fn magic_bytes_name_the_image_when_the_type_does_not() {
    assert_eq!(
        detect_mime(
            Some("application/octet-stream"),
            &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
        ),
        Some("image/png")
    );
    assert_eq!(
        detect_mime(Some("image/jpeg"), &[0xFF, 0xD8, 0xFF, 0x00]),
        Some("image/jpeg")
    );
    assert_eq!(detect_mime(Some("text/html"), b"<html>"), None);
    assert_eq!(detect_mime(Some("image/jpg"), b""), Some("image/jpeg"));
    assert_eq!(detect_mime(Some("image/svg+xml"), b"<svg>"), None);
}

#[test]
fn an_image_link_inside_the_agent_context_node_is_taken() {
    let url = "https://static.example/file/d0b2430c-6826-44a1-9a8d-c8aab90703c7";
    let payload = serde_json::json!({
        "version": 1,
        "text": format!("<message>see below\n{url}</message>"),
    })
    .to_string()
    .replace('<', "\\u003c");
    let prompt = format!("<m-agent-context>{payload}</m-agent-context>\n\nbump");
    assert_eq!(linked_image_urls(&[text(&prompt)]), vec![url.to_owned()]);
}

#[test]
fn a_context_image_is_kept_ahead_of_ordinary_links() {
    let shot = "https://cdn.example/shot.png";
    let payload = serde_json::json!({
        "version": 1,
        "text": shot,
    })
    .to_string();
    let mut prompt = format!("<m-agent-context>{payload}</m-agent-context>\n\n");
    for index in 0..super::MAX_LINK_FETCHES {
        prompt.push_str(&format!("https://news.example/story-{index} "));
    }
    let urls = linked_image_urls(&[text(&prompt)]);
    assert_eq!(urls[0], shot);
    assert_eq!(urls.len(), super::MAX_LINK_FETCHES);
}

#[test]
fn a_markdown_image_in_the_context_is_taken_without_an_extension() {
    let url = "https://cdn.example/render?id=1";
    let payload = serde_json::json!({
        "version": 1,
        "text": format!("![canvas]({url})"),
    })
    .to_string()
    .replace('<', "\\u003c");
    let prompt = format!("<m-agent-context>{payload}</m-agent-context>\n\nbump");
    assert_eq!(linked_image_urls(&[text(&prompt)]), vec![url.to_owned()]);
}

#[test]
fn a_fetched_image_is_an_acp_image_frame_named_by_its_url() {
    let image = CursorPromptImage {
        data: "aW1n".to_owned(),
        mime_type: "image/png".to_owned(),
        source_url: Some("https://cdn.example/a.png".to_owned()),
    };
    let ContentBlock::Image(frame) = image.to_content_block() else {
        panic!("an image frame");
    };
    assert_eq!(frame.data, "aW1n");
    assert_eq!(frame.mime_type, "image/png");
    assert_eq!(frame.uri.as_deref(), Some("https://cdn.example/a.png"));
}
