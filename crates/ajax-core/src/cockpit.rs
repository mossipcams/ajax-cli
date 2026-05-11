use serde::{Deserialize, Serialize};

use crate::models::{LiveStatusKind, TaskId};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CockpitSnapshot {
    pub repos: Vec<CockpitRepoView>,
    pub tasks: Vec<CockpitTaskView>,
    pub attention: Vec<CockpitAttentionItem>,
    pub live: Vec<CockpitLiveStatus>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CockpitTaskView {
    pub id: TaskId,
    pub repo: String,
    pub handle: String,
    pub title: String,
    pub status: TaskStatus,
    pub needs_attention: bool,
    pub session: Option<CockpitSessionView>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CockpitRepoView {
    pub name: String,
    pub path: String,
    pub active_tasks: u32,
    pub attention_items: u32,
    pub reviewable_tasks: u32,
    pub cleanable_tasks: u32,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CockpitSessionView {
    pub tmux_session: String,
    pub worktrunk_window: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CockpitAttentionItem {
    pub task_id: TaskId,
    pub task_handle: String,
    pub reason: String,
    pub priority: u32,
    pub action: CockpitAction,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CockpitLiveStatus {
    pub task_id: TaskId,
    pub kind: LiveStatusKind,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
    Created,
    Provisioning,
    Active,
    Waiting,
    Reviewable,
    Mergeable,
    Merged,
    Cleanable,
    Removed,
    Orphaned,
    Error,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum CockpitAction {
    SelectTask { task_id: TaskId },
    OpenTask { task_id: TaskId },
    CheckTask { task_id: TaskId },
    Refresh,
    Quit,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum CockpitActionResult {
    Snapshot(CockpitSnapshot),
    Message(String),
    Quit,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum CockpitEvent {
    SnapshotUpdated(CockpitSnapshot),
    TaskStatusChanged { task_id: TaskId, status: TaskStatus },
    AttentionChanged(Vec<CockpitAttentionItem>),
}

#[cfg(test)]
mod tests {
    use crate::{
        cockpit::{
            CockpitAction, CockpitActionResult, CockpitAttentionItem, CockpitEvent,
            CockpitLiveStatus, CockpitRepoView, CockpitSessionView, CockpitSnapshot,
            CockpitTaskView, TaskStatus,
        },
        models::{LiveStatusKind, TaskId},
    };

    fn sample_snapshot() -> CockpitSnapshot {
        CockpitSnapshot {
            repos: vec![CockpitRepoView {
                name: "web".to_string(),
                path: "/Users/matt/projects/web".to_string(),
                active_tasks: 1,
                attention_items: 1,
                reviewable_tasks: 0,
                cleanable_tasks: 0,
            }],
            tasks: vec![CockpitTaskView {
                id: TaskId::new("task-1"),
                repo: "web".to_string(),
                handle: "fix-login".to_string(),
                title: "Fix login".to_string(),
                status: TaskStatus::Active,
                needs_attention: true,
                session: Some(CockpitSessionView {
                    tmux_session: "ajax-web-fix-login".to_string(),
                    worktrunk_window: "worktrunk".to_string(),
                }),
            }],
            attention: vec![CockpitAttentionItem {
                task_id: TaskId::new("task-1"),
                task_handle: "web/fix-login".to_string(),
                reason: "waiting for approval".to_string(),
                priority: 5,
                action: CockpitAction::OpenTask {
                    task_id: TaskId::new("task-1"),
                },
            }],
            live: vec![CockpitLiveStatus {
                task_id: TaskId::new("task-1"),
                kind: LiveStatusKind::WaitingForApproval,
                summary: "waiting for approval".to_string(),
            }],
        }
    }

    #[test]
    fn snapshot_serializes_as_plain_typed_contract() {
        let snapshot = sample_snapshot();

        assert_eq!(
            serde_json::to_value(&snapshot).unwrap(),
            serde_json::json!({
                "repos": [{
                    "name": "web",
                    "path": "/Users/matt/projects/web",
                    "active_tasks": 1,
                    "attention_items": 1,
                    "reviewable_tasks": 0,
                    "cleanable_tasks": 0
                }],
                "tasks": [{
                    "id": "task-1",
                    "repo": "web",
                    "handle": "fix-login",
                    "title": "Fix login",
                    "status": "Active",
                    "needs_attention": true,
                    "session": {
                        "tmux_session": "ajax-web-fix-login",
                        "worktrunk_window": "worktrunk"
                    }
                }],
                "attention": [{
                    "task_id": "task-1",
                    "task_handle": "web/fix-login",
                    "reason": "waiting for approval",
                    "priority": 5,
                    "action": {
                        "OpenTask": {
                            "task_id": "task-1"
                        }
                    }
                }],
                "live": [{
                    "task_id": "task-1",
                    "kind": "WaitingForApproval",
                    "summary": "waiting for approval"
                }]
            })
        );
    }

    #[test]
    fn action_result_and_event_round_trip_through_json() {
        let action = CockpitAction::CheckTask {
            task_id: TaskId::new("task-1"),
        };
        let action_json = serde_json::to_string(&action).unwrap();
        assert_eq!(
            serde_json::from_str::<CockpitAction>(&action_json).unwrap(),
            action
        );

        let result = CockpitActionResult::Snapshot(sample_snapshot());
        let result_json = serde_json::to_string(&result).unwrap();
        assert_eq!(
            serde_json::from_str::<CockpitActionResult>(&result_json).unwrap(),
            result
        );

        let event = CockpitEvent::TaskStatusChanged {
            task_id: TaskId::new("task-1"),
            status: TaskStatus::Reviewable,
        };
        let event_json = serde_json::to_string(&event).unwrap();
        assert_eq!(
            serde_json::from_str::<CockpitEvent>(&event_json).unwrap(),
            event
        );
    }
}
