use super::normalize::{is_stream_continuation, merge_stream_text, StreamNormalizer};
use crate::slices::web_session::SessionServerEvent;

#[test]
fn merge_stream_text_appends_deltas_and_replaces_cumulative() {
    assert_eq!(merge_stream_text("hel", "lo"), "hello");
    assert_eq!(merge_stream_text("Hello", "Hello world"), "Hello world");
    assert_eq!(merge_stream_text("same", "same"), "same");
}

#[test]
fn is_stream_continuation_detects_cumulative_delta_and_sentence_chunks() {
    assert!(is_stream_continuation("hel", "lo"));
    assert!(is_stream_continuation("Hello", "Hello world"));
    // Sentence-boundary chunks without messageId stay one stream (#1141).
    assert!(is_stream_continuation("First reply.", "Second reply."));
}

#[test]
fn streamed_agent_updates_publish_one_complete_transcript_item() {
    let mut normalizer = StreamNormalizer::default();
    let events = normalizer.normalize_batch(vec![
        SessionServerEvent::Message {
            role: "agent".to_string(),
            text: "hel".to_string(),
            content_blocks: Vec::new(),
            item_id: String::new(),
            message_id: None,
        },
        SessionServerEvent::Message {
            role: "agent".to_string(),
            text: "hello".to_string(),
            content_blocks: Vec::new(),
            item_id: String::new(),
            message_id: None,
        },
    ]);
    assert_eq!(events.len(), 1);
    let SessionServerEvent::Message { text, item_id, .. } = &events[0] else {
        panic!("expected message");
    };
    assert_eq!(text, "hello");
    assert!(!item_id.is_empty());
}

#[test]
fn sentence_boundary_stream_chunks_without_message_id_stay_one_item() {
    // Regression for #1140 / fix in #1141: streamed prose must not split into new bubbles
    // at sentence boundaries when no user prompt, turn_end, or non-message event intervenes.
    let mut normalizer = StreamNormalizer::default();
    let events = normalizer.normalize_batch(vec![
        SessionServerEvent::Message {
            role: "agent".to_string(),
            text: "First sentence. ".to_string(),
            content_blocks: Vec::new(),
            item_id: String::new(),
            message_id: None,
        },
        SessionServerEvent::Message {
            role: "agent".to_string(),
            text: "Second sentence.".to_string(),
            content_blocks: Vec::new(),
            item_id: String::new(),
            message_id: None,
        },
    ]);
    assert_eq!(events.len(), 1);
    let SessionServerEvent::Message { text, item_id, .. } = &events[0] else {
        panic!("expected message");
    };
    assert_eq!(text, "First sentence. Second sentence.");
    assert!(!item_id.is_empty());
}

#[test]
fn user_prompt_closes_reply_lanes_so_next_turn_does_not_append() {
    let mut normalizer = StreamNormalizer::default();
    let events = normalizer.normalize_batch(vec![
        SessionServerEvent::Message {
            role: "agent".to_string(),
            text: "First reply.".to_string(),
            content_blocks: Vec::new(),
            item_id: String::new(),
            message_id: None,
        },
        SessionServerEvent::Message {
            role: "user".to_string(),
            text: "Next prompt".to_string(),
            content_blocks: Vec::new(),
            item_id: String::new(),
            message_id: None,
        },
        SessionServerEvent::Message {
            role: "agent".to_string(),
            text: "Second reply.".to_string(),
            content_blocks: Vec::new(),
            item_id: String::new(),
            message_id: None,
        },
    ]);
    assert_eq!(events.len(), 3);
    let agent_ids: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            SessionServerEvent::Message { role, item_id, .. } if role == "agent" => {
                Some(item_id.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(agent_ids.len(), 2);
    assert_ne!(agent_ids[0], agent_ids[1]);
}

#[test]
fn cumulative_and_delta_acp_streams_produce_the_same_transcript() {
    let mut delta = StreamNormalizer::default();
    let mut cumulative = StreamNormalizer::default();
    let delta_out = delta.normalize_batch(vec![
        SessionServerEvent::Message {
            role: "agent".to_string(),
            text: "The ".to_string(),
            content_blocks: Vec::new(),
            item_id: String::new(),
            message_id: None,
        },
        SessionServerEvent::Message {
            role: "agent".to_string(),
            text: "bug".to_string(),
            content_blocks: Vec::new(),
            item_id: String::new(),
            message_id: None,
        },
    ]);
    let cumulative_out = cumulative.normalize_batch(vec![
        SessionServerEvent::Message {
            role: "agent".to_string(),
            text: "The ".to_string(),
            content_blocks: Vec::new(),
            item_id: String::new(),
            message_id: None,
        },
        SessionServerEvent::Message {
            role: "agent".to_string(),
            text: "The bug".to_string(),
            content_blocks: Vec::new(),
            item_id: String::new(),
            message_id: None,
        },
    ]);
    let final_delta = delta_out.last().and_then(|e| e.message_text());
    let final_cumulative = cumulative_out.last().and_then(|e| e.message_text());
    assert_eq!(final_delta, Some("The bug".to_string()));
    assert_eq!(final_cumulative, Some("The bug".to_string()));
}

trait MessageHelpers {
    fn message_text(&self) -> Option<String>;
}

impl MessageHelpers for SessionServerEvent {
    fn message_text(&self) -> Option<String> {
        match self {
            SessionServerEvent::Message { text, .. } => Some(text.clone()),
            _ => None,
        }
    }
}
