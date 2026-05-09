use crate::models::{AttentionItem, LiveObservation};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputFormat {
    Human,
    Json,
}

impl OutputFormat {
    pub fn from_json_flag(json: bool) -> Self {
        if json {
            Self::Json
        } else {
            Self::Human
        }
    }
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
    pub reviewable_tasks: u32,
    pub cleanable_tasks: u32,
    pub broken_tasks: u32,
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
pub struct ReconcileResponse {
    pub tasks_checked: u32,
    pub tasks_changed: u32,
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
pub struct CockpitResponse {
    pub repos: ReposResponse,
    pub tasks: TasksResponse,
    pub review: TasksResponse,
    pub inbox: InboxResponse,
}

#[cfg(test)]
mod tests {
    use super::{
        CockpitResponse, DoctorCheck, DoctorResponse, InboxResponse, InspectResponse, NextResponse,
        OutputFormat, ReconcileResponse, RepoSummary, ReposResponse, TaskSummary, TasksResponse,
    };
    use crate::models::AttentionItem;

    #[test]
    fn read_commands_serialize_as_json_contracts() {
        let repos = ReposResponse {
            repos: vec![RepoSummary {
                name: "web".to_string(),
                path: "/Users/matt/projects/web".to_string(),
                active_tasks: 2,
                reviewable_tasks: 1,
                cleanable_tasks: 0,
                broken_tasks: 0,
            }],
        };
        let tasks = TasksResponse {
            tasks: vec![TaskSummary {
                id: "task-1".to_string(),
                qualified_handle: "web/fix-login".to_string(),
                title: "Fix login".to_string(),
                lifecycle_status: "active".to_string(),
                needs_attention: false,
                live_status: None,
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
        let reconcile = ReconcileResponse {
            tasks_checked: 1,
            tasks_changed: 1,
        };
        let doctor = DoctorResponse {
            checks: vec![DoctorCheck {
                name: "workmux".to_string(),
                ok: true,
                message: "available".to_string(),
            }],
        };
        let cockpit = CockpitResponse {
            repos: repos.clone(),
            tasks: tasks.clone(),
            review: tasks.clone(),
            inbox: inbox.clone(),
        };

        assert!(serde_json::to_string(&repos).unwrap().contains("\"repos\""));
        assert!(serde_json::to_string(&tasks).unwrap().contains("\"tasks\""));
        assert!(serde_json::to_string(&inspect)
            .unwrap()
            .contains("\"worktree_path\""));
        assert!(serde_json::to_string(&inbox).unwrap().contains("\"items\""));
        assert!(serde_json::to_string(&next).unwrap().contains("\"item\""));
        assert!(serde_json::to_string(&reconcile)
            .unwrap()
            .contains("\"tasks_changed\""));
        assert!(serde_json::to_string(&doctor)
            .unwrap()
            .contains("\"checks\""));
        assert!(serde_json::to_string(&cockpit)
            .unwrap()
            .contains("\"review\""));
    }

    #[test]
    fn output_format_distinguishes_human_and_json() {
        assert_eq!(OutputFormat::from_json_flag(true), OutputFormat::Json);
        assert_eq!(OutputFormat::from_json_flag(false), OutputFormat::Human);
    }
}
