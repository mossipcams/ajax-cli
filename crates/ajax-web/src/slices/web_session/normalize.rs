//! Host-owned stream normalization: ACP delta or cumulative chunks become one
//! full-content item update with a stable host item id.

use super::output_content::OutputContentBlockWire;
use super::SessionServerEvent;
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct StreamNormalizer {
    next_id: u64,
    lanes: HashMap<String, LaneState>,
}

struct LaneState {
    item_id: String,
    text: String,
    content_blocks: Vec<OutputContentBlockWire>,
}

impl StreamNormalizer {
    pub(crate) fn normalize_batch(
        &mut self,
        events: Vec<SessionServerEvent>,
    ) -> Vec<SessionServerEvent> {
        let mut out = Vec::with_capacity(events.len());
        for event in events {
            out.extend(self.normalize_one(event));
        }
        collapse_same_item(out)
    }

    fn normalize_one(&mut self, event: SessionServerEvent) -> Vec<SessionServerEvent> {
        match event {
            SessionServerEvent::Message {
                role,
                text,
                content_blocks,
                message_id,
                ..
            } if matches!(role.as_str(), "agent" | "thought" | "user") => {
                if text.is_empty() && content_blocks.is_empty() {
                    return Vec::new();
                }
                if role == "user" {
                    self.close_reply_lanes();
                }
                let key = lane_key(&role, &message_id);
                let continuation = self
                    .lanes
                    .get(&key)
                    .map(|lane| is_stream_continuation(&lane.text, &text))
                    .unwrap_or(true);
                let item_id = if continuation {
                    self.lanes
                        .get(&key)
                        .map(|lane| lane.item_id.clone())
                        .unwrap_or_else(|| self.alloc_item_id())
                } else {
                    self.lanes.remove(&key);
                    self.alloc_item_id()
                };
                let lane = self.lanes.entry(key).or_insert_with(|| LaneState {
                    item_id: item_id.clone(),
                    text: String::new(),
                    content_blocks: Vec::new(),
                });
                lane.item_id = item_id.clone();
                lane.text = if continuation {
                    merge_stream_text(&lane.text, &text)
                } else {
                    text.clone()
                };
                for block in content_blocks {
                    if !lane.content_blocks.contains(&block) {
                        lane.content_blocks.push(block);
                    }
                }
                vec![SessionServerEvent::Message {
                    role,
                    text: lane.text.clone(),
                    content_blocks: lane.content_blocks.clone(),
                    message_id,
                    item_id: lane.item_id.clone(),
                }]
            }
            SessionServerEvent::TurnEnd { .. } => {
                self.lanes.clear();
                vec![event]
            }
            other => {
                self.lanes.clear();
                vec![other]
            }
        }
    }

    /// Close agent/thought lanes so the next reply cannot upsert into a prior bubble.
    pub(crate) fn close_reply_lanes(&mut self) {
        self.lanes.retain(|key, _| key.starts_with("user:"));
    }

    pub(crate) fn fresh_item_id(&mut self) -> String {
        self.alloc_item_id()
    }

    fn alloc_item_id(&mut self) -> String {
        self.next_id += 1;
        format!("i{}", self.next_id)
    }
}

fn lane_key(role: &str, message_id: &Option<String>) -> String {
    format!("{role}:{}", message_id.as_deref().unwrap_or(""))
}

fn collapse_same_item(events: Vec<SessionServerEvent>) -> Vec<SessionServerEvent> {
    let mut out = Vec::with_capacity(events.len());
    for event in events {
        let replace_last = match (&out.last(), &event) {
            (
                Some(SessionServerEvent::Message { item_id: left, .. }),
                SessionServerEvent::Message { item_id: right, .. },
            ) => !left.is_empty() && left == right,
            _ => false,
        };
        if replace_last {
            let last = out.len() - 1;
            out[last] = event;
        } else {
            out.push(event);
        }
    }
    out
}

/// True when `incoming` continues token streaming on `previous` (cumulative or delta).
pub(crate) fn is_stream_continuation(previous: &str, incoming: &str) -> bool {
    if previous.is_empty() || incoming.is_empty() {
        return true;
    }
    if incoming == previous {
        return true;
    }
    if incoming.starts_with(previous) {
        return true;
    }
    if (previous.ends_with('.') || previous.ends_with('!') || previous.ends_with('?'))
        && incoming
            .chars()
            .next()
            .is_some_and(|ch| ch.is_uppercase() || ch.is_ascii_digit())
    {
        return false;
    }
    true
}

/// Resolve delta vs cumulative harness behavior into one full string.
pub(crate) fn merge_stream_text(previous: &str, incoming: &str) -> String {
    if incoming == previous || (incoming.starts_with(previous) && incoming.len() > previous.len()) {
        incoming.to_string()
    } else {
        format!("{previous}{incoming}")
    }
}
