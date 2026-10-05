//! Training-host slice for the web training modal (plan:
//! `.planning/agent-plans/web-training-modal.md`).
//!
//! These routes proxy ssh verbs (`status`, `start <job>`, `stop`, `serve`,
//! `profile-set <name>`) to a remote LLM host. The host stays authoritative:
//! this slice validates only the request envelope (confirm flag, job name,
//! profile name) and adds no training policy of its own. Browser-supplied
//! text never reaches ssh argv except a profile name validated against both
//! the host-reported profiles list and `^[a-z0-9][a-z0-9.-]{0,63}$`.

pub mod runner;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

use self::runner::TrainingRunError;
pub use self::runner::{SshTrainingRunner, TrainingCommandRunner};
use crate::adapters::http::{json_response, response_from_web_error, Response};

/// Jobs the training host accepts for `start`. Anything else is rejected.
const TRAINING_JOBS: [&str; 4] = ["generate", "lfm-train", "lfm-eval", "wake-train"];

fn is_known_job(job: &str) -> bool {
    TRAINING_JOBS.contains(&job)
}

/// Profile names: `^[a-z0-9][a-z0-9.-]{0,63}$` (no regex crate dependency).
fn is_valid_profile_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    (1..=64).contains(&bytes.len())
        && matches!(bytes[0], b'a'..=b'z' | b'0'..=b'9')
        && bytes[1..]
            .iter()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-'))
}

/// One line of the host status object (`status` verb). Fields are mirrored to
/// the browser; `run` and `generation` may be null.
#[derive(Debug, Deserialize, Serialize)]
struct HostStatus {
    state: String,
    runtime_up: bool,
    #[serde(default)]
    active_profile: Option<String>,
    #[serde(default)]
    profiles: Vec<String>,
    run: Option<HostRun>,
    generation: Option<HostGeneration>,
    /// Optional per-profile details (`{name: {label, model, serving, sampling}}`);
    /// passed through verbatim and tolerant of any extra fields.
    #[serde(default)]
    profile_details: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct HostRun {
    kind: String,
    started: String,
    /// Whether the run is still active on the host (false for finished runs);
    /// passed through verbatim.
    #[serde(default)]
    running: bool,
    #[serde(default)]
    progress: Option<HostProgress>,
    #[serde(default)]
    log_tail: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct HostProgress {
    step: u64,
    total: u64,
    loss: Option<f64>,
    eta_s: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct HostGeneration {
    rows: u64,
    target: Option<u64>,
    running: bool,
}

fn json(status_code: u16, value: serde_json::Value) -> Response {
    json_response(status_code, value).unwrap_or_else(|error| response_from_web_error(error, None))
}

fn ok_response() -> Response {
    json(200, serde_json::json!({ "ok": true }))
}

fn bad_request(message: &str) -> Response {
    json(400, serde_json::json!({ "ok": false, "error": message }))
}

/// ssh failures/timeouts map to 502/504 with a short generic error. The runner
/// never embeds argv, hostnames, or key paths in its errors, and this keeps it
/// that way by construction.
fn run_error_response(error: TrainingRunError) -> Response {
    match error {
        TrainingRunError::Timeout => json(
            504,
            serde_json::json!({ "ok": false, "error": "training host timed out" }),
        ),
        TrainingRunError::Failed => json(
            502,
            serde_json::json!({ "ok": false, "error": "training host command failed" }),
        ),
    }
}

fn host_status(runner: &dyn TrainingCommandRunner) -> Result<HostStatus, Response> {
    let stdout = runner
        .run(&["status".to_string()])
        .map_err(run_error_response)?;
    serde_json::from_str::<HostStatus>(&stdout).map_err(|_| {
        json(
            502,
            serde_json::json!({ "ok": false, "error": "training host status is not valid JSON" }),
        )
    })
}
/// True while the host must not start a new job or switch profiles: a
/// `train:` gpu state, an active run (e.g. lfm-eval leaves state idle), or an
/// in-progress generation.
fn host_busy(status: &HostStatus) -> bool {
    status.state.starts_with("train:")
        || matches!(&status.run, Some(run) if run.running)
        || matches!(&status.generation, Some(generation) if generation.running)
}

/// Run a non-status verb whose every argument is built from constants plus, at
/// most, the validated profile name.
fn run_verb(runner: &dyn TrainingCommandRunner, args: &[&str]) -> Response {
    let argv: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    match runner.run(&argv) {
        Ok(_) => ok_response(),
        Err(error) => run_error_response(error),
    }
}

/// GET /api/training/status — the host status object reshaped with an `ok`
/// envelope; no browser input.
pub fn status_response(runner: &dyn TrainingCommandRunner) -> Response {
    match host_status(runner) {
        Ok(status) => json(
            200,
            serde_json::json!({
                "ok": true,
                "state": status.state,
                "runtime_up": status.runtime_up,
                "active_profile": status.active_profile,
                "run": status.run,
                "generation": status.generation,
                "profile_details": status.profile_details,
            }),
        ),
        Err(response) => response,
    }
}

/// GET /api/training/models — profiles + active + running. No browser input.
pub fn models_response(runner: &dyn TrainingCommandRunner) -> Response {
    match host_status(runner) {
        Ok(status) => json(
            200,
            serde_json::json!({
                "ok": true,
                "profiles": status.profiles,
                "active_profile": status.active_profile,
                "running": status.runtime_up,
            }),
        ),
        Err(response) => response,
    }
}

#[derive(Deserialize)]
struct StartBody {
    job: String,
    #[serde(default)]
    confirm: bool,
}

#[derive(Deserialize)]
struct ConfirmBody {
    #[serde(default)]
    confirm: bool,
}

#[derive(Deserialize)]
struct ProfileBody {
    profile: String,
    #[serde(default)]
    confirm: bool,
}

/// POST /api/training/start — body `{"job":"generate|lfm-train|lfm-eval|wake-train","confirm":true}`.
pub fn start_response(runner: &dyn TrainingCommandRunner, body: &[u8]) -> Response {
    let parsed: StartBody = match serde_json::from_slice(body) {
        Ok(parsed) => parsed,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if !parsed.confirm {
        return bad_request("confirm is required");
    }
    if !is_known_job(&parsed.job) {
        return bad_request("unknown job");
    }
    let status = match host_status(runner) {
        Ok(status) => status,
        Err(response) => return response,
    };
    if host_busy(&status) {
        return json(
            409,
            serde_json::json!({ "ok": false, "error": "a training, generation, or eval run is already in progress" }),
        );
    }
    run_verb(runner, &["start", parsed.job.as_str()])
}

/// POST /api/training/stop — body `{"confirm":true}`.
pub fn stop_response(runner: &dyn TrainingCommandRunner, body: &[u8]) -> Response {
    let parsed: ConfirmBody = match serde_json::from_slice(body) {
        Ok(parsed) => parsed,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if !parsed.confirm {
        return bad_request("confirm is required");
    }
    run_verb(runner, &["stop"])
}

/// POST /api/training/serve — body `{"confirm":true}`; starts llama with the
/// active profile. No browser input beyond the confirm flag.
pub fn serve_response(runner: &dyn TrainingCommandRunner, body: &[u8]) -> Response {
    let parsed: ConfirmBody = match serde_json::from_slice(body) {
        Ok(parsed) => parsed,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if !parsed.confirm {
        return bad_request("confirm is required");
    }
    run_verb(runner, &["serve"])
}

/// POST /api/training/models/switch — body `{"profile":"<name>","confirm":true}`.
/// The profile must match the name regex AND be present in the profiles list
/// last returned by the host; switching is refused (409) while a training run
/// or an active generation run is in progress.
pub fn switch_response(runner: &dyn TrainingCommandRunner, body: &[u8]) -> Response {
    let parsed: ProfileBody = match serde_json::from_slice(body) {
        Ok(parsed) => parsed,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if !parsed.confirm {
        return bad_request("confirm is required");
    }
    if !is_valid_profile_name(&parsed.profile) {
        return bad_request("invalid profile name");
    }
    let status = match host_status(runner) {
        Ok(status) => status,
        Err(response) => return response,
    };
    if !status.profiles.contains(&parsed.profile) {
        return bad_request("unknown profile");
    }
    if host_busy(&status) {
        return json(
            409,
            serde_json::json!({ "ok": false, "error": "profile switch is refused while training, eval, or generation is running" }),
        );
    }
    run_verb(runner, &["profile-set", parsed.profile.as_str()])
}
