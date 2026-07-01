//! Browser task terminal attach planning.

use ajax_core::{
    adapters::CommandRunner,
    commands::CommandContext,
    registry::Registry,
    slices::pane::{self, PaneError},
};
use std::hash::{Hash, Hasher};

use crate::adapters::tmux_input::TmuxInputAdapter;

/// How many lines of scrollback the read-only mobile viewer captures.
pub const PANE_SNAPSHOT_LIMIT: usize = 200;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalAttachPlan {
    pub qualified_handle: String,
    pub tmux_session: String,
    pub worktrunk_window: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalRouteError {
    TaskNotFound,
    SessionMissing,
}

pub fn prepare_task_terminal<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<TerminalAttachPlan, TerminalRouteError> {
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or(TerminalRouteError::TaskNotFound)?;

    if task.tmux_session.trim().is_empty() {
        return Err(TerminalRouteError::SessionMissing);
    }

    Ok(TerminalAttachPlan {
        qualified_handle: qualified_handle.to_string(),
        tmux_session: task.tmux_session.clone(),
        worktrunk_window: task.worktrunk_window.clone(),
    })
}

/// Why a browser send-keys request could not be applied. Maps directly onto
/// HTTP status in the route handler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SendKeysRouteError {
    TaskNotFound,
    SessionMissing,
    InvalidKeys(String),
    Command(String),
}

/// Send free text (the mobile composer) or an allowed control token to a task's
/// tmux pane without opening the raw terminal socket. Reuses the same session
/// resolution as the attach path and the validated `send_keys` slice.
pub fn send_task_keys<R: Registry>(
    context: &CommandContext<R>,
    runner: &mut impl CommandRunner,
    qualified_handle: &str,
    keys: &str,
    submit: bool,
) -> Result<(), SendKeysRouteError> {
    let plan = prepare_task_terminal(context, qualified_handle).map_err(|error| match error {
        TerminalRouteError::TaskNotFound => SendKeysRouteError::TaskNotFound,
        TerminalRouteError::SessionMissing => SendKeysRouteError::SessionMissing,
    })?;

    TmuxInputAdapter
        .send_keys(runner, &plan.tmux_session, keys, submit)
        .map_err(|error| match error {
            PaneError::InvalidKeys(message) => SendKeysRouteError::InvalidKeys(message),
            PaneError::SessionMissing => SendKeysRouteError::SessionMissing,
            PaneError::CommandRun(run_error) => SendKeysRouteError::Command(run_error.to_string()),
        })?;

    Ok(())
}

/// Browser-facing pane snapshot for the read-only mobile viewer. `pane::PaneSnapshot`
/// isn't serializable, so map it to a flat, wire-friendly shape.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct TaskPaneSnapshotView {
    /// False when `since` matched the current content; `lines` is then empty so
    /// pollers transfer nothing on an idle pane.
    pub sequence_changed: bool,
    pub lines: Vec<String>,
    pub truncated: bool,
    /// Deterministic content fingerprint; echo it back as `since` to poll.
    pub sequence: String,
    /// A short human status hint parsed from the pane, when recognized.
    pub summary: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapshotRouteError {
    TaskNotFound,
    SessionMissing,
    Command(String),
}

fn fingerprint_lines(lines: &[String]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    lines.len().hash(&mut hasher);
    for line in lines {
        line.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

/// Capture the current pane for the read-only viewer. Returns `sequence_changed:
/// false` with no lines when `since` matches the current fingerprint, so idle
/// polling stays cheap. Alt-screen agents are captured fine because this reads
/// the visible pane rather than xterm scrollback.
pub fn task_pane_snapshot<R: Registry>(
    context: &CommandContext<R>,
    runner: &mut impl CommandRunner,
    qualified_handle: &str,
    since: Option<&str>,
    limit: usize,
) -> Result<TaskPaneSnapshotView, SnapshotRouteError> {
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or(SnapshotRouteError::TaskNotFound)?;

    if task.tmux_session.trim().is_empty() {
        return Err(SnapshotRouteError::SessionMissing);
    }

    let snapshot = pane::snapshot(runner, &task.tmux_session, task.selected_agent, None, limit)
        .map_err(|error| match error {
            PaneError::SessionMissing => SnapshotRouteError::SessionMissing,
            PaneError::CommandRun(run_error) => SnapshotRouteError::Command(run_error.to_string()),
            PaneError::InvalidKeys(message) => SnapshotRouteError::Command(message),
        })?;

    let sequence = fingerprint_lines(&snapshot.lines);
    let changed = since != Some(sequence.as_str());
    let summary = snapshot
        .state
        .as_ref()
        .map(|state| state.summary.clone())
        .filter(|summary| !summary.is_empty());

    Ok(TaskPaneSnapshotView {
        sequence_changed: changed,
        lines: if changed { snapshot.lines } else { Vec::new() },
        truncated: snapshot.truncated,
        sequence,
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ajax_core::adapters::{
        CommandOutput, CommandRunError, CommandSpec, RecordingCommandRunner,
    };

    /// Returns the same canned pane capture on every run so repeated snapshots
    /// produce an identical fingerprint.
    #[derive(Default)]
    struct StubRunner {
        stdout: String,
    }

    impl StubRunner {
        fn new(stdout: &str) -> Self {
            Self {
                stdout: stdout.to_string(),
            }
        }
    }

    impl CommandRunner for StubRunner {
        fn run(&mut self, _command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            Ok(CommandOutput {
                status_code: 0,
                stdout: self.stdout.clone(),
                stderr: String::new(),
            })
        }
    }
    use ajax_core::{
        config::{Config, ManagedRepo},
        models::{AgentClient, Task, TaskId},
        registry::InMemoryRegistry,
    };

    fn context_with_task() -> CommandContext<InMemoryRegistry> {
        let config = Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        };
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(Task::new(
                TaskId::new("web/fix-login"),
                "web",
                "fix-login",
                "Fix login",
                "ajax/fix-login",
                "main",
                "/repo/web__worktrees/ajax-fix-login",
                "ajax-web-fix-login",
                "worktrunk",
                AgentClient::Codex,
            ))
            .unwrap();
        CommandContext::new(config, registry)
    }

    fn context_with_empty_session_task() -> CommandContext<InMemoryRegistry> {
        let config = Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        };
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(Task::new(
                TaskId::new("web/fix-login"),
                "web",
                "fix-login",
                "Fix login",
                "ajax/fix-login",
                "main",
                "/repo/web__worktrees/ajax-fix-login",
                "",
                "worktrunk",
                AgentClient::Codex,
            ))
            .unwrap();
        CommandContext::new(config, registry)
    }

    #[test]
    fn prepare_task_terminal_returns_registered_session_and_worktrunk_target() {
        let context = context_with_task();

        let plan = prepare_task_terminal(&context, "web/fix-login").expect("plan");

        assert_eq!(plan.qualified_handle, "web/fix-login");
        assert_eq!(plan.tmux_session, "ajax-web-fix-login");
        assert_eq!(plan.worktrunk_window, "worktrunk");
    }

    #[test]
    fn prepare_task_terminal_returns_task_not_found_for_unknown_handle() {
        let context = context_with_task();

        let error = prepare_task_terminal(&context, "web/missing").unwrap_err();

        assert_eq!(error, TerminalRouteError::TaskNotFound);
    }

    #[test]
    fn prepare_task_terminal_returns_session_missing_for_empty_tmux_session() {
        let context = context_with_empty_session_task();

        let error = prepare_task_terminal(&context, "web/fix-login").unwrap_err();

        assert_eq!(error, TerminalRouteError::SessionMissing);
    }

    #[test]
    fn send_task_keys_sends_literal_text_and_enter() {
        let context = context_with_task();
        let mut runner = RecordingCommandRunner::default();

        send_task_keys(&context, &mut runner, "web/fix-login", "approve it", true).unwrap();

        assert_eq!(
            runner.commands()[0].args,
            vec![
                "send-keys",
                "-t",
                "ajax-web-fix-login:worktrunk",
                "approve it",
                "Enter",
            ]
        );
    }

    #[test]
    fn send_task_keys_omits_enter_when_not_submitting() {
        let context = context_with_task();
        let mut runner = RecordingCommandRunner::default();

        send_task_keys(&context, &mut runner, "web/fix-login", "C-c", false).unwrap();

        assert_eq!(
            runner.commands()[0].args,
            vec!["send-keys", "-t", "ajax-web-fix-login:worktrunk", "C-c"]
        );
    }

    #[test]
    fn send_task_keys_rejects_unknown_task() {
        let context = context_with_task();
        let mut runner = RecordingCommandRunner::default();

        let error = send_task_keys(&context, &mut runner, "web/missing", "hi", true).unwrap_err();

        assert_eq!(error, SendKeysRouteError::TaskNotFound);
        assert!(runner.commands().is_empty());
    }

    #[test]
    fn send_task_keys_rejects_missing_session() {
        let context = context_with_empty_session_task();
        let mut runner = RecordingCommandRunner::default();

        let error = send_task_keys(&context, &mut runner, "web/fix-login", "hi", true).unwrap_err();

        assert_eq!(error, SendKeysRouteError::SessionMissing);
        assert!(runner.commands().is_empty());
    }

    #[test]
    fn send_task_keys_rejects_disallowed_key_token() {
        let context = context_with_task();
        let mut runner = RecordingCommandRunner::default();

        let error =
            send_task_keys(&context, &mut runner, "web/fix-login", "C-x", false).unwrap_err();

        assert!(matches!(error, SendKeysRouteError::InvalidKeys(_)));
    }

    #[test]
    fn task_pane_snapshot_returns_lines_and_marks_change_on_first_capture() {
        let context = context_with_task();
        let mut runner = StubRunner::new("line one\nline two\n");

        let snapshot = task_pane_snapshot(
            &context,
            &mut runner,
            "web/fix-login",
            None,
            PANE_SNAPSHOT_LIMIT,
        )
        .unwrap();

        assert!(snapshot.sequence_changed);
        assert_eq!(snapshot.lines, vec!["line one", "line two"]);
        assert!(!snapshot.sequence.is_empty());
    }

    #[test]
    fn task_pane_snapshot_reports_no_change_when_since_matches() {
        let context = context_with_task();
        let mut runner = StubRunner::new("steady state\n");

        let first = task_pane_snapshot(
            &context,
            &mut runner,
            "web/fix-login",
            None,
            PANE_SNAPSHOT_LIMIT,
        )
        .unwrap();
        let second = task_pane_snapshot(
            &context,
            &mut runner,
            "web/fix-login",
            Some(&first.sequence),
            PANE_SNAPSHOT_LIMIT,
        )
        .unwrap();

        assert!(!second.sequence_changed);
        assert!(second.lines.is_empty());
        assert_eq!(second.sequence, first.sequence);
    }

    #[test]
    fn task_pane_snapshot_rejects_unknown_task() {
        let context = context_with_task();
        let mut runner = StubRunner::new("ignored\n");

        let error = task_pane_snapshot(
            &context,
            &mut runner,
            "web/missing",
            None,
            PANE_SNAPSHOT_LIMIT,
        )
        .unwrap_err();

        assert_eq!(error, SnapshotRouteError::TaskNotFound);
    }

    #[test]
    fn task_pane_snapshot_rejects_missing_session() {
        let context = context_with_empty_session_task();
        let mut runner = StubRunner::new("ignored\n");

        let error = task_pane_snapshot(
            &context,
            &mut runner,
            "web/fix-login",
            None,
            PANE_SNAPSHOT_LIMIT,
        )
        .unwrap_err();

        assert_eq!(error, SnapshotRouteError::SessionMissing);
    }
}
