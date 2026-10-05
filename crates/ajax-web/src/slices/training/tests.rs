//! Tests for the training slice (split out of `mod.rs` to keep it within the
//! Rust file-size limit).

use std::sync::{Arc, Mutex};

use crate::slices::training::*;

mod test_support {
    use super::*;

    /// Records every argv vector it receives. `status` returns a fixed status
    /// JSON document; any other verb returns the configured outcome.
    pub(crate) struct FakeTrainingRunner {
        pub calls: Arc<Mutex<Vec<Vec<String>>>>,
        status_stdout: String,
        result: Result<(), TrainingRunError>,
    }

    impl FakeTrainingRunner {
        pub fn new(status_json: &str) -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                status_stdout: status_json.to_string(),
                result: Ok(()),
            }
        }

        /// Configure non-status verbs to fail with the given runner error.
        pub fn failing(self, error: TrainingRunError) -> Self {
            Self {
                result: Err(error),
                ..self
            }
        }

        pub fn recorded_calls(&self) -> Vec<Vec<String>> {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }
    }

    impl TrainingCommandRunner for FakeTrainingRunner {
        fn run(&self, args: &[String]) -> Result<String, TrainingRunError> {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(args.to_vec());
            if args.first().is_some_and(|verb| verb == "status") {
                return Ok(self.status_stdout.clone());
            }
            self.result.map(|()| String::new())
        }
    }

    pub(crate) fn status_json(
        state: &str,
        run: &serde_json::Value,
        generation: &serde_json::Value,
    ) -> String {
        serde_json::json!({
            "state": state,
            "runtime_up": true,
            "active_profile": "qwen3-4b",
            "profiles": ["qwen3-4b", "llama3-8b-instruct"],
            "run": run,
            "generation": generation,
        })
        .to_string()
    }

    pub(crate) const IDLE_STATUS: &str = r#"{"state":"idle","runtime_up":false,"active_profile":null,"profiles":["qwen3-4b"],"run":null,"generation":null}"#;
}

use test_support::{status_json, FakeTrainingRunner, IDLE_STATUS};

type Value = serde_json::Value;
const RUN: &str = r#"{"kind":"lfm-train","started":"2026-07-10T00:00:00Z","running":true,"progress":{"step":3,"total":10,"loss":0.42,"eta_s":12},"log_tail":["step 3/10"]}"#;
const RUN_DONE: &str = r#"{"kind":"lfm-train","started":"2026-07-10T00:00:00Z","running":false,"progress":{"step":10,"total":10,"loss":0.41,"eta_s":null},"log_tail":["done"]}"#;

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}
const GEN_ACTIVE: &str = r#"{"rows":512,"target":2048,"running":true}"#;
const GEN_IDLE: &str = r#"{"rows":0,"target":null,"running":false}"#;

fn status_of(response: &Response) -> Value {
    serde_json::from_slice(&response.body).unwrap()
}

#[test]
fn start_is_refused_when_a_run_is_already_in_progress() {
    let busy = FakeTrainingRunner::new(&status_json("train:lfm", &Value::Null, &Value::Null));
    let response = start_response(&busy, br#"{"job":"lfm-train","confirm":true}"#);
    assert_eq!(response.status_code, 409);
    assert!(String::from_utf8_lossy(&response.body).contains("already in progress"));

    let gen = FakeTrainingRunner::new(&status_json("idle", &Value::Null, &json(GEN_ACTIVE)));
    let response = start_response(&gen, br#"{"job":"generate","confirm":true}"#);
    assert_eq!(response.status_code, 409);
}

#[test]
fn start_and_switch_are_refused_while_a_run_is_active() {
    // lfm-eval keeps state `idle`; the busy check must come from run.running.
    let status = &status_json("idle", &json(RUN), &Value::Null);
    let runner = FakeTrainingRunner::new(status);
    assert_eq!(
        start_response(&runner, br#"{"job":"lfm-eval","confirm":true}"#).status_code,
        409
    );

    let runner = FakeTrainingRunner::new(status);
    assert_eq!(
        switch_response(&runner, br#"{"profile":"qwen3-4b","confirm":true}"#).status_code,
        409
    );

    // A finished run (running:false) must not block start or switch.
    let status = &status_json("idle", &json(RUN_DONE), &Value::Null);
    let runner = FakeTrainingRunner::new(status);
    assert_eq!(
        start_response(&runner, br#"{"job":"generate","confirm":true}"#).status_code,
        200
    );
    let runner = FakeTrainingRunner::new(status);
    assert_eq!(
        switch_response(&runner, br#"{"profile":"qwen3-4b","confirm":true}"#).status_code,
        200
    );
}

#[test]
fn status_parses_full_run_and_generation() {
    let runner = FakeTrainingRunner::new(&status_json("train:lfm", &json(RUN), &json(GEN_IDLE)));
    let response = status_response(&runner);
    assert_eq!(response.status_code, 200);
    let value = status_of(&response);
    assert_eq!(value["ok"], true);
    assert_eq!(value["state"], "train:lfm");
    assert_eq!(value["run"]["kind"], "lfm-train");
    assert_eq!(value["run"]["progress"]["loss"], 0.42);
    assert_eq!(value["run"]["running"], true);
    assert_eq!(value["generation"]["rows"], 0);

    // A finished run (running:false) must still parse and pass through.
    let runner = FakeTrainingRunner::new(&status_json("idle", &json(RUN_DONE), &Value::Null));
    let value = status_of(&status_response(&runner));
    assert_eq!(value["run"]["running"], false);
    assert_eq!(value["run"]["progress"]["eta_s"], Value::Null);
}

#[test]
fn status_parses_null_run_and_generation() {
    let runner = FakeTrainingRunner::new(IDLE_STATUS);
    let response = status_response(&runner);
    assert_eq!(response.status_code, 200);
    let value = status_of(&response);
    assert_eq!(value["state"], "idle");
    assert!(value["run"].is_null());
    assert!(value["generation"].is_null());
    assert!(value["active_profile"].is_null());
    assert!(value["profile_details"].is_null());
}

#[test]
fn status_passes_through_running_and_profile_details() {
    let status = serde_json::json!({
        "state": "train:lfm",
        "runtime_up": true,
        "active_profile": "qwen3-4b",
        "profiles": ["qwen3-4b"],
        "run": json(RUN),
        "generation": null,
        "profile_details": {
            "qwen3-4b": {
                "label": "Qwen3 4B",
                "model": "/models/qwen3-4b",
                "serving": {"engine": "vllm", "port": 8000},
                "sampling": {"temperature": 0.6}
            }
        },
        "host_time": "2026-07-10T01:00:00Z"
    })
    .to_string();
    let runner = FakeTrainingRunner::new(&status);
    let response = status_response(&runner);
    assert_eq!(
        response.status_code, 200,
        "unknown fields must be tolerated"
    );
    let value = status_of(&response);
    assert_eq!(value["run"]["running"], true);
    assert_eq!(value["profile_details"]["qwen3-4b"]["label"], "Qwen3 4B");
    assert_eq!(
        value["profile_details"]["qwen3-4b"]["model"],
        "/models/qwen3-4b"
    );
}

#[test]
fn models_returns_profiles_active_and_running() {
    let runner = FakeTrainingRunner::new(IDLE_STATUS);
    let response = models_response(&runner);
    assert_eq!(response.status_code, 200);
    let value = status_of(&response);
    assert_eq!(value["profiles"][0], "qwen3-4b");
    assert!(value["active_profile"].is_null());
    assert_eq!(value["running"], false);
}

#[test]
fn start_rejects_missing_confirm() {
    let runner = FakeTrainingRunner::new(IDLE_STATUS);
    let response = start_response(&runner, br#"{"job":"generate"}"#);
    assert_eq!(response.status_code, 400);
    assert_eq!(status_of(&response)["error"], "confirm is required");
    assert!(runner.recorded_calls().is_empty());
}

#[test]
fn start_rejects_unknown_job() {
    let runner = FakeTrainingRunner::new(IDLE_STATUS);
    let response = start_response(&runner, br#"{"job":"rm -rf /","confirm":true}"#);
    assert_eq!(response.status_code, 400);

    assert!(runner.recorded_calls().is_empty());
}

#[test]
fn start_rejects_malformed_body() {
    let runner = FakeTrainingRunner::new(IDLE_STATUS);
    assert_eq!(start_response(&runner, b"nope").status_code, 400);
    assert_eq!(
        start_response(&runner, br#"{"confirm":true}"#).status_code,
        400
    );
}

#[test]
fn start_sends_only_the_validated_job() {
    let runner = FakeTrainingRunner::new(IDLE_STATUS);
    let response = start_response(&runner, br#"{"job":"lfm-eval","confirm":true}"#);
    assert_eq!(response.status_code, 200);
    assert_eq!(status_of(&response)["ok"], true);
    assert_eq!(
        runner.recorded_calls(),
        vec![
            vec!["status".to_string()],
            vec!["start".to_string(), "lfm-eval".to_string()]
        ]
    );
}

#[test]
fn every_job_is_accepted() {
    for job in TRAINING_JOBS {
        let runner = FakeTrainingRunner::new(IDLE_STATUS);
        let body = format!("{{\"job\":\"{job}\",\"confirm\":true}}");
        assert_eq!(
            start_response(&runner, body.as_bytes()).status_code,
            200,
            "job {job}"
        );
    }
}

#[test]
fn stop_and_serve_require_confirm() {
    let runner = FakeTrainingRunner::new(IDLE_STATUS);
    assert_eq!(stop_response(&runner, b"{}").status_code, 400);
    assert_eq!(serve_response(&runner, b"{}").status_code, 400);

    let response = stop_response(&runner, br#"{"confirm":true}"#);
    assert_eq!(response.status_code, 200);
    let response = serve_response(&runner, br#"{"confirm":true}"#);
    assert_eq!(response.status_code, 200);

    let calls = runner.recorded_calls();
    assert_eq!(calls[0], vec!["stop"]);
    assert_eq!(calls[1], vec!["serve"]);
}

#[test]
fn switch_rejects_bad_profile_names() {
    for name in ["Qwen3-4b", "a.b.c/def", "", "-lead", &"x".repeat(65)] {
        let runner = FakeTrainingRunner::new(IDLE_STATUS);
        let body = format!("{{\"profile\":\"{name}\",\"confirm\":true}}");
        let response = switch_response(&runner, body.as_bytes());
        assert_eq!(response.status_code, 400, "profile {name:?}");
    }

    // 64 characters is the maximum allowed length.
    let long = format!("a{}", "b".repeat(63));
    let runner = FakeTrainingRunner::new(&format!(
        r#"{{"state":"idle","runtime_up":false,"active_profile":null,"profiles":["{long}"],"run":null,"generation":null}}"#
    ));
    let body = format!("{{\"profile\":\"{long}\",\"confirm\":true}}");
    assert_eq!(switch_response(&runner, body.as_bytes()).status_code, 200);
}

#[test]
fn switch_rejects_profile_not_in_host_list() {
    let runner = FakeTrainingRunner::new(IDLE_STATUS);
    let response = switch_response(&runner, br#"{"profile":"not-listed","confirm":true}"#);
    assert_eq!(response.status_code, 400);
}

#[test]
fn switch_refused_while_training() {
    let runner = FakeTrainingRunner::new(&status_json("train:lfm", &Value::Null, &Value::Null));
    let response = switch_response(&runner, br#"{"profile":"qwen3-4b","confirm":true}"#);
    assert_eq!(response.status_code, 409);
    assert_eq!(
        runner.recorded_calls(),
        vec![vec!["status".to_string()]],
        "no profile-set may be issued while training"
    );
}

#[test]
fn switch_refused_while_generation_running() {
    let runner = FakeTrainingRunner::new(&status_json("idle", &Value::Null, &json(GEN_ACTIVE)));
    assert_eq!(
        switch_response(&runner, br#"{"profile":"qwen3-4b","confirm":true}"#).status_code,
        409
    );
}

#[test]
fn switch_allowed_when_generation_idle() {
    let runner = FakeTrainingRunner::new(&status_json("idle", &Value::Null, &json(GEN_IDLE)));
    let response = switch_response(&runner, br#"{"profile":"qwen3-4b","confirm":true}"#);
    assert_eq!(response.status_code, 200);
    assert_eq!(
        runner.recorded_calls(),
        vec![
            vec!["status".to_string()],
            vec!["profile-set".to_string(), "qwen3-4b".to_string()],
        ],
        "idle generation allows the profile switch"
    );
}

#[test]
fn switch_requires_confirm() {
    let runner = FakeTrainingRunner::new(IDLE_STATUS);
    assert_eq!(
        switch_response(&runner, br#"{"profile":"qwen3-4b"}"#).status_code,
        400
    );
}

#[test]
fn runner_failures_map_to_502_and_504_without_leaking_argv() {
    for (error, status) in [
        (TrainingRunError::Failed, 502u16),
        (TrainingRunError::Timeout, 504),
    ] {
        let runner = FakeTrainingRunner::new(IDLE_STATUS).failing(error);
        for response in [
            start_response(&runner, br#"{"job":"generate","confirm":true}"#),
            stop_response(&runner, br#"{"confirm":true}"#),
            serve_response(&runner, br#"{"confirm":true}"#),
            switch_response(&runner, br#"{"profile":"qwen3-4b","confirm":true}"#),
        ] {
            assert_eq!(response.status_code, status, "error {error:?}");
            let value = status_of(&response);
            assert_eq!(value["ok"], false);
            let body = String::from_utf8_lossy(&response.body);
            assert!(!body.contains("ssh") && !body.contains(".ssh") && !body.contains("generate"));
        }
    }
}

#[test]
fn unreadable_host_status_maps_to_502() {
    let runner = FakeTrainingRunner::new("not json");
    assert_eq!(status_response(&runner).status_code, 502);
    assert_eq!(models_response(&runner).status_code, 502);
}

#[test]
fn argv_never_contains_browser_text() {
    // A hostile profile is rejected before the runner is touched at all.
    let runner = FakeTrainingRunner::new(IDLE_STATUS);
    switch_response(&runner, br#"{"profile":"evil; rm -rf /","confirm":true}"#);
    start_response(
        &runner,
        br#"{"job":"rm -rf / && curl evil.example","confirm":true}"#,
    );
    for call in runner.recorded_calls() {
        assert!(
            call.iter()
                .all(|arg| !arg.contains(' ') && !arg.contains(';') && !arg.contains('$')),
            "argv must not carry browser text: {call:?}"
        );
    }
}

#[test]
fn profile_name_validation_matches_regex() {
    for name in ["a", "0", "ab.cd-ef9", "x1.y2.z3.w4"] {
        assert!(is_valid_profile_name(name), "{name}");
    }
    for name in ["", "_a", "-a", ".a", "A1", "a b", &"a".repeat(65)] {
        assert!(!is_valid_profile_name(name), "{name}");
    }
}
