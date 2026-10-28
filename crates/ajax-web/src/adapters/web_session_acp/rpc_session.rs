#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::agent_event::{AgentEvent, NativeEvent, NativeImage};
use super::jsonl_process::{JsonlProcess, JsonlRecord};
use super::pi_rpc_map::{run_error, PiRunError};
use super::rpc_handshake::RpcHandshake;

const POLL_INTERVAL: Duration = Duration::from_millis(5);

pub type RecordMapper = fn(&Value, &str) -> RpcMapping;

pub struct RpcMapping {
    pub events: Vec<AgentEvent>,
    pub finished: bool,
    pub ended_aborted: bool,
}

#[derive(Debug)]
pub enum RpcStep {
    Events(Vec<AgentEvent>),
    RunFinished { aborted: bool },
    PromptRejected(String),
    Error(String),
    Exited,
    Idle,
}

pub struct RpcSession {
    mapper: RecordMapper,
    process: JsonlProcess,
    session_id: String,
    queued: VecDeque<Value>,
    pub(super) prompt_id: Option<String>,
    run_active: bool,
    ended_aborted: bool,
    pending_error: Option<String>,
    exited: bool,
    deferred_finish: Option<bool>,
    image_type_tag: bool,
    tool_inputs: HashMap<String, Value>,
}

impl RpcSession {
    pub fn new(process: JsonlProcess, handshake: RpcHandshake, mapper: RecordMapper) -> Self {
        Self::with_queue(
            process,
            handshake.session_id,
            VecDeque::from(handshake.pending),
            mapper,
            false,
        )
    }

    pub(crate) fn adopt(mapper: RecordMapper, process: JsonlProcess, session_id: String) -> Self {
        Self::with_queue(process, session_id, VecDeque::new(), mapper, true)
    }

    fn with_queue(
        process: JsonlProcess,
        session_id: String,
        queued: VecDeque<Value>,
        mapper: RecordMapper,
        run_active: bool,
    ) -> Self {
        Self {
            mapper,
            session_id,
            queued,
            process,
            prompt_id: None,
            run_active,
            ended_aborted: false,
            pending_error: None,
            exited: false,
            deferred_finish: None,
            image_type_tag: false,
            tool_inputs: HashMap::new(),
        }
    }

    pub fn with_image_type_tag(mut self) -> Self {
        self.image_type_tag = true;
        self
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn child_pid(&self) -> u32 {
        self.process.pid()
    }

    pub fn run_active(&self) -> bool {
        self.run_active
    }

    pub fn begin_prompt(&mut self, text: &str) -> Result<(), String> {
        self.begin_prompt_with_images(text, &[])
    }

    pub fn begin_prompt_with_images(
        &mut self,
        text: &str,
        images: &[NativeImage],
    ) -> Result<(), String> {
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
        self.begin_run("prompt", fields)
    }

    #[allow(dead_code)]
    pub fn begin_command(&mut self, command: &str, fields: Value) -> Result<(), String> {
        self.begin_run(command, fields)
    }

    fn begin_run(&mut self, record_type: &str, fields: Value) -> Result<(), String> {
        if self.run_active {
            return Err("a prompt run is already active".to_owned());
        }
        let id = self.process.send(record_type, fields)?;
        self.prompt_id = Some(id);
        self.run_active = true;
        self.ended_aborted = false;
        self.pending_error = None;
        Ok(())
    }

    pub fn abort(&mut self) -> Result<(), String> {
        if !self.run_active {
            return Err("no active prompt run to abort".to_owned());
        }
        let _id = self.process.send("abort", json!({}))?;
        Ok(())
    }

    pub fn respond(&mut self, command_type: &str, id: &str, fields: Value) -> Result<(), String> {
        self.process.send_with_id(command_type, id, fields)
    }

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
                Err(mpsc::TryRecvError::Empty) => {}
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

    pub fn next_step(&mut self, timeout: Duration) -> RpcStep {
        if self.exited {
            self.exited = false;
            self.run_active = false;
            self.prompt_id = None;
            self.ended_aborted = false;
            self.pending_error = None;
            self.tool_inputs.clear();
            return RpcStep::Exited;
        }

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
                JsonlRecord::Error(text) => return RpcStep::Error(text),
                JsonlRecord::Exited => {
                    self.run_active = false;
                    self.prompt_id = None;
                    self.ended_aborted = false;
                    self.pending_error = None;
                    self.tool_inputs.clear();
                    return RpcStep::Exited;
                }
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
                self.run_active = false;
                self.prompt_id = None;
                self.ended_aborted = false;
                self.pending_error = None;
                self.tool_inputs.clear();
                return Some(RpcStep::RunFinished { aborted });
            }
            self.deferred_finish = Some(aborted);
            return Some(RpcStep::Events(mapping.events));
        }
        if !mapping.events.is_empty() {
            return Some(RpcStep::Events(mapping.events));
        }
        None
    }

    pub(super) fn prompt_response_step(&mut self, value: &Value) -> Option<RpcStep> {
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
                let handled =
                    value.pointer("/data/disposition").and_then(Value::as_str) == Some("handled");
                let command_run = value
                    .get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|command| command != "prompt");
                if handled || command_run {
                    self.run_active = false;
                    self.prompt_id = None;
                    self.ended_aborted = false;
                    self.pending_error = None;
                    self.tool_inputs.clear();
                    Some(RpcStep::RunFinished { aborted: false })
                } else {
                    None
                }
            }
        }
    }

    fn next_record(&mut self, deadline: Instant) -> Option<JsonlRecord> {
        if let Some(value) = self.queued.pop_front() {
            return Some(JsonlRecord::Record(value));
        }

        loop {
            match self.process.try_recv() {
                Ok(record) => return Some(record),
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => return None,
            }

            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}

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
