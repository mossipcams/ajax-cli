use ajax_supervisor::codex::CodexAdapter;
use clap::ArgMatches;

use crate::CliError;

pub(crate) fn render_supervise_command(matches: &ArgMatches) -> Result<String, CliError> {
    let prompt = matches
        .get_one::<String>("prompt")
        .cloned()
        .ok_or_else(|| CliError::CommandFailed("supervise prompt is required".to_string()))?;
    let codex_bin = matches
        .get_one::<String>("codex-bin")
        .cloned()
        .or_else(|| std::env::var("AJAX_CODEX_BIN").ok())
        .unwrap_or_else(|| "codex".to_string());
    let json = matches.get_flag("json");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|error| CliError::CommandFailed(format!("failed to start supervisor: {error}")))?;

    let (events, supervise_result) = runtime.block_on(async move {
        let adapter = CodexAdapter::new(codex_bin);
        let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
        let handle = tokio::spawn(async move { adapter.supervise_exec_json(&prompt, tx).await });
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        let supervise_result = match handle.await {
            Ok(result) => result.map_err(|error| format!("supervisor failed: {error}")),
            Err(error) => Err(format!("supervisor task failed: {error}")),
        };
        Ok::<_, CliError>((events, supervise_result))
    })?;
    if let Err(message) = supervise_result {
        return Err(CliError::CommandFailed(supervisor_failure_message(
            message, &events,
        )));
    }

    if json {
        events
            .iter()
            .map(|event| {
                serde_json::to_string(event)
                    .map_err(|error| CliError::JsonSerialization(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|lines| lines.join("\n"))
    } else {
        Ok(events
            .iter()
            .map(ajax_supervisor::renderer::render_event_log_line)
            .collect::<Vec<_>>()
            .join("\n"))
    }
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
