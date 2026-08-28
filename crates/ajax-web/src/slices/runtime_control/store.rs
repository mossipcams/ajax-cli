//! Durable runtime-control state under `<host-clone>/.ajax-dev-web/`.

use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::LazyLock,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

pub(crate) const STATUS_FILE_NAME: &str = "runtime-control.json";
pub(crate) const LOG_FILE_NAME: &str = "runtime-control.log.jsonl";
pub(crate) const RUNTIME_STATUS_ENV: &str = "AJAX_RUNTIME_STATUS_FILE";
pub(crate) const RUNTIME_LOG_ENV: &str = "AJAX_RUNTIME_LOG_FILE";

static PROCESS_START: LazyLock<Instant> = LazyLock::new(Instant::now);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Restart,
    Update,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationPhase {
    Queued,
    Fetching,
    Building,
    Installing,
    Restarting,
    HealthCheck,
    Succeeded,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationResult {
    Succeeded,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationRecord {
    pub kind: OperationKind,
    pub phase: OperationPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<OperationResult>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rollback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuntimeControlState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<OperationRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

pub(crate) fn process_uptime_seconds() -> u64 {
    PROCESS_START.elapsed().as_secs()
}

pub fn host_runtime_dir_from_restart_script(script: &str) -> Option<PathBuf> {
    let scripts = Path::new(script).parent()?;
    let host_clone = scripts.parent()?;
    Some(host_clone.join(".ajax-dev-web"))
}

pub fn resolve_runtime_dir(
    restart_script_env: Option<&str>,
    cwd: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(script) = restart_script_env.filter(|value| !value.is_empty()) {
        if let Some(dir) = host_runtime_dir_from_restart_script(script) {
            return Some(dir);
        }
    }
    let cwd = cwd?;
    let discovered = crate::adapters::server::resolve_restart_script(None, Some(cwd))?;
    host_runtime_dir_from_restart_script(&discovered)
}

pub fn status_file_path(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join(STATUS_FILE_NAME)
}

pub fn log_file_path(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join(LOG_FILE_NAME)
}

pub fn read_state(runtime_dir: &Path) -> RuntimeControlState {
    let path = status_file_path(runtime_dir);
    let Ok(raw) = fs::read_to_string(&path) else {
        return RuntimeControlState::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn write_state(runtime_dir: &Path, state: &RuntimeControlState) -> Result<(), String> {
    fs::create_dir_all(runtime_dir).map_err(|error| format!("create runtime dir: {error}"))?;
    let path = status_file_path(runtime_dir);
    let body = serde_json::to_string_pretty(state)
        .map_err(|error| format!("serialize runtime state: {error}"))?;
    fs::write(&path, format!("{body}\n")).map_err(|error| format!("write runtime state: {error}"))
}

pub fn append_log_line(runtime_dir: &Path, line: &str) -> Result<(), String> {
    fs::create_dir_all(runtime_dir).map_err(|error| format!("create runtime dir: {error}"))?;
    let path = log_file_path(runtime_dir);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("open runtime log: {error}"))?;
    writeln!(file, "{line}").map_err(|error| format!("append runtime log: {error}"))
}

pub fn iso_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let (year, month, day, hour, minute, second) = unix_to_utc_parts(secs);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

// ponytail: hand-rolled UTC calendar for second-resolution timestamps; chrono if sub-second needed
fn unix_to_utc_parts(secs: i64) -> (i64, i64, i64, i64, i64, i64) {
    let mut days = secs.div_euclid(86_400);
    let time = secs.rem_euclid(86_400);
    let mut year = 1970_i64;
    while days >= if is_leap_year(year) { 366 } else { 365 } {
        days -= if is_leap_year(year) { 366 } else { 365 };
        year += 1;
    }
    let month_lengths = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1_i64;
    let mut day = days + 1;
    for length in month_lengths {
        if day <= length {
            break;
        }
        day -= length;
        month += 1;
    }
    (year, month, day, time / 3600, (time % 3600) / 60, time % 60)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn queue_operation(runtime_dir: &Path, kind: OperationKind) -> Result<OperationRecord, String> {
    let started_at = iso_now();
    let record = OperationRecord {
        kind,
        phase: OperationPhase::Queued,
        started_at: Some(started_at.clone()),
        finished_at: None,
        result: None,
        rollback: false,
    };
    let mut state = read_state(runtime_dir);
    state.operation = Some(record.clone());
    state.updated_at = Some(started_at);
    write_state(runtime_dir, &state)?;
    append_log_line(
        runtime_dir,
        &format!("queued {} operation", kind_label(kind)),
    )?;
    Ok(record)
}

pub fn kind_label(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::Restart => "restart",
        OperationKind::Update => "update",
    }
}

pub fn operation_is_active(state: &RuntimeControlState) -> bool {
    state.operation.as_ref().is_some_and(|op| {
        matches!(
            op.phase,
            OperationPhase::Queued
                | OperationPhase::Fetching
                | OperationPhase::Building
                | OperationPhase::Installing
                | OperationPhase::Restarting
                | OperationPhase::HealthCheck
        )
    })
}
