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
        if let Some(cwd) = &command.cwd {
            format!(
                "$ (cd {} && {} {})",
                cwd,
                command.program,
                command.args.join(" ")
            )
        } else {
            format!("$ {} {}", command.program, command.args.join(" "))
        }
    }));

    lines.join("\n")
}
