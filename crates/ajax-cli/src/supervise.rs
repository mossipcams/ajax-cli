use std::path::{Path, PathBuf};

use ajax_supervisor::{spawn_monitor, MonitorConfig, MonitorEvent, ProcessEvent, SupervisorAgent};
use clap::{parser::ValueSource, ArgMatches};

use crate::CliError;

const MAX_RETAINED_SUPERVISOR_EVENTS: usize = 256;

pub(crate) fn supervise_event_log_path(logs_dir: &Path) -> Option<PathBuf> {
    if supervise_event_log_enabled() {
        Some(logs_dir.join("monitor/events.jsonl"))
    } else {
        None
    }
}

fn supervise_event_log_enabled() -> bool {
    matches!(
        std::env::var_os("AJAX_SUPERVISE_EVENT_LOG").as_deref(),
        Some(value) if value == "1" || value == "true" || value == "on" || value == "yes"
    )
}

pub(crate) fn supervise_command_output_and_events(
    matches: &ArgMatches,
    task_agent: Option<SupervisorAgent>,
    logs_dir: &Path,
) -> Result<(String, Vec<MonitorEvent>), CliError> {
    let prompt = matches
        .get_one::<String>("prompt")
        .cloned()
        .ok_or_else(|| CliError::CommandFailed("supervise prompt is required".to_string()))?;
    let agent = resolve_supervisor_agent(matches, task_agent);
    let json = matches.get_flag("json");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|error| CliError::CommandFailed(format!("failed to start supervisor: {error}")))?;

    let (events, output, supervise_result) = runtime.block_on(async move {
        let mut config = match agent {
            "cursor" => MonitorConfig::cursor_exec(prompt),
            _ => MonitorConfig::codex_exec(prompt),
        };
        config.agent_bin = match agent {
            "cursor" => matches
                .get_one::<String>("cursor-bin")
                .cloned()
                .or_else(|| std::env::var("AJAX_CURSOR_BIN").ok())
                .unwrap_or_else(|| "cursor".to_string()),
            _ => matches
                .get_one::<String>("codex-bin")
                .cloned()
                .or_else(|| std::env::var("AJAX_CODEX_BIN").ok())
                .unwrap_or_else(|| "codex".to_string()),
        };
        config.event_log_path = supervise_event_log_path(logs_dir);
        let (handle, mut rx) =
            spawn_monitor(config).map_err(|error| CliError::CommandFailed(error.to_string()))?;
        let mut events = Vec::new();
        let mut output = String::new();
        while let Some(event) = rx.recv().await {
            if !output.is_empty() {
                output.push('\n');
            }
            let rendered = if json {
                serde_json::to_string(&event)
                    .map_err(|error| CliError::JsonSerialization(error.to_string()))?
            } else {
                ajax_supervisor::renderer::render_event_log_line(&event)
            };
            output.push_str(&rendered);
            retain_supervisor_event(&mut events, event);
        }
        let supervise_result = handle
            .wait()
            .await
            .map(|_| ())
            .map_err(|error| format!("supervisor failed: {error}"));
        Ok::<_, CliError>((events, output, supervise_result))
    })?;
    if let Err(message) = supervise_result {
        return Err(CliError::CommandFailed(supervisor_failure_message(
            message, &events,
        )));
    }

    Ok((output, events))
}

fn resolve_supervisor_agent(
    matches: &ArgMatches,
    task_agent: Option<SupervisorAgent>,
) -> &'static str {
    if matches.value_source("agent") != Some(ValueSource::DefaultValue) {
        return match matches.get_one::<String>("agent").map(String::as_str) {
            Some("cursor") => "cursor",
            _ => "codex",
        };
    }

    match task_agent {
        Some(SupervisorAgent::Cursor) => "cursor",
        Some(SupervisorAgent::Codex) | None => "codex",
    }
}

fn retain_supervisor_event(events: &mut Vec<MonitorEvent>, event: MonitorEvent) {
    if events.len() >= MAX_RETAINED_SUPERVISOR_EVENTS {
        let drop_index = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    MonitorEvent::Process(
                        ProcessEvent::Stdout { .. } | ProcessEvent::Stderr { .. }
                    )
                )
            })
            .unwrap_or(0);
        events.remove(drop_index);
    }
    events.push(event);
}

fn supervisor_failure_message(message: String, events: &[ajax_supervisor::MonitorEvent]) -> String {
    let stderr = events
        .iter()
        .filter_map(|event| match event {
            ajax_supervisor::MonitorEvent::Process(ajax_supervisor::ProcessEvent::Stderr {
                line,
            }) => Some(line.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    if stderr.is_empty() {
        return message;
    }

    format!("{message}; stderr: {}", stderr.join(" | "))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use ajax_supervisor::{MonitorEvent, ProcessEvent};

    #[test]
    fn supervise_event_log_path_respects_logs_dir_and_opt_in_env() {
        std::env::remove_var("AJAX_SUPERVISE_EVENT_LOG");
        let logs_dir = Path::new("/tmp/ajax-logs");
        assert_eq!(super::supervise_event_log_path(logs_dir), None);

        std::env::set_var("AJAX_SUPERVISE_EVENT_LOG", "1");
        assert_eq!(
            super::supervise_event_log_path(logs_dir),
            Some(logs_dir.join("monitor/events.jsonl"))
        );

        std::env::set_var("AJAX_SUPERVISE_EVENT_LOG", "0");
        assert_eq!(super::supervise_event_log_path(logs_dir), None);
        std::env::remove_var("AJAX_SUPERVISE_EVENT_LOG");
    }

    #[test]
    fn retained_supervisor_events_are_bounded_for_noisy_process_output() {
        let mut retained = Vec::new();

        for index in 0..300 {
            super::retain_supervisor_event(
                &mut retained,
                MonitorEvent::Process(ProcessEvent::Stdout {
                    line: format!("line {index}"),
                }),
            );
        }

        assert!(retained.len() <= super::MAX_RETAINED_SUPERVISOR_EVENTS);
        assert_eq!(
            retained.last(),
            Some(&MonitorEvent::Process(ProcessEvent::Stdout {
                line: "line 299".to_string()
            }))
        );
    }
}
