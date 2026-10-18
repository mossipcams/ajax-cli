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

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::agent_event::{AgentEvent, NativeEvent, NativeImage};
use super::jsonl_process::{JsonlProcess, JsonlRecord};
use super::pi_rpc_map::{run_error, PiRunError};
use super::rpc_handshake::RpcHandshake;

/// How long each `try_recv` gap sleeps before re-polling the record channel.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Maps one transport record into adapter events and run state.
pub type RecordMapper = fn(&Value, &str) -> RpcMapping;

/// What one inbound transport record means for the web session adapter.
pub struct RpcMapping {
    /// Adapter events to forward, in order. Empty for non-mappable records.
    pub events: Vec<AgentEvent>,
    /// The agent run has settled.
    pub finished: bool,
    /// The run was reported aborted.
    pub ended_aborted: bool,
}

/// One step of a run as seen by the session driver.
#[derive(Debug)]
pub enum RpcStep {
    /// Adapter events to forward, in order.
    Events(Vec<AgentEvent>),
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
    /// An assistant error the current run may still retry, surfaced as an
    /// [`NativeEvent::Error`] just before [`RpcStep::RunFinished`] unless a
    /// retry or a later assistant message clears it; reset on every run end.
    pending_error: Option<String>,
    /// Set when `Exited` was read while a [`Self::request`] was in flight; the
    /// next [`Self::next_step`] reports it exactly once and clears this flag.
    exited: bool,
    /// Set when a finishing record also carried events; the [`RpcStep::RunFinished`] it implies is delivered on the next call, after those events.
    deferred_finish: Option<bool>,
    /// When true, image objects in `prompt` commands carry an extra
    /// `"type": "image"` key (required by Pi RPC); default shape stays
    /// `{data, mimeType}` for consumers that validate by exact key count.
    image_type_tag: bool,
    /// Tool call ids to their last-seen input objects, so a later update with
    /// no `raw_input` can carry the opening call's input; cleared on every run end.
    tool_inputs: HashMap<String, Value>,
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
            pending_error: None,
            exited: false,
            deferred_finish: None,
            image_type_tag: false,
            tool_inputs: HashMap::new(),
        }
    }

    /// Mark this session for consumers whose `prompt` image objects must carry
    /// a `"type": "image"` key alongside `data` and `mimeType` (e.g. Pi RPC).
    pub fn with_image_type_tag(mut self) -> Self {
        self.image_type_tag = true;
        self
    }

    /// The ACP session id this stream is bound to.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// OS process id of the child driving this session.
    pub fn child_pid(&self) -> u32 {
        self.process.pid()
    }

    /// Whether a prompt run is currently active (sent, not yet settled).
    pub fn run_active(&self) -> bool {
        self.run_active
    }

    /// Send a `prompt` command and mark the run active. Fails when a run is
    /// already active or the child's stdin is closed.
    pub fn begin_prompt(&mut self, text: &str) -> Result<(), String> {
        self.begin_prompt_with_images(text, &[])
    }

    /// Like [`Self::begin_prompt`], attaching images as `images:` entries when
    /// any are given. Each entry is `{data, mimeType}` by default; when
    /// [`Self::with_image_type_tag`] was used it is
    /// `{"type": "image", data, mimeType}` (required by Pi RPC).
    pub fn begin_prompt_with_images(
        &mut self,
        text: &str,
        images: &[NativeImage],
    ) -> Result<(), String> {
        if self.run_active {
            return Err("a prompt run is already active".to_owned());
        }
        let mut fields = json!({ "message": text });
        if !images.is_empty() {
            let images: Vec<Value> = images
                .iter()
                .map(|image| {
                    if self.image_type_tag {
                        json!({ "type": "image", "data": image.data, "mimeType": image.mime_type })
                    } else {
                        json!({ "data": image.data, "mimeType": image.mime_type })
                    }
                })
                .collect();
            fields["images"] = Value::Array(images);
        }
        let id = self.process.send("prompt", fields)?;
        self.prompt_id = Some(id);
        self.run_active = true;
        self.ended_aborted = false;
        self.pending_error = None;
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

    /// Write one response line carrying a caller-chosen `id` (e.g. Pi's
    /// `extension_ui_response`, which must echo the id of its
    /// `extension_ui_request`). No auto-generated id is allocated.
    pub fn respond(&mut self, command_type: &str, id: &str, fields: Value) -> Result<(), String> {
        self.process.send_with_id(command_type, id, fields)
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
            self.pending_error = None;
            self.tool_inputs.clear();
            return RpcStep::Exited;
        }

        // A deferred finish is delivered before any new record is read.
        if let Some(aborted) = self.deferred_finish.take() {
            self.run_active = false;
            self.prompt_id = None;
            self.ended_aborted = false;
            self.pending_error = None;
            self.tool_inputs.clear();
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
                    self.pending_error = None;
                    self.tool_inputs.clear();
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

        let mut mapping: RpcMapping = (self.mapper)(value, self.session_id());
        carry_tool_inputs(&mut self.tool_inputs, &mut mapping.events);
        if mapping.ended_aborted {
            self.ended_aborted = true;
        }
        fold_pending_error(&mut self.pending_error, value, &mut mapping);
        if mapping.finished {
            let aborted = self.ended_aborted;
            if mapping.events.is_empty() {
                // The run has settled: clear it so a later prompt starts fresh.
                self.run_active = false;
                self.prompt_id = None;
                self.ended_aborted = false;
                self.pending_error = None;
                self.tool_inputs.clear();
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
                self.pending_error = None;
                self.tool_inputs.clear();
                Some(RpcStep::PromptRejected(detail))
            }
            _ => {
                if value.pointer("/data/disposition").and_then(Value::as_str) == Some("handled") {
                    // The response reports the prompt fully handled: the run is over.
                    self.run_active = false;
                    self.prompt_id = None;
                    self.ended_aborted = false;
                    self.pending_error = None;
                    self.tool_inputs.clear();
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

/// Fold one Pi record's run-error meaning into the pending assistant error.
/// An assistant error sets `pending`; a retry, a later assistant message, or a
/// failed run clears it; an extension notice passes through untouched. When
/// `mapping` finishes the run with an error still pending, that error is
/// appended as the run's final [`NativeEvent::Error`]. Claude and Cursor
/// records never carry the Pi types classified here, so they pass through.
pub(super) fn fold_pending_error(
    pending: &mut Option<String>,
    record: &Value,
    mapping: &mut RpcMapping,
) {
    match run_error(record) {
        Some(PiRunError::Pending(text)) => *pending = Some(text),
        Some(PiRunError::Clear | PiRunError::Failed(_)) => *pending = None,
        Some(PiRunError::Notice(_)) | None => {}
    }
    if mapping.finished {
        if let Some(text) = pending.take() {
            mapping
                .events
                .push(AgentEvent::Native(NativeEvent::Error(text)));
        }
    }
}

/// Re-attach tool call inputs to their later updates: an update whose
/// `raw_input` is missing but whose id was remembered from the opening
/// `ToolCall` (or an earlier input-bearing event) gets that input, and a
/// terminal (`completed`/`failed`) update drops its memo entry afterwards.
pub(super) fn carry_tool_inputs(memo: &mut HashMap<String, Value>, events: &mut [AgentEvent]) {
    for event in events.iter_mut() {
        let AgentEvent::Native(native) = event else {
            continue;
        };
        match native {
            NativeEvent::ToolCall { id, raw_input, .. } => {
                if let Some(input) = raw_input.as_ref() {
                    if input.is_object() {
                        memo.insert(id.clone(), input.clone());
                    }
                }
            }
            NativeEvent::ToolCallUpdate {
                id,
                status,
                raw_input,
                ..
            } => match raw_input.as_ref() {
                Some(input) if input.is_object() => {
                    memo.insert(id.clone(), input.clone());
                }
                None => {
                    if let Some(input) = memo.get(id).cloned().filter(|v| v.is_object()) {
                        *raw_input = Some(input);
                        if status == "completed" || status == "failed" {
                            memo.remove(id);
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}
