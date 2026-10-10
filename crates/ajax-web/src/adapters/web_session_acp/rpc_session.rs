//! The transport-neutral session driver core on top of [`JsonlProcess`].
//! A [`RpcSession`] owns a handshaken child process, sends `prompt` and `abort`
//! commands through the id-correlated transport, and turns the record stream
//! (handshake pending records first, then live stdout) into [`RpcStep`] values
//! for the caller. It keeps only run state — no usage stats, no restore.
//!
//! Step rules: a `response` record whose id matches the active prompt decides
//! that run (`success:false` rejects it with the response's error text,
//! `data.disposition == "handled"` finishes it). Every other record is folded
//! through the injected record mapper — events are surfaced in order, and a
//! mapper-reported finish ends the run with the remembered aborted flag.
//! `next_step` returns at most one step per call; records that carry no adapter
//! meaning are consumed inside the same call until a step-worthy result arrives
//! or the timeout elapses ([`RpcStep::Idle`]).

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::client::AcpClientEvent;
use super::jsonl_process::{JsonlProcess, JsonlRecord};
use super::rpc_handshake::RpcHandshake;

/// How long each `try_recv` gap sleeps before re-polling the record channel.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Maps one transport record into adapter events and run state.
pub type RecordMapper = fn(&Value, &str) -> RpcMapping;

/// What one inbound transport record means for the web session adapter.
pub struct RpcMapping {
    /// Adapter events to forward, in order. Empty for non-mappable records.
    pub events: Vec<AcpClientEvent>,
    /// The agent run has settled.
    pub finished: bool,
    /// The run was reported aborted.
    pub ended_aborted: bool,
}

/// One step of a run as seen by the session driver.
#[derive(Debug)]
pub enum RpcStep {
    /// Adapter events to forward, in order.
    Events(Vec<AcpClientEvent>),
    /// The agent run has settled; `aborted` reflects the injected record mapper's stop reason.
    RunFinished { aborted: bool },
    /// The prompt was rejected before any run started; carries the response's error text.
    PromptRejected(String),
    /// A stdout line that was not valid JSON; carries the raw text.
    Error(String),
    /// The child process exited; no more records will arrive.
    Exited,
    /// No step-worthy record arrived within the timeout.
    Idle,
}

/// A handshaken JSONL session: prompt/abort commands in, [`RpcStep`]s out.
pub struct RpcSession {
    mapper: RecordMapper,
    process: JsonlProcess,
    session_id: String,
    /// Handshake pending records, replayed before any live stdout record.
    queued: VecDeque<Value>,
    /// The id allocated by the last `begin_prompt`; `None` until one is sent.
    prompt_id: Option<String>,
    run_active: bool,
    /// Set when a record in the current run reports an aborted stop reason;
    /// folded into the next [`RpcStep::RunFinished`] and reset on every run end.
    ended_aborted: bool,
    /// Set when `Exited` was read while a [`Self::request`] was in flight; the
    /// next [`Self::next_step`] reports it exactly once and clears this flag.
    exited: bool,
    /// Set when a finishing record also carried events; the [`RpcStep::RunFinished`] it implies is delivered on the next call, after those events.
    deferred_finish: Option<bool>,
}

impl RpcSession {
    /// Adopt a handshaken process. `handshake.session_id` binds every generated
    /// notification and `handshake.pending` is queued so those records are
    /// processed first, in order, by [`Self::next_step`].
    pub fn new(process: JsonlProcess, handshake: RpcHandshake, mapper: RecordMapper) -> Self {
        Self {
            mapper,
            session_id: handshake.session_id,
            queued: VecDeque::from(handshake.pending),
            process,
            prompt_id: None,
            run_active: false,
            ended_aborted: false,
            exited: false,
            deferred_finish: None,
        }
    }

    /// The ACP session id this stream is bound to.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Whether a prompt run is currently active (sent, not yet settled).
    pub fn run_active(&self) -> bool {
        self.run_active
    }

    /// Send a `prompt` command and mark the run active. Fails when a run is
    /// already active or the child's stdin is closed.
    pub fn begin_prompt(&mut self, text: &str) -> Result<(), String> {
        if self.run_active {
            return Err("a prompt run is already active".to_owned());
        }
        let id = self.process.send("prompt", json!({ "message": text }))?;
        self.prompt_id = Some(id);
        self.run_active = true;
        self.ended_aborted = false;
        Ok(())
    }

    /// Send an `abort` command for the active run. Fails when no run is active.
    pub fn abort(&mut self) -> Result<(), String> {
        if !self.run_active {
            return Err("no active prompt run to abort".to_owned());
        }
        let _id = self.process.send("abort", json!({}))?;
        Ok(())
    }

    /// Send a one-shot command and wait for its `response` record. Unlike
    /// [`Self::next_step`], this reads straight from the process stream (the
    /// `queued` pending records are never consumed here); every other record
    /// read while waiting is pushed onto the BACK of `queued` in arrival order
    /// so a later `next_step` processes it. A transport error, child exit, or
    /// deadline elapse without the matching response is an `Err`; on success
    /// the response's `data` field is returned (`Null` when absent).
    pub fn request(
        &mut self,
        command_type: &str,
        fields: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let id = self.process.send(command_type, fields)?;
        let deadline = Instant::now() + timeout;

        loop {
            match self.process.try_recv() {
                // No record yet: keep polling until the deadline below.
                Err(mpsc::TryRecvError::Empty) => {}
                // Disconnected only happens after `Exited` was already
                // consumed, so this is a terminal no-response path.
                Err(mpsc::TryRecvError::Disconnected) => break,
                Ok(record) => match record {
                    JsonlRecord::Record(value)
                        if value.get("type").and_then(Value::as_str) == Some("response")
                            && value.get("id").and_then(Value::as_str) == Some(id.as_str()) =>
                    {
                        return match value.get("success").and_then(Value::as_bool) {
                            Some(false) => Err(value
                                .get("error")
                                .and_then(Value::as_str)
                                .map(str::to_owned)
                                .unwrap_or_else(|| {
                                    format!("request {command_type} failed with no error field")
                                })),
                            _ => Ok(value.get("data").cloned().unwrap_or(Value::Null)),
                        };
                    }
                    // Not this command's response: preserve it for `next_step`.
                    JsonlRecord::Record(value) => self.queued.push_back(value),
                    JsonlRecord::Error(text) => {
                        return Err(format!(
                            "request {command_type} hit a transport error: {text}"
                        ));
                    }
                    JsonlRecord::Exited => {
                        self.exited = true;
                        return Err(format!(
                            "child exited during request {command_type}; stderr tail: {}",
                            self.process.stderr_tail()
                        ));
                    }
                },
            }

            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(POLL_INTERVAL);
        }

        Err(format!(
            "request {command_type} timed out with no response; stderr tail: {}",
            self.process.stderr_tail()
        ))
    }

    /// Take the next step of the run: a child exit observed during a prior
    /// [`Self::request`] is reported first (exactly once), then queued pending
    /// records, then the process stream, polling `try_recv` every 5 ms until
    /// `timeout` elapses.
    /// Returns at most one [`RpcStep`] per call ([`RpcStep::Idle`] on timeout);
    /// records without adapter meaning are consumed inside this call so the
    /// caller never sees them.
    pub fn next_step(&mut self, timeout: Duration) -> RpcStep {
        // A child exit observed inside a `request` is reported here exactly
        // once, before any queued record could mask it.
        if self.exited {
            self.exited = false;
            self.run_active = false;
            self.prompt_id = None;
            self.ended_aborted = false;
            return RpcStep::Exited;
        }

        // A deferred finish is delivered before any new record is read.
        if let Some(aborted) = self.deferred_finish.take() {
            self.run_active = false;
            self.prompt_id = None;
            self.ended_aborted = false;
            return RpcStep::RunFinished { aborted };
        }

        let deadline = Instant::now() + timeout;

        loop {
            let Some(record) = self.next_record(deadline) else {
                return RpcStep::Idle;
            };

            match record {
                // Transport-level anomalies surface immediately; a prompt in
                // flight cannot settle after the child is gone.
                JsonlRecord::Error(text) => return RpcStep::Error(text),
                JsonlRecord::Exited => {
                    self.run_active = false;
                    self.prompt_id = None;
                    self.ended_aborted = false;
                    return RpcStep::Exited;
                }
                // No adapter meaning keeps the loop consuming within this
                // call; only a step-worthy fold stops it.
                JsonlRecord::Record(value) => {
                    if let Some(step) = self.step_for_record(&value) {
                        return step;
                    }
                }
            }

            if Instant::now() >= deadline {
                return RpcStep::Idle;
            }
        }
    }

    /// Fold one parsed record into a step. `None` means the record carries no
    /// adapter meaning for this session and the loop keeps consuming.
    fn step_for_record(&mut self, value: &Value) -> Option<RpcStep> {
        if let Some(step) = self.prompt_response_step(value) {
            return Some(step);
        }

        let mapping: RpcMapping = (self.mapper)(value, self.session_id());
        if mapping.ended_aborted {
            self.ended_aborted = true;
        }
        if mapping.finished {
            let aborted = self.ended_aborted;
            if mapping.events.is_empty() {
                // The run has settled: clear it so a later prompt starts fresh.
                self.run_active = false;
                self.prompt_id = None;
                self.ended_aborted = false;
                return Some(RpcStep::RunFinished { aborted });
            }
            // A finishing record that also carries events surfaces the
            // events first; the finish is delivered on the next call.
            self.deferred_finish = Some(aborted);
            return Some(RpcStep::Events(mapping.events));
        }
        if !mapping.events.is_empty() {
            return Some(RpcStep::Events(mapping.events));
        }
        None
    }

    /// The outcome of the active prompt's `response` record, when this is one.
    /// Responses to other commands (handshake leftovers, `abort`) pass through
    /// to the injected record mapper.
    fn prompt_response_step(&mut self, value: &Value) -> Option<RpcStep> {
        if value.get("type").and_then(Value::as_str) != Some("response") {
            return None;
        }
        let prompt_id = self.prompt_id.as_deref()?;
        if value.get("id").and_then(Value::as_str)? != prompt_id {
            return None;
        }

        match value.get("success").and_then(Value::as_bool) {
            Some(false) => {
                let detail = value
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| "prompt failed with no error field".to_owned());
                self.run_active = false;
                self.prompt_id = None;
                self.ended_aborted = false;
                Some(RpcStep::PromptRejected(detail))
            }
            _ => {
                if value.pointer("/data/disposition").and_then(Value::as_str) == Some("handled") {
                    // The response reports the prompt fully handled: the run is over.
                    self.run_active = false;
                    self.prompt_id = None;
                    self.ended_aborted = false;
                    Some(RpcStep::RunFinished { aborted: false })
                } else {
                    // e.g. `disposition: "started"` — acknowledgement only; the
                    // run keeps going, so keep the prompt id for correlation.
                    None
                }
            }
        }
    }

    /// The next record to process: a queued pending record (no waiting) or the
    /// first live stdout record before `deadline`; `None` on timeout.
    fn next_record(&mut self, deadline: Instant) -> Option<JsonlRecord> {
        if let Some(value) = self.queued.pop_front() {
            return Some(JsonlRecord::Record(value));
        }

        loop {
            match self.process.try_recv() {
                Ok(record) => return Some(record),
                Err(mpsc::TryRecvError::Empty) => {}
                // The reader thread only disconnects after delivering `Exited`,
                // which is already in the channel; unreachable, but drain-safe.
                Err(mpsc::TryRecvError::Disconnected) => return None,
            }

            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}
