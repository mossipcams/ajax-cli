//! Pure test helpers for the agent-watcher policy tests: event
//! constructors and small state/config builders. No assertions live here.

use crate::agent_watcher::policy::WatcherConfig;
use crate::agent_watcher::state::WatcherState;
use crate::agent_watcher::types::{
    ProgressState, TaskFrame, WatcherEvent, WatcherEventDetail, WatcherEventKind, WatcherVerdict,
};
use crate::canonical_agent_event::{AttentionReason, TurnOutcome};

pub fn frame() -> TaskFrame {
    TaskFrame {
        objective: "Fix the flaky test in the parser".to_string(),
    }
}

pub fn state() -> WatcherState {
    WatcherState::new(&frame(), "task-1", "run-1", "tmux")
}

pub fn config() -> WatcherConfig {
    WatcherConfig::default()
}

pub struct Ids(usize);

impl Ids {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn next(&mut self) -> String {
        self.0 += 1;
        format!("e{}", self.0)
    }
}

pub fn verdict(state: ProgressState, confidence: f64) -> WatcherVerdict {
    WatcherVerdict { state, confidence }
}

pub fn turn_started(ids: &mut Ids, at: u64) -> WatcherEvent {
    WatcherEvent {
        kind: WatcherEventKind::TurnStarted,
        detail: WatcherEventDetail::None,
        occurred_at_ms: at,
        event_id: ids.next(),
    }
}

pub fn activity_finished(
    ids: &mut Ids,
    at: u64,
    activity_id: &str,
    signature: &str,
) -> WatcherEvent {
    WatcherEvent {
        kind: WatcherEventKind::ActivityFinished,
        detail: WatcherEventDetail::Activity {
            activity_id: Some(activity_id.to_string()),
            signature: Some(signature.to_string()),
            success: Some(true),
        },
        occurred_at_ms: at,
        event_id: ids.next(),
    }
}

pub fn activity_finished_with_result(
    ids: &mut Ids,
    at: u64,
    activity_id: &str,
    signature: &str,
    success: Option<bool>,
) -> WatcherEvent {
    WatcherEvent {
        kind: WatcherEventKind::ActivityFinished,
        detail: WatcherEventDetail::Activity {
            activity_id: Some(activity_id.to_string()),
            signature: Some(signature.to_string()),
            success,
        },
        occurred_at_ms: at,
        event_id: ids.next(),
    }
}

pub fn activity_started(ids: &mut Ids, at: u64, activity_id: &str) -> WatcherEvent {
    WatcherEvent {
        kind: WatcherEventKind::ActivityStarted,
        detail: WatcherEventDetail::Activity {
            activity_id: Some(activity_id.to_string()),
            signature: None,
            success: None,
        },
        occurred_at_ms: at,
        event_id: ids.next(),
    }
}

pub fn child_started(ids: &mut Ids, at: u64) -> WatcherEvent {
    WatcherEvent {
        kind: WatcherEventKind::ChildStarted,
        detail: WatcherEventDetail::None,
        occurred_at_ms: at,
        event_id: ids.next(),
    }
}

pub fn child_settled(ids: &mut Ids, at: u64) -> WatcherEvent {
    WatcherEvent {
        kind: WatcherEventKind::ChildSettled,
        detail: WatcherEventDetail::None,
        occurred_at_ms: at,
        event_id: ids.next(),
    }
}

pub fn heartbeat_with_id(at: u64, event_id: String) -> WatcherEvent {
    WatcherEvent {
        kind: WatcherEventKind::Heartbeat,
        detail: WatcherEventDetail::None,
        occurred_at_ms: at,
        event_id,
    }
}

pub fn settled_completed(ids: &mut Ids, at: u64) -> WatcherEvent {
    WatcherEvent {
        kind: WatcherEventKind::TurnSettled,
        detail: WatcherEventDetail::TurnSettled {
            outcome: TurnOutcome::Completed,
        },
        occurred_at_ms: at,
        event_id: ids.next(),
    }
}

pub fn attention(ids: &mut Ids, at: u64, reason: AttentionReason) -> WatcherEvent {
    WatcherEvent {
        kind: WatcherEventKind::Attention,
        detail: WatcherEventDetail::Attention { attention: reason },
        occurred_at_ms: at,
        event_id: ids.next(),
    }
}

pub fn heartbeat(ids: &mut Ids, at: u64) -> WatcherEvent {
    WatcherEvent {
        kind: WatcherEventKind::Heartbeat,
        detail: WatcherEventDetail::None,
        occurred_at_ms: at,
        event_id: ids.next(),
    }
}
