pub mod runner;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

use self::runner::TrainingRunError;
pub use self::runner::{SshTrainingRunner, TrainingCommandRunner};
use crate::adapters::http::{json_response, response_from_web_error, Response};

const TRAINING_JOBS: [&str; 4] = ["generate", "lfm-train", "lfm-eval", "wake-train"];

fn is_known_job(job: &str) -> bool {
    TRAINING_JOBS.contains(&job)
}

fn is_valid_profile_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    (1..=64).contains(&bytes.len())
        && matches!(bytes[0], b'a'..=b'z' | b'0'..=b'9')
        && bytes[1..]
            .iter()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-'))
}

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
    #[serde(default)]
    profile_details: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct HostRun {
    kind: String,
    #[serde(default)]
    started: String,
    #[serde(default)]
    running: bool,
    #[serde(default)]
    progress: Option<HostProgress>,
    #[serde(default)]
    log_tail: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct HostProgress {
    #[serde(default)]
    step: u64,
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    loss: Option<f64>,
    #[serde(default)]
    eta_s: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct HostGeneration {
    #[serde(default)]
    rows: Option<u64>,
    #[serde(default)]
    target: Option<u64>,
    #[serde(default)]
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
fn host_busy(status: &HostStatus) -> bool {
    status.state.starts_with("train:")
        || matches!(&status.run, Some(run) if run.running)
        || matches!(&status.generation, Some(generation) if generation.running)
}

fn run_verb(runner: &dyn TrainingCommandRunner, args: &[&str]) -> Response {
    let argv: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    match runner.run(&argv) {
        Ok(_) => ok_response(),
        Err(error) => run_error_response(error),
    }
}

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
