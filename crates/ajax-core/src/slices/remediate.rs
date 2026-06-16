use crate::{
    adapters::{CommandOutput, CommandRunner, TmuxAdapter},
    commands::CommandContext,
    models::Task,
    recommended::{ActionAvailability, RemediationId, TaskActionDecision, TaskActionId},
    registry::Registry,
};

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemediationOutcome {
    pub state_changed: bool,
    pub output: String,
}

pub fn decisions(task: &Task) -> Vec<TaskActionDecision> {
    let mut ids = remediations_for_task(task)
        .into_iter()
        .filter_map(|option| RemediationId::from_label(option.id.as_str()))
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids.into_iter()
        .map(|id| TaskActionDecision {
            id: TaskActionId::Remediation(id),
            availability: ActionAvailability::Available,
            reason: id.compatibility_label().to_string(),
            requires_confirmation: false,
        })
        .collect()
}

pub fn remediations_for_task(task: &Task) -> Vec<RemediationOption> {
    let mut options = Vec::new();
    if has_merge_conflict(task) {
        options.push(RemediationOption {
            id: RemediationId::ResolveMergeConflicts
                .compatibility_label()
                .to_string(),
            label: "Resolve merge conflicts".to_string(),
            skill_name: "resolve-merge-conflicts".to_string(),
        });
    }
    if has_ci_failure(task) {
        options.push(RemediationOption {
            id: RemediationId::FixCi.compatibility_label().to_string(),
            label: "Fix CI".to_string(),
            skill_name: "gh-fix-ci".to_string(),
        });
    }
    options
}

pub fn is_remediation_action(action: &str) -> bool {
    RemediationId::from_label(action).is_some()
}

pub fn format_brief(remediation_id: RemediationId, task: &Task, skill_path: &str) -> String {
    let handle = task.qualified_handle();
    let base = task.base_branch.as_str();
    match remediation_id {
        RemediationId::FixCi => format!(
            "Use the gh-fix-ci skill at {skill_path} for task {handle}. \
             Inspect failing GitHub Actions checks for this branch's PR, summarize failures, \
             propose a fix plan, and implement only after approval."
        ),
        RemediationId::ResolveMergeConflicts => format!(
            "Use the resolve-merge-conflicts skill at {skill_path} for task {handle} \
             (base branch {base}). Compare ancestor, ours, and theirs for each conflicted file; \
             preserve valid behavior from both sides; run project validation before finishing."
        ),
    }
}

pub fn execute_remediation<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    task_handle: &str,
    remediation_id: RemediationId,
    skill_path: &str,
) -> Result<RemediationOutcome, RemediationError> {
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == task_handle)
        .ok_or_else(|| RemediationError::TaskNotFound(task_handle.to_string()))?;

    if !decisions(task)
        .into_iter()
        .any(|decision| decision.id == TaskActionId::Remediation(remediation_id))
    {
        return Err(RemediationError::UnsupportedCapability(
            "remediation is no longer available for this task",
        ));
    }

    let brief = format_brief(remediation_id, task, skill_path);
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

fn has_merge_conflict(task: &Task) -> bool {
    task.facts().conflicted
}

fn has_ci_failure(task: &Task) -> bool {
    task.facts().tests_failed
}

fn format_remediation_output(output: &CommandOutput, remediation_id: RemediationId) -> String {
    let label = match remediation_id {
        RemediationId::FixCi => "Fix CI",
        RemediationId::ResolveMergeConflicts => "Resolve merge conflicts",
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
        decisions, execute_remediation, format_brief, remediations_for_task, RemediationError,
    };
    use crate::{
        adapters::RecordingCommandRunner,
        commands::CommandContext,
        config::{Config, ManagedRepo},
        models::{
            AgentClient, LiveObservation, LiveStatusKind, SideFlag, Task, TaskId, TmuxStatus,
        },
        recommended::{RemediationId, TaskActionId},
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
    fn remediation_decision_uses_typed_id_and_preserves_compatibility_label() {
        let mut task = task("ci");
        task.live_status = Some(LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"));
        task.add_side_flag(SideFlag::TestsFailed);

        let decision = decisions(&task)
            .into_iter()
            .find(|decision| decision.id == TaskActionId::Remediation(RemediationId::FixCi))
            .unwrap();

        assert!(decision.is_available());
        assert_eq!(decision.reason, RemediationId::FixCi.compatibility_label());
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
        assert_eq!(
            options[0].id,
            RemediationId::ResolveMergeConflicts.compatibility_label()
        );
        assert_eq!(options[0].skill_name, "resolve-merge-conflicts");
    }

    #[test]
    fn remediations_for_ci_failed_includes_fix_ci_skill() {
        let mut task = task("ci");
        task.live_status = Some(LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"));
        task.add_side_flag(SideFlag::TestsFailed);

        let options = remediations_for_task(&task);

        assert_eq!(options.len(), 1);
        assert_eq!(options[0].id, RemediationId::FixCi.compatibility_label());
        assert_eq!(options[0].skill_name, "gh-fix-ci");
    }

    #[test]
    fn format_brief_includes_skill_path_and_handle() {
        let task = task("ci");
        let brief = format_brief(
            RemediationId::FixCi,
            &task,
            "/home/me/.codex/skills/gh-fix-ci/SKILL.md",
        );

        assert!(brief.contains("gh-fix-ci"));
        assert!(brief.contains("web/ci"));
        assert!(brief.contains("/home/me/.codex/skills/gh-fix-ci/SKILL.md"));
    }

    #[test]
    fn remediation_execution_revalidates_live_tmux_and_current_blocker() {
        let mut task = task("merge");
        task.tmux_status = Some(TmuxStatus::present("ajax-web-merge"));
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::MergeConflict,
            "merge conflict needs attention",
        ));
        task.add_side_flag(SideFlag::Conflicted);
        let mut context = context_with_task(task);
        let mut runner = RecordingCommandRunner::default();

        let outcome = execute_remediation(
            &mut context,
            &mut runner,
            "web/merge",
            RemediationId::ResolveMergeConflicts,
            "/tmp/resolve/SKILL.md",
        )
        .unwrap();

        assert!(outcome.output.contains("Resolve merge conflicts"));
        assert_eq!(runner.commands().len(), 1);

        let current = context
            .registry
            .get_task_mut(&TaskId::new("task-merge"))
            .unwrap();
        current.remove_side_flag(SideFlag::Conflicted);
        current.live_status = None;

        let error = execute_remediation(
            &mut context,
            &mut runner,
            "web/merge",
            RemediationId::ResolveMergeConflicts,
            "/tmp/resolve/SKILL.md",
        )
        .unwrap_err();

        assert_eq!(
            error,
            RemediationError::UnsupportedCapability(
                "remediation is no longer available for this task",
            )
        );
    }

    #[test]
    fn execute_remediation_requires_tmux_session() {
        let mut task = task("merge");
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::MergeConflict,
            "merge conflict needs attention",
        ));
        task.add_side_flag(SideFlag::Conflicted);
        let mut context = context_with_task(task);
        let mut runner = RecordingCommandRunner::default();

        let error = execute_remediation(
            &mut context,
            &mut runner,
            "web/merge",
            RemediationId::ResolveMergeConflicts,
            "/tmp/resolve/SKILL.md",
        )
        .unwrap_err();

        assert_eq!(
            error,
            RemediationError::UnsupportedCapability(
                "remediation requires an active tmux task session; open the task in native Cockpit first",
            )
        );
    }
}
