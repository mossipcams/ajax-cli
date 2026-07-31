use std::{
    fs, io,
    path::{Path, PathBuf},
};

use clap::ArgMatches;
use serde_json::{Map, Value};

use crate::CliError;

const AGENT_EVENT_MARKER: &str = "ajax-cli __agent-event";

pub(crate) fn run_agent_hooks_command(matches: &ArgMatches) -> Result<String, CliError> {
    match matches.subcommand() {
        Some(("install", _)) => {
            let home = std::env::var("HOME")
                .map(PathBuf::from)
                .map_err(|_| CliError::CommandFailed("HOME is not set".to_string()))?;
            install_agent_hooks(&home)
        }
        _ => Err(CliError::CommandFailed(
            "unknown agent-hooks subcommand; try `agent-hooks install`".to_string(),
        )),
    }
}

pub(crate) fn install_agent_hooks(home: &Path) -> Result<String, CliError> {
    let claude = install_claude_hooks(home)?;
    let codex = install_codex_hooks(home)?;
    let cursor = install_cursor_hooks(home)?;
    let pi = install_pi_extension(home)?;
    Ok(format!(
        "claude: {claude}\ncodex: {codex}\ncursor: {cursor}\npi: {pi}"
    ))
}

fn install_claude_hooks(home: &Path) -> Result<&'static str, CliError> {
    let path = home.join(".claude/settings.json");
    let mut root = read_json_or_empty(&path)?;
    let events = [
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "Stop",
        "StopFailure",
        "SessionStart",
        "SessionEnd",
    ];
    // Every matcher here must have a `translate_native_event` arm; an installed
    // hook with no arm spawns `__agent-event` per notification and drops it.
    // `auth_success` is deliberately absent: it carries no status Ajax projects.
    let notification_matchers = [
        "permission_prompt",
        "idle_prompt",
        "elicitation_dialog",
        "agent_needs_input",
        "agent_completed",
    ];
    let mut changed = false;
    let legacy_notification_command = hook_command("claude", "Notification");
    if purge_legacy_hook_command(&mut root, "Notification", &legacy_notification_command) {
        changed = true;
    }
    for event in events {
        let command = hook_command("claude", event);
        if merge_hook_entries(&mut root, event, &command) {
            changed = true;
        }
    }
    for matcher in notification_matchers {
        let command = hook_command_matched("claude", "Notification", matcher);
        if merge_matched_hook_entries(&mut root, "Notification", matcher, &command) {
            changed = true;
        }
    }
    if changed {
        write_json(&path, &root)?;
        Ok("installed")
    } else {
        Ok("already installed")
    }
}

fn install_codex_hooks(home: &Path) -> Result<&'static str, CliError> {
    let path = home.join(".codex/hooks.json");
    let mut root = read_json_or_empty(&path)?;
    let events = [
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PermissionRequest",
        "Stop",
        "SessionStart",
        "SessionEnd",
    ];
    let mut changed = false;
    for event in events {
        let command = hook_command("codex", event);
        if merge_hook_entries(&mut root, event, &command) {
            changed = true;
        }
    }
    if changed {
        write_json(&path, &root)?;
        Ok("installed")
    } else {
        Ok("already installed")
    }
}

fn install_cursor_hooks(home: &Path) -> Result<&'static str, CliError> {
    let path = home.join(".cursor/hooks.json");
    fs::create_dir_all(path.parent().ok_or_else(|| {
        CliError::CommandFailed("cursor hooks path has no parent directory".to_string())
    })?)
    .map_err(io_error)?;

    let mut root = read_json_or_empty(&path)?;
    ensure_cursor_version(&mut root);
    let events = [
        "beforeSubmitPrompt",
        "preToolUse",
        "postToolUse",
        "postToolUseFailure",
        "subagentStart",
        "subagentStop",
        "stop",
        "sessionStart",
        "sessionEnd",
    ];
    let mut changed = false;
    for event in events {
        let command = hook_command("cursor", event);
        if merge_cursor_hook_entry(&mut root, event, &command) {
            changed = true;
        }
    }
    for matcher in ["permission_prompt", "elicitation_dialog"] {
        let command = hook_command_matched("cursor", "Notification", matcher);
        if merge_cursor_matched_hook_entry(&mut root, "Notification", matcher, &command) {
            changed = true;
        }
    }
    let elicitation_result_command = hook_command("cursor", "ElicitationResult");
    if merge_cursor_hook_entry(&mut root, "ElicitationResult", &elicitation_result_command) {
        changed = true;
    }
    if changed {
        write_json(&path, &root)?;
        Ok("installed")
    } else {
        Ok("already installed")
    }
}

fn install_pi_extension(home: &Path) -> Result<&'static str, CliError> {
    let path = home.join(".pi/agent/extensions/ajax-agent-events.ts");
    fs::create_dir_all(path.parent().ok_or_else(|| {
        CliError::CommandFailed("pi extension path has no parent directory".to_string())
    })?)
    .map_err(io_error)?;

    let content = pi_extension_source();
    let existed = path.exists();
    let unchanged = existed
        && fs::read_to_string(&path)
            .map(|existing| existing == content)
            .unwrap_or(false);
    if !unchanged {
        fs::write(&path, content).map_err(io_error)?;
        Ok("installed")
    } else {
        Ok("already installed")
    }
}

fn hook_command(client: &str, event: &str) -> String {
    format!("{AGENT_EVENT_MARKER} --client {client} --event {event}")
}

fn hook_command_matched(client: &str, event: &str, matcher: &str) -> String {
    format!("{AGENT_EVENT_MARKER} --client {client} --event {event}:{matcher}")
}

fn pi_extension_source() -> &'static str {
    r#"export default function (pi) {
  pi.on("before_agent_start", async () => {
    try {
      await pi.exec("ajax-cli", ["__agent-event", "--client", "pi", "--event", "before_agent_start"]);
    } catch {}
  });
  pi.on("agent_settled", async () => {
    try {
      await pi.exec("ajax-cli", ["__agent-event", "--client", "pi", "--event", "agent_settled"]);
    } catch {}
  });
}
"#
}

fn read_json_or_empty(path: &Path) -> Result<Value, CliError> {
    if !path.exists() {
        return Ok(Value::Object(Map::new()));
    }
    let contents = fs::read_to_string(path).map_err(io_error)?;
    serde_json::from_str(&contents).map_err(|error| {
        CliError::CommandFailed(format!("failed to parse {}: {error}", path.display()))
    })
}

fn write_json(path: &Path, value: &Value) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let encoded = serde_json::to_string_pretty(value)
        .map_err(|error| CliError::JsonSerialization(error.to_string()))?;
    fs::write(path, format!("{encoded}\n")).map_err(io_error)
}

fn ensure_cursor_version(root: &mut Value) {
    let Some(object) = root.as_object_mut() else {
        *root = Value::Object(Map::new());
        ensure_cursor_version(root);
        return;
    };
    if !object.contains_key("version") {
        object.insert("version".to_string(), Value::Number(1.into()));
    }
}

fn merge_hook_entries(root: &mut Value, event: &str, command: &str) -> bool {
    let object = root.as_object_mut().expect("root json object");
    if !object.contains_key("hooks") {
        object.insert("hooks".to_string(), Value::Object(Map::new()));
    }
    let hooks = object
        .get_mut("hooks")
        .and_then(Value::as_object_mut)
        .expect("hooks object");
    if !hooks.contains_key(event) {
        hooks.insert(event.to_string(), Value::Array(Vec::new()));
    }
    let entries = hooks
        .get_mut(event)
        .and_then(Value::as_array_mut)
        .expect("event hook array");
    if hook_command_present(entries, command) {
        return false;
    }
    entries.push(serde_json::json!({
        "hooks": [
            {
                "type": "command",
                "command": command
            }
        ]
    }));
    true
}

fn merge_matched_hook_entries(root: &mut Value, event: &str, matcher: &str, command: &str) -> bool {
    let object = root.as_object_mut().expect("root json object");
    if !object.contains_key("hooks") {
        object.insert("hooks".to_string(), Value::Object(Map::new()));
    }
    let hooks = object
        .get_mut("hooks")
        .and_then(Value::as_object_mut)
        .expect("hooks object");
    if !hooks.contains_key(event) {
        hooks.insert(event.to_string(), Value::Array(Vec::new()));
    }
    let entries = hooks
        .get_mut(event)
        .and_then(Value::as_array_mut)
        .expect("event hook array");
    if hook_command_present(entries, command) {
        return false;
    }
    entries.push(serde_json::json!({
        "matcher": matcher,
        "hooks": [
            {
                "type": "command",
                "command": command
            }
        ]
    }));
    true
}

fn purge_legacy_hook_command(root: &mut Value, event: &str, command: &str) -> bool {
    let object = root.as_object_mut().expect("root json object");
    let Some(hooks) = object.get_mut("hooks").and_then(Value::as_object_mut) else {
        return false;
    };
    let Some(entries) = hooks.get_mut(event).and_then(Value::as_array_mut) else {
        return false;
    };
    let before = entries.len();
    entries.retain(|entry| {
        !entry
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|hooks| {
                hooks.iter().any(|hook| {
                    hook.get("command")
                        .and_then(Value::as_str)
                        .is_some_and(|existing| existing == command)
                })
            })
    });
    entries.len() < before
}

fn merge_cursor_hook_entry(root: &mut Value, event: &str, command: &str) -> bool {
    ensure_cursor_version(root);
    let object = root.as_object_mut().expect("root json object");
    if !object.contains_key("hooks") {
        object.insert("hooks".to_string(), Value::Object(Map::new()));
    }
    let hooks = object
        .get_mut("hooks")
        .and_then(Value::as_object_mut)
        .expect("hooks object");
    if !hooks.contains_key(event) {
        hooks.insert(event.to_string(), Value::Array(Vec::new()));
    }
    let entries = hooks
        .get_mut(event)
        .and_then(Value::as_array_mut)
        .expect("event hook array");
    if cursor_command_present(entries, command) {
        return false;
    }
    entries.push(serde_json::json!({ "command": command }));
    true
}

fn merge_cursor_matched_hook_entry(
    root: &mut Value,
    event: &str,
    matcher: &str,
    command: &str,
) -> bool {
    ensure_cursor_version(root);
    let object = root.as_object_mut().expect("root json object");
    if !object.contains_key("hooks") {
        object.insert("hooks".to_string(), Value::Object(Map::new()));
    }
    let hooks = object
        .get_mut("hooks")
        .and_then(Value::as_object_mut)
        .expect("hooks object");
    if !hooks.contains_key(event) {
        hooks.insert(event.to_string(), Value::Array(Vec::new()));
    }
    let entries = hooks
        .get_mut(event)
        .and_then(Value::as_array_mut)
        .expect("event hook array");
    if cursor_matched_command_present(entries, matcher, command) {
        return false;
    }
    entries.push(serde_json::json!({ "matcher": matcher, "command": command }));
    true
}

fn hook_command_present(entries: &[Value], command: &str) -> bool {
    entries.iter().any(|entry| {
        entry
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|hooks| {
                hooks.iter().any(|hook| {
                    hook.get("command")
                        .and_then(Value::as_str)
                        .is_some_and(|existing| existing == command)
                })
            })
    })
}

fn cursor_command_present(entries: &[Value], command: &str) -> bool {
    entries.iter().any(|entry| {
        entry
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(|existing| existing == command)
    })
}

fn cursor_matched_command_present(entries: &[Value], matcher: &str, command: &str) -> bool {
    entries.iter().any(|entry| {
        entry.get("matcher").and_then(Value::as_str) == Some(matcher)
            && entry
                .get("command")
                .and_then(Value::as_str)
                .is_some_and(|existing| existing == command)
    })
}

fn io_error(error: io::Error) -> CliError {
    CliError::CommandFailed(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{hook_command, install_agent_hooks, AGENT_EVENT_MARKER};

    fn temp_home(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "ajax-agent-hooks-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn claude_events() -> [&'static str; 7] {
        [
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "Stop",
            "StopFailure",
            "SessionStart",
            "SessionEnd",
        ]
    }

    fn claude_notification_matchers() -> [&'static str; 5] {
        [
            "permission_prompt",
            "idle_prompt",
            "elicitation_dialog",
            "agent_needs_input",
            "agent_completed",
        ]
    }

    fn codex_events() -> [&'static str; 7] {
        [
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PermissionRequest",
            "Stop",
            "SessionStart",
            "SessionEnd",
        ]
    }

    fn cursor_events() -> [&'static str; 9] {
        [
            "beforeSubmitPrompt",
            "preToolUse",
            "postToolUse",
            "postToolUseFailure",
            "subagentStart",
            "subagentStop",
            "stop",
            "sessionStart",
            "sessionEnd",
        ]
    }

    fn assert_claude_hooks(home: &std::path::Path) {
        let settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(home.join(".claude/settings.json")).unwrap())
                .unwrap();
        for event in claude_events() {
            let command = hook_command("claude", event);
            let entries = settings["hooks"][event].as_array().unwrap();
            assert!(
                entries.iter().any(|entry| entry["hooks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|hook| hook["command"] == command)),
                "missing claude hook for {event}"
            );
            assert!(
                command.starts_with(AGENT_EVENT_MARKER),
                "claude hook command must include marker"
            );
        }
        for matcher in claude_notification_matchers() {
            let command =
                format!("{AGENT_EVENT_MARKER} --client claude --event Notification:{matcher}");
            let entries = settings["hooks"]["Notification"].as_array().unwrap();
            assert!(
                entries.iter().any(|entry| {
                    entry.get("matcher") == Some(&serde_json::Value::String(matcher.to_string()))
                        && entry["hooks"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|hook| hook["command"] == command)
                }),
                "missing matched claude Notification hook for {matcher}"
            );
        }
    }

    fn assert_codex_hooks(home: &std::path::Path) {
        let hooks: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(home.join(".codex/hooks.json")).unwrap())
                .unwrap();
        for event in codex_events() {
            let command = hook_command("codex", event);
            let entries = hooks["hooks"][event].as_array().unwrap();
            assert!(
                entries.iter().any(|entry| entry["hooks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|hook| hook["command"] == command)),
                "missing codex hook for {event}"
            );
        }
    }

    fn assert_cursor_hooks(home: &std::path::Path) {
        let hooks: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(home.join(".cursor/hooks.json")).unwrap())
                .unwrap();
        assert_eq!(hooks["version"], 1);
        for event in cursor_events() {
            let command = hook_command("cursor", event);
            let entries = hooks["hooks"][event].as_array().unwrap();
            assert!(
                entries.iter().any(|entry| entry["command"] == command),
                "missing cursor hook for {event}"
            );
        }
        for matcher in ["permission_prompt", "elicitation_dialog"] {
            let command =
                format!("{AGENT_EVENT_MARKER} --client cursor --event Notification:{matcher}");
            let entries = hooks["hooks"]["Notification"].as_array().unwrap();
            assert!(
                entries.iter().any(|entry| {
                    entry.get("matcher") == Some(&serde_json::Value::String(matcher.to_string()))
                        && entry["command"] == command
                }),
                "missing matched cursor Notification hook for {matcher}"
            );
        }
        let elicitation_result_command =
            format!("{AGENT_EVENT_MARKER} --client cursor --event ElicitationResult");
        let elicitation_entries = hooks["hooks"]["ElicitationResult"].as_array().unwrap();
        assert!(
            elicitation_entries
                .iter()
                .any(|entry| entry["command"] == elicitation_result_command),
            "missing cursor ElicitationResult hook"
        );
    }

    fn assert_pi_extension(home: &std::path::Path) {
        let path = home.join(".pi/agent/extensions/ajax-agent-events.ts");
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("before_agent_start"));
        assert!(content.contains("agent_settled"));
    }

    #[test]
    fn install_creates_all_configs_in_empty_home() {
        let home = temp_home("empty");
        fs::create_dir_all(&home).unwrap();

        install_agent_hooks(&home).unwrap();

        assert_claude_hooks(&home);
        assert_codex_hooks(&home);
        assert_cursor_hooks(&home);
        assert_pi_extension(&home);

        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn install_is_idempotent() {
        let home = temp_home("idempotent");
        fs::create_dir_all(&home).unwrap();

        install_agent_hooks(&home).unwrap();
        let claude_after_first = fs::read_to_string(home.join(".claude/settings.json")).unwrap();
        let codex_after_first = fs::read_to_string(home.join(".codex/hooks.json")).unwrap();
        let cursor_after_first = fs::read_to_string(home.join(".cursor/hooks.json")).unwrap();
        let pi_after_first =
            fs::read_to_string(home.join(".pi/agent/extensions/ajax-agent-events.ts")).unwrap();

        install_agent_hooks(&home).unwrap();

        assert_eq!(
            fs::read_to_string(home.join(".claude/settings.json")).unwrap(),
            claude_after_first
        );
        assert_eq!(
            fs::read_to_string(home.join(".codex/hooks.json")).unwrap(),
            codex_after_first
        );
        assert_eq!(
            fs::read_to_string(home.join(".cursor/hooks.json")).unwrap(),
            cursor_after_first
        );
        assert_eq!(
            fs::read_to_string(home.join(".pi/agent/extensions/ajax-agent-events.ts")).unwrap(),
            pi_after_first
        );

        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn install_preserves_existing_user_hooks() {
        let home = temp_home("preserve");
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(
            claude_dir.join("settings.json"),
            r#"{
  "model": "claude-sonnet-4",
  "hooks": {
    "PostToolUse": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "workmux set-window-status working"
          }
        ]
      }
    ]
  }
}"#,
        )
        .unwrap();

        install_agent_hooks(&home).unwrap();

        let settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(claude_dir.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(settings["model"], "claude-sonnet-4");
        let post_tool_use = settings["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(post_tool_use.len(), 2);
        assert_eq!(
            post_tool_use[0]["hooks"][0]["command"],
            "workmux set-window-status working"
        );
        let ajax_command = hook_command("claude", "PostToolUse");
        assert!(post_tool_use
            .iter()
            .any(|entry| entry["hooks"][0]["command"] == ajax_command));

        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn claude_install_writes_matched_notification_hooks() {
        let home = temp_home("claude-matched-notifications");
        fs::create_dir_all(&home).unwrap();

        install_agent_hooks(&home).unwrap();

        let settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(home.join(".claude/settings.json")).unwrap())
                .unwrap();
        let notification_types = [
            "permission_prompt",
            "idle_prompt",
            "elicitation_dialog",
            "agent_needs_input",
            "agent_completed",
        ];
        for matcher in notification_types {
            let command =
                format!("{AGENT_EVENT_MARKER} --client claude --event Notification:{matcher}");
            let entries = settings["hooks"]["Notification"]
                .as_array()
                .unwrap_or_else(|| panic!("missing Notification hooks array"));
            let matched = entries.iter().any(|entry| {
                entry["hooks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|hook| hook["command"] == command && hook.get("matcher").is_none())
                    && entry.get("matcher") == Some(&serde_json::Value::String(matcher.to_string()))
            });
            assert!(matched, "missing matched Notification hook for {matcher}");
        }

        let stop_failure_command = hook_command("claude", "StopFailure");
        let stop_failure_entries = settings["hooks"]["StopFailure"].as_array().unwrap();
        assert!(stop_failure_entries.iter().any(|entry| entry["hooks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hook| hook["command"] == stop_failure_command)));

        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn claude_install_purges_legacy_unmatched_notification() {
        let home = temp_home("claude-purge-legacy");
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let legacy_command = format!("{AGENT_EVENT_MARKER} --client claude --event Notification");
        fs::write(
            claude_dir.join("settings.json"),
            format!(
                r#"{{
  "hooks": {{
    "Notification": [
      {{
        "hooks": [
          {{
            "type": "command",
            "command": "{legacy_command}"
          }}
        ]
      }}
    ]
  }}
}}"#
            ),
        )
        .unwrap();

        install_agent_hooks(&home).unwrap();

        let settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(claude_dir.join("settings.json")).unwrap())
                .unwrap();
        let entries = settings["hooks"]["Notification"].as_array().unwrap();
        assert!(
            !entries.iter().any(|entry| entry["hooks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|hook| hook["command"] == legacy_command)),
            "legacy unmatched Notification hook must be purged"
        );
        assert!(
            entries.iter().any(|entry| entry.get("matcher").is_some()),
            "matched Notification hooks must exist after upgrade"
        );

        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn cursor_install_writes_failure_and_subagent_events() {
        let home = temp_home("cursor-failure-subagent");
        fs::create_dir_all(&home).unwrap();

        install_agent_hooks(&home).unwrap();

        let hooks: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(home.join(".cursor/hooks.json")).unwrap())
                .unwrap();
        for event in ["postToolUseFailure", "subagentStart", "subagentStop"] {
            let command = hook_command("cursor", event);
            let entries = hooks["hooks"][event].as_array().unwrap();
            assert!(
                entries.iter().any(|entry| entry["command"] == command),
                "missing cursor hook for {event}"
            );
        }

        fs::remove_dir_all(home).unwrap();
    }
}
