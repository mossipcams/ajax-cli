use crate::{
    models::{AttentionItem, LifecycleStatus, LiveObservation, RecommendedAction, Task, TaskId},
    registry::{Registry, RegistryEvent},
    ui_state::UiState,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskCard {
    pub id: TaskId,
    pub qualified_handle: String,
    pub title: String,
    pub ui_state: UiState,
    pub lifecycle: LifecycleStatus,
    pub recommended_action: RecommendedAction,
    pub action_reason: String,
    pub available_actions: Vec<RecommendedAction>,
    pub live_summary: Option<String>,
    pub blocker_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CockpitNextStep {
    pub task_id: TaskId,
    pub task_handle: String,
    pub ui_state: UiState,
    pub action: RecommendedAction,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CockpitProjection {
    pub counts: CockpitSummary,
    pub cards: Vec<TaskCard>,
    pub attention: Vec<AttentionItem>,
    pub next: Option<CockpitNextStep>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ReposResponse {
    pub repos: Vec<RepoSummary>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct RepoSummary {
    pub name: String,
    pub path: String,
    pub active_tasks: u32,
    pub attention_items: u32,
    pub reviewable_tasks: u32,
    pub cleanable_tasks: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct TasksResponse {
    pub tasks: Vec<TaskSummary>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct TaskSummary {
    pub id: String,
    pub qualified_handle: String,
    pub title: String,
    pub lifecycle_status: String,
    pub needs_attention: bool,
    pub live_status: Option<LiveObservation>,
    #[serde(default, skip_serializing)]
    pub actions: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct InspectResponse {
    pub task: TaskSummary,
    pub branch: String,
    pub worktree_path: String,
    pub tmux_session: String,
    pub flags: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct InboxResponse {
    pub items: Vec<AttentionItem>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct NextResponse {
    pub item: Option<AttentionItem>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct DoctorResponse {
    pub checks: Vec<DoctorCheck>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub ok: bool,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct CockpitSummary {
    pub repos: u32,
    pub tasks: u32,
    pub active_tasks: u32,
    pub attention_items: u32,
    pub reviewable_tasks: u32,
    pub cleanable_tasks: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct CockpitResponse {
    pub summary: CockpitSummary,
    pub repos: ReposResponse,
    pub tasks: TasksResponse,
    pub review: TasksResponse,
    pub inbox: InboxResponse,
    pub next: NextResponse,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct RegistryExportSnapshot {
    pub tasks: Vec<Task>,
    pub events: Vec<RegistryEvent>,
}

pub fn registry_export_snapshot<R: Registry>(registry: &R) -> RegistryExportSnapshot {
    RegistryExportSnapshot {
        tasks: registry.list_tasks().into_iter().cloned().collect(),
        events: registry.list_events().into_iter().cloned().collect(),
    }
}

pub fn registry_export_json_snapshot<R: Registry>(
    registry: &R,
) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&registry_export_snapshot(registry))
}

#[cfg(test)]
mod tests {
    use super::{
        registry_export_json_snapshot, CockpitResponse, CockpitSummary, DoctorCheck,
        DoctorResponse, InboxResponse, InspectResponse, NextResponse, RepoSummary, ReposResponse,
        TaskSummary, TasksResponse,
    };
    use crate::{
        models::{
            AgentClient, AttentionItem, LiveObservation, LiveStatusKind, RecommendedAction, Task,
            TaskId,
        },
        registry::{InMemoryRegistry, Registry, RegistryEventKind},
    };

    #[test]
    fn read_commands_serialize_as_json_contracts() {
        let repos = ReposResponse {
            repos: vec![RepoSummary {
                name: "web".to_string(),
                path: "/Users/matt/projects/web".to_string(),
                active_tasks: 2,
                attention_items: 1,
                reviewable_tasks: 1,
                cleanable_tasks: 0,
            }],
        };
        let tasks = TasksResponse {
            tasks: vec![TaskSummary {
                id: "task-1".to_string(),
                qualified_handle: "web/fix-login".to_string(),
                title: "Fix login".to_string(),
                lifecycle_status: "active".to_string(),
                needs_attention: false,
                live_status: Some(LiveObservation::new(
                    LiveStatusKind::WaitingForApproval,
                    "waiting for approval",
                )),
                actions: vec![RecommendedAction::OpenTask.as_str().to_string()],
            }],
        };
        let inspect = InspectResponse {
            task: tasks.tasks[0].clone(),
            branch: "ajax/fix-login".to_string(),
            worktree_path: "/tmp/worktrees/web-fix-login".to_string(),
            tmux_session: "ajax-web-fix-login".to_string(),
            flags: vec!["dirty".to_string()],
        };
        let inbox = InboxResponse {
            items: vec![AttentionItem {
                task_id: crate::models::TaskId::new("task-1"),
                task_handle: "web/fix-login".to_string(),
                reason: "agent needs input".to_string(),
                priority: 10,
                recommended_action: "open task".to_string(),
            }],
        };
        let next = NextResponse {
            item: Some(inbox.items[0].clone()),
        };
        let doctor = DoctorResponse {
            checks: vec![DoctorCheck {
                name: "git".to_string(),
                ok: true,
                message: "available".to_string(),
            }],
        };
        let cockpit = CockpitResponse {
            summary: CockpitSummary {
                repos: 1,
                tasks: 1,
                active_tasks: 1,
                attention_items: 1,
                reviewable_tasks: 0,
                cleanable_tasks: 0,
            },
            repos: repos.clone(),
            tasks: tasks.clone(),
            review: TasksResponse { tasks: vec![] },
            inbox: inbox.clone(),
            next: next.clone(),
        };

        assert_eq!(
            serde_json::to_value(&repos).unwrap(),
            serde_json::json!({
                "repos": [{
                    "name": "web",
                    "path": "/Users/matt/projects/web",
                    "active_tasks": 2,
                    "attention_items": 1,
                    "reviewable_tasks": 1,
                    "cleanable_tasks": 0
                }]
            })
        );
        assert_eq!(
            serde_json::to_value(&tasks).unwrap(),
            serde_json::json!({
                "tasks": [{
                    "id": "task-1",
                    "qualified_handle": "web/fix-login",
                    "title": "Fix login",
                    "lifecycle_status": "active",
                    "needs_attention": false,
                    "live_status": {
                        "kind": "WaitingForApproval",
                        "summary": "waiting for approval"
                    }
                }]
            })
        );
        assert_eq!(
            serde_json::to_value(&inspect).unwrap(),
            serde_json::json!({
                "task": tasks.tasks[0],
                "branch": "ajax/fix-login",
                "worktree_path": "/tmp/worktrees/web-fix-login",
                "tmux_session": "ajax-web-fix-login",
                "flags": ["dirty"]
            })
        );
        assert_eq!(
            serde_json::to_value(&inbox).unwrap(),
            serde_json::json!({
                "items": [{
                    "task_id": "task-1",
                    "task_handle": "web/fix-login",
                    "reason": "agent needs input",
                    "priority": 10,
                    "recommended_action": RecommendedAction::OpenTask.as_str()
                }]
            })
        );
        assert_eq!(
            serde_json::to_value(&next).unwrap(),
            serde_json::json!({
                "item": inbox.items[0]
            })
        );
        assert_eq!(
            serde_json::to_value(&doctor).unwrap(),
            serde_json::json!({
                "checks": [{
                    "name": "git",
                    "ok": true,
                    "message": "available"
                }]
            })
        );
        assert_eq!(
            serde_json::to_value(&cockpit).unwrap(),
            serde_json::json!({
                "summary": {
                    "repos": 1,
                    "tasks": 1,
                    "active_tasks": 1,
                    "attention_items": 1,
                    "reviewable_tasks": 0,
                    "cleanable_tasks": 0
                },
                "repos": repos,
                "tasks": tasks,
                "review": { "tasks": [] },
                "inbox": inbox,
                "next": next
            })
        );
    }

    #[test]
    fn output_contracts_do_not_keep_unused_format_wrapper() {
        let output_source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/output.rs"),
        )
        .unwrap();
        let wrapper_name = ["Output", "Format"].concat();

        assert!(!output_source.contains(&wrapper_name));
    }

    #[test]
    fn registry_export_snapshot_serializes_state_as_json_contract() {
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(Task::new(
                TaskId::new("task-1"),
                "web",
                "fix-login",
                "Fix login",
                "ajax/fix-login",
                "main",
                "/tmp/worktrees/web-fix-login",
                "ajax-web-fix-login",
                "worktrunk",
                AgentClient::Codex,
            ))
            .unwrap();
        registry
            .record_event(TaskId::new("task-1"), RegistryEventKind::UserNote, "ready")
            .unwrap();

        let json = registry_export_json_snapshot(&registry).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["tasks"][0]["repo"], "web");
        assert_eq!(parsed["tasks"][0]["handle"], "fix-login");
        assert_eq!(parsed["events"][1]["message"], "ready");
    }
}
