pub mod runner;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

use self::runner::TrainingRunError;
pub use self::runner::{SshTrainingRunner, TrainingCommandRunner};
use crate::adapters::http::{json_response, response_from_web_error, Response};

const TRAINING_JOBS: [&str; 9] = [
    "generate",
    "lfm-train",
    "lfm-eval",
    "wake-train",
    "validate",
    "canary",
    "promote-dataset",
    "promote-model",
    "select-checkpoint",
];

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

fn is_valid_run_id(run_id: &str) -> bool {
    let bytes = run_id.as_bytes();
    bytes.len() == 25
        && bytes[..8].iter().all(u8::is_ascii_digit)
        && bytes[8] == b'T'
        && bytes[9..15].iter().all(u8::is_ascii_digit)
        && bytes[15] == b'Z'
        && bytes[16] == b'-'
        && bytes[17..]
            .iter()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
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
    #[serde(default)]
    device: Option<String>,
    #[serde(default)]
    pipeline: Option<serde_json::Value>,
    #[serde(default)]
    checkpoint_eval: bool,
    #[serde(default)]
    eval_progress: Vec<serde_json::Value>,
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
    #[serde(default)]
    external: bool,
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
        || !status.eval_progress.is_empty()
        || matches!(&status.run, Some(run) if run.running)
        || matches!(&status.generation, Some(generation) if generation.running)
}

fn host_document(
    runner: &dyn TrainingCommandRunner,
    verb: &str,
    key: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, Response> {
    let stdout = runner
        .run(&[verb.to_string()])
        .map_err(run_error_response)?;
    match serde_json::from_str::<serde_json::Value>(&stdout) {
        Ok(serde_json::Value::Object(document))
            if document.get(key).is_some_and(serde_json::Value::is_array) =>
        {
            Ok(document)
        }
        _ => Err(json(
            502,
            serde_json::json!({ "ok": false, "error": format!("training host {verb} is not valid JSON") }),
        )),
    }
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
                "profiles": status.profiles,
                "run": status.run,
                "generation": status.generation,
                "profile_details": status.profile_details,
                "device": status.device,
                "pipeline": status.pipeline,
                "checkpoint_eval": status.checkpoint_eval,
                "eval_progress": status.eval_progress,
            }),
        ),
        Err(response) => response,
    }
}

pub fn datasets_response(runner: &dyn TrainingCommandRunner) -> Response {
    match host_document(runner, "datasets", "datasets") {
        Ok(mut document) => json(
            200,
            serde_json::json!({
                "ok": true,
                "datasets": document.remove("datasets"),
                "promoted_run_id": document
                    .remove("promoted_run_id")
                    .unwrap_or(serde_json::Value::Null),
                "promoted_record_present": document
                    .remove("promoted_record_present")
                    .unwrap_or(serde_json::json!(false)),
            }),
        ),
        Err(response) => response,
    }
}

pub fn evals_response(runner: &dyn TrainingCommandRunner) -> Response {
    match host_document(runner, "evals", "evals") {
        Ok(mut document) => json(
            200,
            serde_json::json!({
                "ok": true,
                "evals": document.remove("evals"),
                "champion": document.remove("champion").unwrap_or(serde_json::Value::Null),
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

#[derive(Deserialize)]
struct DatasetBody {
    run_id: String,
    #[serde(default)]
    confirm: bool,
}

pub fn select_dataset_response(runner: &dyn TrainingCommandRunner, body: &[u8]) -> Response {
    let parsed: DatasetBody = match serde_json::from_slice(body) {
        Ok(parsed) => parsed,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if !parsed.confirm {
        return bad_request("confirm is required");
    }
    if !is_valid_run_id(&parsed.run_id) {
        return bad_request("invalid run id");
    }
    let status = match host_status(runner) {
        Ok(status) => status,
        Err(response) => return response,
    };
    if host_busy(&status) {
        return json(
            409,
            serde_json::json!({ "ok": false, "error": "dataset selection is refused while training, eval, or generation is running" }),
        );
    }
    let datasets = match host_document(runner, "datasets", "datasets") {
        Ok(document) => document,
        Err(response) => return response,
    };
    let known = datasets
        .get("datasets")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|entries| {
            entries.iter().any(|entry| {
                entry.get("run_id").and_then(serde_json::Value::as_str)
                    == Some(parsed.run_id.as_str())
            })
        });
    if !known {
        return bad_request("unknown dataset");
    }
    run_verb(runner, &["dataset-select", parsed.run_id.as_str()])
}
