//! Host-owned stream normalization: ACP delta or cumulative chunks become one
//! full-content item update with a stable host item id.

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
                message_id,
                ..
            } if matches!(role.as_str(), "agent" | "thought" | "user") => {
                if text.is_empty() {
                    return Vec::new();
                }
                let key = lane_key(&role, &message_id);
                let item_id = self
                    .lanes
                    .get(&key)
                    .map(|lane| lane.item_id.clone())
                    .unwrap_or_else(|| self.alloc_item_id());
                let lane = self.lanes.entry(key).or_insert_with(|| LaneState {
                    item_id: item_id.clone(),
                    text: String::new(),
                });
                lane.text = merge_stream_text(&lane.text, &text);
                vec![SessionServerEvent::Message {
                    role,
                    text: lane.text.clone(),
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

/// Resolve delta vs cumulative harness behavior into one full string.
pub(crate) fn merge_stream_text(previous: &str, incoming: &str) -> String {
    if incoming == previous || (incoming.starts_with(previous) && incoming.len() > previous.len()) {
        incoming.to_string()
    } else {
        format!("{previous}{incoming}")
    }
}
