use crate::{
    adapters::{CommandOutput, CommandRunner, TmuxAdapter},
    commands::CommandContext,
    diff_review::{stored_pull_requests, PullRequestState},
    models::{LifecycleStatus, LiveStatusKind, SideFlag, Task, TaskKind},
    operation::has_open_pull_request,
    registry::Registry,
};

pub const FIX_CI: &str = "fix-ci";
pub const RESOLVE_MERGE_CONFLICTS: &str = "resolve-merge-conflicts";
pub const CREATE_PR: &str = "create-pr";
pub const MERGE_PR: &str = "merge-pr";
pub const RESTART_AGENT: &str = "restart-agent";

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
    if offers_create_pr(task) {
        options.push(RemediationOption {
            id: CREATE_PR.to_string(),
            label: "Create PR".to_string(),
            skill_name: String::new(),
        });
    }
    if offers_merge_pr(task) {
        options.push(RemediationOption {
            id: MERGE_PR.to_string(),
            label: "Merge PR".to_string(),
            skill_name: String::new(),
        });
    }
    if offers_restart_agent(task) {
        options.push(RemediationOption {
            id: RESTART_AGENT.to_string(),
            label: "Restart agent".to_string(),
            skill_name: String::new(),
        });
    }
    options
}

pub fn is_remediation_action(action: &str) -> bool {
    action == FIX_CI || action == RESOLVE_MERGE_CONFLICTS || action == CREATE_PR
}

fn offers_create_pr(task: &Task) -> bool {
    is_open_chat_task(task)
        && !matches!(
            task.lifecycle_status,
            LifecycleStatus::Removing
                | LifecycleStatus::TeardownIncomplete
                | LifecycleStatus::Error
                | LifecycleStatus::Orphaned
        )
        && stored_pull_requests(task)
            .iter()
            .all(|pr| pr.state == PullRequestState::Closed)
}

fn offers_merge_pr(task: &Task) -> bool {
    is_open_chat_task(task) && has_open_pull_request(task)
}

fn offers_restart_agent(task: &Task) -> bool {
    task.kind() == TaskKind::Chat && !matches!(task.lifecycle_status, LifecycleStatus::Removed)
}

fn is_open_chat_task(task: &Task) -> bool {
    task.kind() == TaskKind::Chat
        && !matches!(
            task.lifecycle_status,
            LifecycleStatus::Merged | LifecycleStatus::Removed
        )
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
        CREATE_PR => Some(format!(
            "Create a pull request for task {handle} (base branch {base}). Commit any uncommitted \
             work on this branch, push the branch, then open a pull request for it with \
             `gh pr create`: use a conventional-commit title (type(scope): summary) and a body \
             that summarizes the change. Report the pull request URL when done."
        )),
        _ => None,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemediationOutcome {
    pub state_changed: bool,
    pub output: String,
}

pub fn chat_remediation_brief<R: Registry>(
    context: &CommandContext<R>,
    task_handle: &str,
    remediation_id: &str,
    skill_path: impl FnOnce(&str) -> Option<String>,
) -> Result<(Task, String), RemediationError> {
    let skill = if remediation_id == CREATE_PR {
        None
    } else {
        Some(
            skill_name(remediation_id)
                .ok_or_else(|| RemediationError::UnknownRemediation(remediation_id.to_string()))?,
        )
    };
    let task = find_task(context, task_handle)?;
    let skill_path = match skill {
        Some(skill_name) => {
            skill_path(skill_name).ok_or(RemediationError::UnsupportedCapability(
                "required agent skill is not installed on the companion host",
            ))?
        }
        None => String::new(),
    };
    let brief = format_brief(remediation_id, task, &skill_path)
        .ok_or_else(|| RemediationError::UnknownRemediation(remediation_id.to_string()))?;
    Ok((task.clone(), brief))
}

fn find_task<'a, R: Registry>(
    context: &'a CommandContext<R>,
    task_handle: &str,
) -> Result<&'a Task, RemediationError> {
    context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == task_handle)
        .ok_or_else(|| RemediationError::TaskNotFound(task_handle.to_string()))
}

fn skill_name(remediation_id: &str) -> Option<&'static str> {
    match remediation_id {
        FIX_CI => Some("gh-fix-ci"),
        RESOLVE_MERGE_CONFLICTS => Some("resolve-merge-conflicts"),
        _ => None,
    }
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

    let task = find_task(context, task_handle)?;

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

    let spec = tmux.send_agent_command(&task.tmux_session, &task.task_window, &brief);
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
        chat_remediation_brief, execute_remediation, format_brief, remediations_for_task,
        CREATE_PR, FIX_CI, MERGE_PR, RESOLVE_MERGE_CONFLICTS, RESTART_AGENT,
    };
    use crate::{
        adapters::RecordingCommandRunner,
        commands::CommandContext,
        config::{Config, ManagedRepo},
        diff_review::{remember_pull_requests, PullRequestRef, PullRequestState},
        models::{
            AgentClient, LifecycleStatus, LiveObservation, LiveStatusKind, SideFlag, Task, TaskId,
            TmuxStatus,
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
            "task",
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
    fn chat_task_without_open_pr_offers_create_pr() {
        let mut task = task("chat");
        task.set_skip_interactive_agent(true);

        let options = remediations_for_task(&task);

        let ids: Vec<&str> = options.iter().map(|option| option.id.as_str()).collect();
        assert_eq!(ids, vec![CREATE_PR, RESTART_AGENT]);
        assert_eq!(options[0].label, "Create PR");
    }

    #[test]
    fn chat_task_with_open_pr_does_not_offer_create_pr() {
        let mut task = task("chat");
        task.set_skip_interactive_agent(true);
        remember_pull_requests(
            &mut task,
            &[PullRequestRef {
                number: 7,
                title: "feat(web): chat".to_string(),
                url: "https://github.com/mossipcams/ajax-cli/pull/7".to_string(),
                state: PullRequestState::Open,
                head_ref: "ajax/chat".to_string(),
                head_sha: None,
            }],
        );

        assert!(remediations_for_task(&task)
            .iter()
            .all(|option| option.id != CREATE_PR));
    }

    #[test]
    fn chat_task_with_open_pr_offers_merge_pr_regardless_of_ci() {
        let mut task = task("chat");
        task.set_skip_interactive_agent(true);
        task.live_status = Some(LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"));
        task.add_side_flag(SideFlag::TestsFailed);
        remember_pull_requests(
            &mut task,
            &[PullRequestRef {
                number: 7,
                title: "feat(web): chat".to_string(),
                url: "https://github.com/mossipcams/ajax-cli/pull/7".to_string(),
                state: PullRequestState::Open,
                head_ref: "ajax/chat".to_string(),
                head_sha: None,
            }],
        );

        let options = remediations_for_task(&task);

        assert!(options.iter().any(|option| option.id == MERGE_PR
            && option.label == "Merge PR"
            && option.skill_name.is_empty()));
        assert!(options.iter().all(|option| option.id != CREATE_PR));
    }

    #[test]
    fn restart_agent_is_offered_for_chat_tasks_that_are_not_removed() {
        let mut chat = task("chat");
        chat.set_skip_interactive_agent(true);
        assert!(remediations_for_task(&chat)
            .iter()
            .any(|option| option.id == RESTART_AGENT && option.label == "Restart agent"));

        let mut removed = task("removed");
        removed.set_skip_interactive_agent(true);
        removed.lifecycle_status = LifecycleStatus::Removed;
        assert!(remediations_for_task(&removed)
            .iter()
            .all(|option| option.id != RESTART_AGENT));

        assert!(remediations_for_task(&task("terminal"))
            .iter()
            .all(|option| option.id != RESTART_AGENT));
    }

    #[test]
    fn chat_task_without_open_pr_does_not_offer_merge_pr() {
        let mut task = task("chat");
        task.set_skip_interactive_agent(true);

        assert!(remediations_for_task(&task)
            .iter()
            .all(|option| option.id != MERGE_PR));
    }

    #[test]
    fn terminal_task_never_offers_create_pr() {
        let task = task("terminal");

        assert!(remediations_for_task(&task)
            .iter()
            .all(|option| option.id != CREATE_PR));
    }

    #[test]
    fn merged_or_removed_chat_task_does_not_offer_create_pr() {
        for lifecycle in [LifecycleStatus::Merged, LifecycleStatus::Removed] {
            let mut task = task("done");
            task.set_skip_interactive_agent(true);
            task.lifecycle_status = lifecycle;

            assert!(remediations_for_task(&task)
                .iter()
                .all(|option| option.id != CREATE_PR));
        }
    }

    #[test]
    fn chat_task_whose_pr_just_merged_does_not_offer_create_pr_before_refresh() {
        let mut task = task("chat");
        task.set_skip_interactive_agent(true);
        task.lifecycle_status = LifecycleStatus::Active;
        remember_pull_requests(
            &mut task,
            &[PullRequestRef {
                number: 7,
                title: "feat(web): chat".to_string(),
                url: "https://github.com/mossipcams/ajax-cli/pull/7".to_string(),
                state: PullRequestState::Merged,
                head_ref: "ajax/chat".to_string(),
                head_sha: None,
            }],
        );

        let options = remediations_for_task(&task);

        assert!(options.iter().all(|option| option.id != CREATE_PR));
        assert!(options.iter().all(|option| option.id != MERGE_PR));
    }

    #[test]
    fn chat_task_whose_pr_was_closed_without_merge_offers_create_pr_again() {
        let mut task = task("chat");
        task.set_skip_interactive_agent(true);
        remember_pull_requests(
            &mut task,
            &[PullRequestRef {
                number: 7,
                title: "feat(web): chat".to_string(),
                url: "https://github.com/mossipcams/ajax-cli/pull/7".to_string(),
                state: PullRequestState::Closed,
                head_ref: "ajax/chat".to_string(),
                head_sha: None,
            }],
        );

        let options = remediations_for_task(&task);

        assert!(options.iter().any(|option| option.id == CREATE_PR));
        assert!(options.iter().all(|option| option.id != MERGE_PR));
    }

    #[test]
    fn chat_task_in_teardown_or_error_lifecycle_does_not_offer_create_pr() {
        for lifecycle in [
            LifecycleStatus::Removing,
            LifecycleStatus::TeardownIncomplete,
            LifecycleStatus::Error,
            LifecycleStatus::Orphaned,
        ] {
            let mut task = task("stuck");
            task.set_skip_interactive_agent(true);
            task.lifecycle_status = lifecycle;

            assert!(remediations_for_task(&task)
                .iter()
                .all(|option| option.id != CREATE_PR));
        }
    }

    #[test]
    fn chat_remediation_brief_for_create_pr_needs_no_skill() {
        let mut task = task("chat");
        task.set_skip_interactive_agent(true);
        let context = context_with_task(task);

        let (_, brief) = chat_remediation_brief(&context, "web/chat", CREATE_PR, |_| None).unwrap();

        assert!(brief.contains("gh pr create"));
        assert!(brief.contains("web/chat"));
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

    #[test]
    fn chat_remediation_brief_needs_no_tmux_session() {
        let mut task = task("chat");
        task.set_skip_interactive_agent(true);
        let context = context_with_task(task);

        let (task, brief) = chat_remediation_brief(&context, "web/chat", FIX_CI, |name| {
            assert_eq!(name, "gh-fix-ci");
            Some("/skills/gh-fix-ci/SKILL.md".to_string())
        })
        .unwrap();

        assert_eq!(task.qualified_handle(), "web/chat");
        assert_eq!(
            brief,
            format_brief(FIX_CI, &task, "/skills/gh-fix-ci/SKILL.md").unwrap()
        );
    }

    #[test]
    fn chat_remediation_brief_requires_installed_skill() {
        let context = context_with_task(task("chat"));

        let error = chat_remediation_brief(&context, "web/chat", RESOLVE_MERGE_CONFLICTS, |_| None)
            .unwrap_err();

        assert!(matches!(
            error,
            super::RemediationError::UnsupportedCapability(_)
        ));
    }
}
