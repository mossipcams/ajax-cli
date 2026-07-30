use std::{
    collections::hash_map::DefaultHasher,
    fs::{self, OpenOptions},
    hash::{Hash, Hasher},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use ajax_core::canonical_agent_event::CanonicalEventKind;
use serde::{Deserialize, Serialize};

use crate::CliError;
use clap::ArgMatches;

const DEFAULT_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);
const RUNTIME_HOOK_SETTLE_GRACE_MS: u128 = 15_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AgentRuntimeState {
    Starting,
    Running,
    ExitedSuccess,
    ExitedFailure,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub(crate) struct AgentRuntimeSnapshot {
    pub task_id: String,
    pub state: AgentRuntimeState,
    pub observed_at_unix_millis: u128,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub message: Option<String>,
}

pub(crate) fn run_agent_runtime(
    task_id: &str,
    state_root: &Path,
    program: &str,
    args: &[String],
) -> Result<i32, CliError> {
    let borrowed = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_agent_runtime_with_interval(
        task_id,
        state_root,
        program,
        &borrowed,
        DEFAULT_HEARTBEAT_INTERVAL,
    )
}

pub(crate) fn run_agent_runtime_command(matches: &ArgMatches) -> Result<String, CliError> {
    let task_id = matches
        .get_one::<String>("task-id")
        .map(String::as_str)
        .ok_or_else(|| CliError::CommandFailed("agent runtime task id is required".to_string()))?;
    let state_root = matches
        .get_one::<String>("state-root")
        .map(PathBuf::from)
        .ok_or_else(|| {
            CliError::CommandFailed("agent runtime state root is required".to_string())
        })?;
    let program = matches
        .get_one::<String>("program")
        .map(String::as_str)
        .ok_or_else(|| CliError::CommandFailed("agent runtime program is required".to_string()))?;
    let args = matches
        .get_many::<String>("agent-args")
        .map(|values| values.cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    let exit_code = run_agent_runtime(task_id, &state_root, program, &args)?;
    if exit_code == 0 {
        Ok(String::new())
    } else {
        Err(CliError::CommandFailed(format!(
            "agent exited with status {exit_code}"
        )))
    }
}

fn run_agent_runtime_with_interval(
    task_id: &str,
    state_root: &Path,
    program: &str,
    args: &[&str],
    heartbeat_interval: Duration,
) -> Result<i32, CliError> {
    write_runtime_snapshot(
        state_root,
        &AgentRuntimeSnapshot {
            task_id: task_id.to_string(),
            state: AgentRuntimeState::Starting,
            observed_at_unix_millis: now_millis()?,
            pid: None,
            exit_code: None,
            message: None,
        },
    )?;

    let agent_events_dir = state_root
        .parent()
        .unwrap_or(state_root)
        .join("agent-events");
    fs::create_dir_all(&agent_events_dir).map_err(|error| {
        CliError::CommandFailed(format!("failed to create agent events directory: {error}"))
    })?;
    let agent_events_dir = fs::canonicalize(&agent_events_dir).unwrap_or(agent_events_dir);
    let cwd = std::env::current_dir().map_err(|error| {
        CliError::CommandFailed(format!("failed to read current directory: {error}"))
    })?;
    publish_cwd_index(&agent_events_dir, task_id, "primary", &cwd)?;
    let child = Command::new(program)
        .args(args)
        .env("AJAX_TASK_ID", task_id)
        .env("AJAX_RUN_ID", "primary")
        .env("AJAX_AGENT_EVENTS_DIR", &agent_events_dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            let _ = clear_cwd_index(&agent_events_dir, &cwd);
            write_runtime_snapshot(
                state_root,
                &AgentRuntimeSnapshot {
                    task_id: task_id.to_string(),
                    state: AgentRuntimeState::ExitedFailure,
                    observed_at_unix_millis: now_millis()?,
                    pid: None,
                    exit_code: None,
                    message: Some(format!("failed to start agent: {error}")),
                },
            )?;
            return Err(CliError::CommandFailed(format!(
                "failed to start agent: {error}"
            )));
        }
    };
    let pid = child.id();

    loop {
        let status = child.try_wait().map_err(|error| {
            CliError::CommandFailed(format!("failed to observe agent: {error}"))
        })?;
        if let Some(status) = status {
            let exit_code = status.code();
            let state = if status.success() {
                AgentRuntimeState::ExitedSuccess
            } else {
                AgentRuntimeState::ExitedFailure
            };
            write_runtime_snapshot(
                state_root,
                &AgentRuntimeSnapshot {
                    task_id: task_id.to_string(),
                    state,
                    observed_at_unix_millis: now_millis()?,
                    pid: Some(pid),
                    exit_code,
                    message: None,
                },
            )?;
            // Keep cwd-index through the post-exit settle grace so Cursor
            // stop/sessionEnd can still resolve identity without AJAX_* env.
            // runtime_hooks_accepted rejects non-settle / stale writes.
            // Next publish for the same cwd overwrites the entry.
            return Ok(exit_code.unwrap_or(1));
        }

        write_runtime_snapshot(
            state_root,
            &AgentRuntimeSnapshot {
                task_id: task_id.to_string(),
                state: AgentRuntimeState::Running,
                observed_at_unix_millis: now_millis()?,
                pid: Some(pid),
                exit_code: None,
                message: None,
            },
        )?;
        thread::sleep(heartbeat_interval);
    }
}

fn write_runtime_snapshot(
    state_root: &Path,
    snapshot: &AgentRuntimeSnapshot,
) -> Result<(), CliError> {
    fs::create_dir_all(state_root).map_err(|error| {
        CliError::CommandFailed(format!("failed to create agent runtime directory: {error}"))
    })?;
    let encoded = serde_json::to_vec(snapshot)
        .map_err(|error| CliError::JsonSerialization(error.to_string()))?;
    let latest_path = snapshot_path(state_root, &snapshot.task_id);
    let temporary_path = state_root.join(format!(
        ".{}.tmp-{}",
        task_file_stem(&snapshot.task_id),
        std::process::id()
    ));
    fs::write(&temporary_path, &encoded).map_err(|error| {
        CliError::CommandFailed(format!("failed to write agent runtime snapshot: {error}"))
    })?;
    fs::rename(&temporary_path, &latest_path).map_err(|error| {
        CliError::CommandFailed(format!("failed to publish agent runtime snapshot: {error}"))
    })?;

    let history_path = state_root.join(format!("{}.jsonl", task_file_stem(&snapshot.task_id)));
    let mut history = OpenOptions::new()
        .create(true)
        .append(true)
        .open(history_path)
        .map_err(|error| {
            CliError::CommandFailed(format!("failed to open agent runtime history: {error}"))
        })?;
    history.write_all(&encoded).map_err(|error| {
        CliError::CommandFailed(format!("failed to append agent runtime history: {error}"))
    })?;
    history.write_all(b"\n").map_err(|error| {
        CliError::CommandFailed(format!("failed to append agent runtime history: {error}"))
    })?;
    history.flush().map_err(|error| {
        CliError::CommandFailed(format!("failed to flush agent runtime history: {error}"))
    })
}

fn snapshot_path(state_root: &Path, task_id: &str) -> PathBuf {
    state_root.join(format!("{}.json", task_file_stem(task_id)))
}

pub(crate) fn task_file_stem(task_id: &str) -> String {
    task_id.replace(['/', '\\'], "__")
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub(crate) struct CwdIndexEntry {
    pub task_id: String,
    pub run_id: String,
    pub events_dir: String,
    pub cwd: String,
}

const CWD_PATH_STEM_MAX_LEN: usize = 200;

pub(crate) fn cwd_path_stem(path: &str) -> String {
    if path.len() <= CWD_PATH_STEM_MAX_LEN {
        path.replace(['/', '\\', ':'], "__")
    } else {
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        format!("hash__{:x}", hasher.finish())
    }
}

pub(crate) fn cwd_index_path(agent_events_dir: &Path, cwd: &Path) -> PathBuf {
    let stem = cwd_path_stem(&cwd.to_string_lossy());
    agent_events_dir
        .join("cwd-index")
        .join(format!("{stem}.json"))
}

pub(crate) fn publish_cwd_index(
    agent_events_dir: &Path,
    task_id: &str,
    run_id: &str,
    cwd: &Path,
) -> Result<(), CliError> {
    let absolute_events_dir =
        fs::canonicalize(agent_events_dir).unwrap_or_else(|_| agent_events_dir.to_path_buf());
    fs::create_dir_all(&absolute_events_dir).map_err(|error| {
        CliError::CommandFailed(format!("failed to create agent events directory: {error}"))
    })?;
    let absolute_cwd = fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let entry = CwdIndexEntry {
        task_id: task_id.to_string(),
        run_id: run_id.to_string(),
        events_dir: absolute_events_dir.to_string_lossy().into_owned(),
        cwd: absolute_cwd.to_string_lossy().into_owned(),
    };
    let index_dir = absolute_events_dir.join("cwd-index");
    fs::create_dir_all(&index_dir).map_err(|error| {
        CliError::CommandFailed(format!("failed to create cwd index directory: {error}"))
    })?;
    let stem = cwd_path_stem(&entry.cwd);
    let index_path = index_dir.join(format!("{stem}.json"));
    let temporary_path = index_dir.join(format!(".{stem}.tmp-{}", std::process::id()));
    let encoded = serde_json::to_vec(&entry)
        .map_err(|error| CliError::JsonSerialization(error.to_string()))?;
    fs::write(&temporary_path, &encoded).map_err(|error| {
        CliError::CommandFailed(format!("failed to write cwd index entry: {error}"))
    })?;
    fs::rename(&temporary_path, &index_path).map_err(|error| {
        CliError::CommandFailed(format!("failed to publish cwd index entry: {error}"))
    })
}

pub(crate) fn clear_cwd_index(agent_events_dir: &Path, cwd: &Path) -> Result<(), CliError> {
    let absolute_events_dir =
        fs::canonicalize(agent_events_dir).unwrap_or_else(|_| agent_events_dir.to_path_buf());
    let index_path = cwd_index_path(&absolute_events_dir, cwd);
    if index_path.is_file() {
        fs::remove_file(&index_path).map_err(|error| {
            CliError::CommandFailed(format!("failed to clear cwd index entry: {error}"))
        })?;
    }
    Ok(())
}

pub(crate) fn now_millis() -> Result<u128, CliError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| CliError::CommandFailed(format!("system clock before epoch: {error}")))
}

pub(crate) fn runtime_hooks_accepted(
    events_dir: &Path,
    task_id: &str,
    event_kind: &CanonicalEventKind,
    now_millis: u128,
) -> bool {
    let is_post_exit_settle = matches!(
        event_kind,
        CanonicalEventKind::TurnSettled | CanonicalEventKind::SessionClosed
    );
    let runtime_root = match events_dir.parent() {
        Some(parent) => parent.join("agent-runtime"),
        None => return false,
    };
    let snapshot_path = runtime_root.join(format!("{}.json", task_file_stem(task_id)));
    let content = match fs::read_to_string(&snapshot_path) {
        Ok(content) => content,
        Err(_) => return false,
    };
    let snapshot: AgentRuntimeSnapshot = match serde_json::from_str(&content) {
        Ok(snapshot) => snapshot,
        Err(_) => return false,
    };

    match snapshot.state {
        AgentRuntimeState::Starting | AgentRuntimeState::Running => true,
        AgentRuntimeState::ExitedSuccess | AgentRuntimeState::ExitedFailure => {
            is_post_exit_settle
                && now_millis.saturating_sub(snapshot.observed_at_unix_millis)
                    <= RUNTIME_HOOK_SETTLE_GRACE_MS
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, time::Duration};

    use super::{
        clear_cwd_index, cwd_index_path, publish_cwd_index, run_agent_runtime_with_interval,
        snapshot_path, AgentRuntimeSnapshot, AgentRuntimeState,
    };

    fn temp_runtime_cache(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "ajax-agent-runtime-cache-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn temp_runtime_root(label: &str) -> std::path::PathBuf {
        let cache_root = temp_runtime_cache(label);
        let runtime_root = cache_root.join("agent-runtime");
        fs::create_dir_all(&runtime_root).unwrap();
        runtime_root
    }

    fn snapshots(root: &std::path::Path) -> Vec<AgentRuntimeSnapshot> {
        fs::read_to_string(root.join("web__fix-login.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn runtime_wrapper_records_start_heartbeat_and_successful_exit() {
        let root = temp_runtime_root("success");

        let exit_code = run_agent_runtime_with_interval(
            "web/fix-login",
            &root,
            "/bin/sh",
            &["-c", "sleep 0.03; exit 0"],
            Duration::from_millis(5),
        )
        .unwrap();

        assert_eq!(exit_code, 0);
        let history = snapshots(&root);
        assert_eq!(history.first().unwrap().state, AgentRuntimeState::Starting);
        assert!(history
            .iter()
            .any(|snapshot| snapshot.state == AgentRuntimeState::Running));
        assert_eq!(
            history.last().unwrap().state,
            AgentRuntimeState::ExitedSuccess
        );
        assert_eq!(history.last().unwrap().exit_code, Some(0));
        let latest: AgentRuntimeSnapshot = serde_json::from_str(
            &fs::read_to_string(snapshot_path(&root, "web/fix-login")).unwrap(),
        )
        .unwrap();
        assert_eq!(latest.state, AgentRuntimeState::ExitedSuccess);

        fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn runtime_wrapper_records_failed_exit_code() {
        let root = temp_runtime_root("failure");

        let exit_code = run_agent_runtime_with_interval(
            "web/fix-login",
            &root,
            "/bin/sh",
            &["-c", "exit 7"],
            Duration::from_millis(5),
        )
        .unwrap();

        assert_eq!(exit_code, 7);
        let latest = snapshots(&root).pop().unwrap();
        assert_eq!(latest.state, AgentRuntimeState::ExitedFailure);
        assert_eq!(latest.exit_code, Some(7));

        fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn runtime_wrapper_records_start_before_spawn_failure() {
        let root = temp_runtime_root("spawn-failure");

        let error = run_agent_runtime_with_interval(
            "web/fix-login",
            &root,
            "/definitely/missing/ajax-agent",
            &[],
            Duration::from_millis(5),
        )
        .unwrap_err();

        assert!(error.to_string().contains("failed to start agent"));
        let history = snapshots(&root);
        assert_eq!(history.first().unwrap().state, AgentRuntimeState::Starting);
        assert_eq!(
            history.last().unwrap().state,
            AgentRuntimeState::ExitedFailure
        );

        fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn runtime_wrapper_injects_identity_env() {
        let root = temp_runtime_root("identity-env");
        let env_file = root.join("child-env.txt");

        let exit_code = run_agent_runtime_with_interval(
            "web/fix-login",
            &root,
            "/bin/sh",
            &[
                "-c",
                &format!(
                    "printf '%s|%s|%s' \"$AJAX_TASK_ID\" \"$AJAX_RUN_ID\" \"$AJAX_AGENT_EVENTS_DIR\" > {}",
                    env_file.display()
                ),
            ],
            Duration::from_millis(5),
        )
        .unwrap();

        assert_eq!(exit_code, 0);
        let captured = fs::read_to_string(&env_file).unwrap();
        let expected_events_dir = root.parent().unwrap().join("agent-events");
        let expected_events_dir =
            fs::canonicalize(&expected_events_dir).unwrap_or(expected_events_dir);
        assert_eq!(
            captured,
            format!("web/fix-login|primary|{}", expected_events_dir.display())
        );

        fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn runtime_publishes_and_clears_cwd_index() {
        let cache_root = temp_runtime_cache("cwd-index");
        let events_dir = cache_root.join("agent-events");
        let root = cache_root.join("agent-runtime");
        fs::create_dir_all(&root).unwrap();
        let cwd = std::env::current_dir().unwrap();

        publish_cwd_index(&events_dir, "web/fix-login", "primary", &cwd).unwrap();
        let index_path = cwd_index_path(&events_dir, &cwd);
        assert!(index_path.is_file());
        let entry: super::CwdIndexEntry =
            serde_json::from_str(&fs::read_to_string(&index_path).unwrap()).unwrap();
        assert_eq!(entry.task_id, "web/fix-login");
        assert_eq!(entry.run_id, "primary");
        assert_eq!(entry.cwd, cwd.to_string_lossy());

        clear_cwd_index(&events_dir, &cwd).unwrap();
        assert!(!index_path.exists());

        let exit_code = run_agent_runtime_with_interval(
            "web/fix-login",
            &root,
            "/bin/sh",
            &["-c", "exit 0"],
            Duration::from_millis(5),
        )
        .unwrap();
        assert_eq!(exit_code, 0);
        // Index remains after exit so settle hooks can resolve identity.
        assert!(cwd_index_path(&events_dir, &cwd).is_file());

        fs::remove_dir_all(cache_root).unwrap();
    }
}
