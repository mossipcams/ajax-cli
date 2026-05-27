//! Skill-backed remediation options for merge conflicts and CI failures.

use crate::{
    adapters::{CommandOutput, CommandRunner, TmuxAdapter},
    commands::CommandContext,
    models::{LiveStatusKind, SideFlag, Task},
    registry::Registry,
};

pub const FIX_CI: &str = "fix-ci";
pub const RESOLVE_MERGE_CONFLICTS: &str = "resolve-merge-conflicts";

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RemediationOption {
    pub id: String,
    pub label: String,
    pub skill_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RemediationError {
    UnknownRemediation(String),
    TaskNotFound(String),
    UnsupportedCapability(&'static str),
    CommandRun(String),
}

pub fn remediations_for_task(task: &Task) -> Vec<RemediationOption> {
    let mut options = Vec::new();
    if has_merge_conflict(task) {
        options.push(RemediationOption {
            id: RESOLVE_MERGE_CONFLICTS.to_string(),
            label: "Resolve merge conflicts".to_string(),
            skill_name: "resolve-merge-conflicts".to_string(),
        });
    }
    if has_ci_failure(task) {
        options.push(RemediationOption {
            id: FIX_CI.to_string(),
            label: "Fix CI".to_string(),
            skill_name: "gh-fix-ci".to_string(),
        });
    }
    options
}

pub fn is_remediation_action(action: &str) -> bool {
    action == FIX_CI || action == RESOLVE_MERGE_CONFLICTS
}

fn has_merge_conflict(task: &Task) -> bool {
    task.has_side_flag(SideFlag::Conflicted)
        || task
            .git_status
            .as_ref()
            .is_some_and(|status| status.conflicted)
        || task
            .live_status
            .as_ref()
            .is_some_and(|live| live.kind == LiveStatusKind::MergeConflict)
}

fn has_ci_failure(task: &Task) -> bool {
    task.has_side_flag(SideFlag::TestsFailed)
        || task
            .live_status
            .as_ref()
            .is_some_and(|live| live.kind == LiveStatusKind::CiFailed)
}

pub fn format_brief(remediation_id: &str, task: &Task, skill_path: &str) -> Option<String> {
    let handle = task.qualified_handle();
    let base = task.base_branch.as_str();
    match remediation_id {
        FIX_CI => Some(format!(
            "Use the gh-fix-ci skill at {skill_path} for task {handle}. \
             Inspect failing GitHub Actions checks for this branch's PR, summarize failures, \
             propose a fix plan, and implement only after approval."
        )),
        RESOLVE_MERGE_CONFLICTS => Some(format!(
            "Use the resolve-merge-conflicts skill at {skill_path} for task {handle} \
             (base branch {base}). Compare ancestor, ours, and theirs for each conflicted file; \
             preserve valid behavior from both sides; run project validation before finishing."
        )),
        _ => None,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemediationOutcome {
    pub state_changed: bool,
    pub output: String,
}

pub fn execute_remediation<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    task_handle: &str,
    remediation_id: &str,
    skill_path: &str,
) -> Result<RemediationOutcome, RemediationError> {
    if !is_remediation_action(remediation_id) {
        return Err(RemediationError::UnknownRemediation(
            remediation_id.to_string(),
        ));
    }

    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == task_handle)
        .ok_or_else(|| RemediationError::TaskNotFound(task_handle.to_string()))?;

    let brief = format_brief(remediation_id, task, skill_path)
        .ok_or_else(|| RemediationError::UnknownRemediation(remediation_id.to_string()))?;

    let tmux = TmuxAdapter::new("tmux");
    if !task
        .tmux_status
        .as_ref()
        .is_some_and(|status| status.exists)
    {
        return Err(RemediationError::UnsupportedCapability(
            "remediation requires an active tmux task session; open the task in native Cockpit first",
        ));
    }

    let spec = tmux.send_agent_command(&task.tmux_session, &task.worktrunk_window, &brief);
    let output = runner
        .run(&spec)
        .map_err(|error| RemediationError::CommandRun(error.to_string()))?;

    Ok(RemediationOutcome {
        state_changed: false,
        output: format_remediation_output(&output, remediation_id),
    })
}

fn format_remediation_output(output: &CommandOutput, remediation_id: &str) -> String {
    let label = match remediation_id {
        FIX_CI => "Fix CI",
        RESOLVE_MERGE_CONFLICTS => "Resolve merge conflicts",
        _ => remediation_id,
    };
    let mut parts = vec![format!("{label}: sent skill brief to task agent")];
    let stdout = output.stdout.trim();
    let stderr = output.stderr.trim();
    if !stdout.is_empty() {
        parts.push(stdout.to_string());
    }
    if !stderr.is_empty() {
        parts.push(stderr.to_string());
    }
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::{
        execute_remediation, format_brief, remediations_for_task, FIX_CI, RESOLVE_MERGE_CONFLICTS,
    };
    use crate::{
        adapters::RecordingCommandRunner,
        commands::CommandContext,
        config::{Config, ManagedRepo},
        models::{
            AgentClient, LiveObservation, LiveStatusKind, SideFlag, Task, TaskId, TmuxStatus,
        },
        registry::{InMemoryRegistry, Registry},
    };

    fn task(handle: &str) -> Task {
        Task::new(
            TaskId::new(format!("task-{handle}")),
            "web",
            handle,
            format!("Task {handle}"),
            format!("ajax/{handle}"),
            "main",
            format!("/tmp/worktrees/{handle}"),
            format!("ajax-web-{handle}"),
            "worktrunk",
            AgentClient::Codex,
        )
    }

    fn context_with_task(task: Task) -> CommandContext<InMemoryRegistry> {
        let config = Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        };
        let mut registry = InMemoryRegistry::default();
        registry.create_task(task.clone()).unwrap();
        CommandContext::new(config, registry)
    }

    #[test]
    fn remediations_for_merge_conflict_includes_resolve_merge_conflicts_skill() {
        let mut task = task("merge");
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::MergeConflict,
            "merge conflict needs attention",
        ));
        task.add_side_flag(SideFlag::Conflicted);

        let options = remediations_for_task(&task);

        assert_eq!(options.len(), 1);
        assert_eq!(options[0].id, RESOLVE_MERGE_CONFLICTS);
        assert_eq!(options[0].skill_name, "resolve-merge-conflicts");
    }

    #[test]
    fn remediations_for_ci_failed_includes_fix_ci_skill() {
        let mut task = task("ci");
        task.live_status = Some(LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"));
        task.add_side_flag(SideFlag::TestsFailed);

        let options = remediations_for_task(&task);

        assert_eq!(options.len(), 1);
        assert_eq!(options[0].id, FIX_CI);
        assert_eq!(options[0].skill_name, "gh-fix-ci");
    }

    #[test]
    fn format_brief_includes_skill_path_and_handle() {
        let task = task("web/fix");

        let brief =
            format_brief(FIX_CI, &task, "/home/me/.codex/skills/gh-fix-ci/SKILL.md").unwrap();

        assert!(brief.contains("gh-fix-ci"));
        assert!(brief.contains("/home/me/.codex/skills/gh-fix-ci/SKILL.md"));
        assert!(brief.contains("web/fix"));
    }

    #[test]
    fn execute_remediation_sends_skill_brief_via_tmux() {
        let mut task = task("fix");
        task.tmux_status = Some(TmuxStatus {
            exists: true,
            session_name: task.tmux_session.clone(),
        });
        let mut context = context_with_task(task);
        let mut runner = RecordingCommandRunner::default();

        let outcome = execute_remediation(
            &mut context,
            &mut runner,
            "web/fix",
            FIX_CI,
            "/skills/gh-fix-ci/SKILL.md",
        )
        .unwrap();

        assert!(!outcome.state_changed);
        assert!(outcome.output.contains("Fix CI"));
        let commands = runner.commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].program, "tmux");
        assert_eq!(commands[0].args[0], "send-keys");
        assert!(commands[0].args[3].contains("gh-fix-ci"));
    }

    #[test]
    fn execute_remediation_requires_tmux_session() {
        let task = task("fix");
        let mut context = context_with_task(task);
        let mut runner = RecordingCommandRunner::default();

        let error = execute_remediation(
            &mut context,
            &mut runner,
            "web/fix",
            FIX_CI,
            "/skills/gh-fix-ci/SKILL.md",
        )
        .unwrap_err();

        assert!(matches!(
            error,
            super::RemediationError::UnsupportedCapability(_)
        ));
        assert!(runner.commands().is_empty());
    }
}
