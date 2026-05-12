use ajax_core::{
    adapters::CommandOutput,
    commands,
    output::{
        DoctorResponse, InboxResponse, InspectResponse, NextResponse, ReposResponse, TaskSummary,
        TasksResponse,
    },
};
use serde::Serialize;

use crate::CliError;

pub(crate) fn render_response<T: Serialize>(
    response: T,
    json: bool,
    human: fn(&T) -> String,
) -> Result<String, CliError> {
    if json {
        serde_json::to_string_pretty(&response)
            .map_err(|error| CliError::JsonSerialization(error.to_string()))
    } else {
        Ok(human(&response))
    }
}

pub(crate) fn render_plan(plan: commands::CommandPlan, json: bool) -> Result<String, CliError> {
    render_response(plan, json, render_plan_human)
}

pub(crate) fn render_execution_outputs(
    outputs: &[CommandOutput],
    recorded_task: Option<&str>,
) -> String {
    let mut lines = outputs
        .iter()
        .map(|output| {
            format!(
                "exit:{}\nstdout:{}\nstderr:{}",
                output.status_code, output.stdout, output.stderr
            )
        })
        .collect::<Vec<_>>();

    if let Some(task) = recorded_task {
        lines.push(format!("recorded task: {task}"));
    }

    lines.join("\n")
}

pub(crate) fn render_repos_human(response: &ReposResponse) -> String {
    response
        .repos
        .iter()
        .map(|repo| {
            format!(
                "{}\t{}\tactive:{} reviewable:{} cleanable:{}",
                repo.name,
                repo.path,
                repo.active_tasks,
                repo.reviewable_tasks,
                repo.cleanable_tasks
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn render_tasks_human(response: &TasksResponse) -> String {
    response
        .tasks
        .iter()
        .map(render_task_summary)
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn render_inspect_human(response: &InspectResponse) -> String {
    format!(
        "{}\nbranch: {}\nworktree: {}\ntmux: {}\nflags: {}",
        render_task_summary(&response.task),
        response.branch,
        response.worktree_path,
        response.tmux_session,
        response.flags.join(", ")
    )
}

pub(crate) fn render_inbox_human(response: &InboxResponse) -> String {
    response
        .items
        .iter()
        .map(render_attention_item_human)
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn render_next_human(response: &NextResponse) -> String {
    response
        .item
        .as_ref()
        .map(render_attention_item_human)
        .unwrap_or_else(|| "no tasks need attention".to_string())
}

pub(crate) fn render_doctor_human(response: &DoctorResponse) -> String {
    response
        .checks
        .iter()
        .map(|check| format!("{}\t{}\t{}", check.name, check.ok, check.message))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_task_summary(task: &TaskSummary) -> String {
    format!(
        "{}\t{}\t{}",
        task.qualified_handle, task.lifecycle_status, task.title
    )
}

fn render_attention_item_human(item: &ajax_core::models::AttentionItem) -> String {
    format!(
        "{}: {} -> {}",
        item.task_handle, item.reason, item.recommended_action
    )
}

fn render_plan_human(plan: &commands::CommandPlan) -> String {
    let mut lines = vec![plan.title.clone()];

    if plan.requires_confirmation {
        lines.push("requires confirmation".to_string());
    }

    lines.extend(
        plan.blocked_reasons
            .iter()
            .map(|reason| format!("blocked: {reason}")),
    );
    lines.extend(plan.commands.iter().map(|command| {
        let command_line = shell_words(
            std::iter::once(command.program.as_str())
                .chain(command.args.iter().map(String::as_str)),
        );
        if let Some(cwd) = &command.cwd {
            let cwd = cwd.to_string_lossy();
            format!("$ (cd {} && {})", shell_quote(cwd.as_ref()), command_line)
        } else {
            format!("$ {command_line}")
        }
    }));

    lines.join("\n")
}

fn shell_words<'a>(words: impl Iterator<Item = &'a str>) -> String {
    words.map(shell_quote).collect::<Vec<_>>().join(" ")
}

fn shell_quote(word: &str) -> String {
    if word.is_empty() {
        return "''".to_string();
    }

    if word
        .bytes()
        .all(|byte| matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'@' | b'%' | b'+' | b'=' | b':' | b',' | b'.' | b'/' | b'-'))
    {
        return word.to_string();
    }

    format!("'{}'", word.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use ajax_core::{
        adapters::CommandSpec,
        commands::CommandPlan,
        models::{AttentionItem, TaskId},
        output::{InboxResponse, InspectResponse, TaskSummary},
    };

    use super::{render_inbox_human, render_inspect_human, render_plan, render_plan_human};

    #[test]
    fn render_plan_quotes_shell_words_for_copy_paste_safe_human_output() {
        let mut plan = CommandPlan::new("copy safe");
        plan.commands.push(
            CommandSpec::new(
                "my tool",
                ["hello world", "semi;colon", "it's", "$(danger)"],
            )
            .with_cwd("/tmp/ajax worktrees/feat;rm"),
        );

        let rendered = render_plan_human(&plan);

        assert_eq!(
            rendered,
            "copy safe\n$ (cd '/tmp/ajax worktrees/feat;rm' && 'my tool' 'hello world' 'semi;colon' 'it'\\''s' '$(danger)')"
        );
    }

    #[test]
    fn render_plan_json_remains_structured() {
        let mut plan = CommandPlan::new("copy safe");
        plan.commands.push(
            CommandSpec::new("my tool", ["hello world", "semi;colon"])
                .with_cwd("/tmp/ajax worktrees/feat;rm"),
        );

        let rendered = render_plan(plan, true).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&rendered).unwrap();

        assert_eq!(parsed["commands"][0]["program"], "my tool");
        assert_eq!(parsed["commands"][0]["args"][0], "hello world");
        assert_eq!(parsed["commands"][0]["cwd"], "/tmp/ajax worktrees/feat;rm");
    }

    #[test]
    fn inspect_human_renders_task_resource_details() {
        let rendered = render_inspect_human(&InspectResponse {
            task: TaskSummary {
                id: "task-1".to_string(),
                qualified_handle: "web/fix-login".to_string(),
                title: "Fix login".to_string(),
                lifecycle_status: "Reviewable".to_string(),
                needs_attention: true,
                live_status: None,
                actions: vec!["open task".to_string()],
            },
            branch: "ajax/fix-login".to_string(),
            worktree_path: "/tmp/worktrees/web-fix-login".to_string(),
            tmux_session: "ajax-web-fix-login".to_string(),
            flags: vec!["needs-input".to_string(), "dirty".to_string()],
        });

        assert_eq!(
            rendered,
            "web/fix-login\tReviewable\tFix login\nbranch: ajax/fix-login\nworktree: /tmp/worktrees/web-fix-login\ntmux: ajax-web-fix-login\nflags: needs-input, dirty"
        );
    }

    #[test]
    fn inbox_human_renders_attention_item_action_lines() {
        let rendered = render_inbox_human(&InboxResponse {
            items: vec![AttentionItem {
                task_id: TaskId::new("task-1"),
                task_handle: "web/fix-login".to_string(),
                reason: "agent needs input".to_string(),
                priority: 75,
                recommended_action: "open task".to_string(),
            }],
        });

        assert_eq!(rendered, "web/fix-login: agent needs input -> open task");
    }
}
