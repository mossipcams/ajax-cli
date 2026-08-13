//! Parse `agent models` stdout into browser catalog entries.

use serde::Serialize;
use std::process::Command;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CursorModel {
    pub id: String,
    pub label: String,
}

pub fn list_cursor_models_sync() -> Result<Vec<CursorModel>, String> {
    let output = Command::new("agent")
        .arg("models")
        .output()
        .map_err(|error| format!("agent models failed: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "agent models exited with {}",
            output.status.code().unwrap_or(-1)
        ));
    }
    Ok(parse_models_stdout(
        std::str::from_utf8(&output.stdout).map_err(|error| error.to_string())?,
    ))
}

pub fn parse_models_stdout(stdout: &str) -> Vec<CursorModel> {
    stdout.lines().filter_map(parse_model_line).collect()
}

fn parse_model_line(line: &str) -> Option<CursorModel> {
    let line = line.trim();
    let (id, label) = line.split_once(" - ")?;
    let id = id.trim();
    let label = label.trim();
    if id.is_empty() || label.is_empty() {
        return None;
    }
    Some(CursorModel {
        id: id.to_string(),
        label: label.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE_MODELS: &str = "Available models\n\nauto - Auto\ncomposer-2.5 - Composer 2.5\n";

    #[test]
    fn parse_models_stdout_reads_fake_agent_catalog() {
        let models = parse_models_stdout(FAKE_MODELS);

        assert_eq!(
            models,
            vec![
                CursorModel {
                    id: "auto".to_string(),
                    label: "Auto".to_string(),
                },
                CursorModel {
                    id: "composer-2.5".to_string(),
                    label: "Composer 2.5".to_string(),
                },
            ]
        );
    }
}
