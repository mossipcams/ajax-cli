//! Process-local web app state, control lane, and operation admission.

use crate::adapters::http::{
    json_response, operation_response_with_request_id, response_from_web_error, Response,
};
use crate::runtime::bridge::{response_with_fresh_cockpit, RuntimeBridge};
use crate::{
    adapters::{
        browser_session::BrowserSession, cloudflare_access::CloudflareAccessConfig,
        stt_provider::MoonshineProvider,
    },
    slices::{
        dev_deploy,
        push::PushHub,
        web_session::{owned_session_handles, TaskSessionDirectory},
    },
    WebError,
};
use ajax_core::{
    adapters::CommandRunner, commands::CommandContext, config::Config, registry::InMemoryRegistry,
};
use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub(crate) const COCKPIT_REFRESH_CACHE_TTL: Duration = Duration::from_millis(750);
pub(crate) const BROWSER_CONNECTED_TTL: Duration = Duration::from_secs(90);
pub(crate) const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const MAX_COMPLETED_OPERATIONS: usize = 128;
/// Operator-facing copy when set_model persist hits a runtime/panic failure (#962).
pub(crate) const SESSION_MODEL_PERSIST_RUNTIME_ERROR: &str =
    "Could not save the session model. Try again in a moment.";

pub(crate) fn moonshine_provider_from_config(config: &Config) -> Arc<Mutex<MoonshineProvider>> {
    Arc::new(Mutex::new(MoonshineProvider::new(
        config.stt.provider_command.clone(),
        config.stt.max_buffered_audio_ms,
        config.stt.phrase_end_silence_ms,
    )))
}

pub struct WebAppState<C, B> {
    pub(crate) shared: Arc<Mutex<WebSharedState<C, B>>>,
    pub(crate) operations: Arc<Mutex<OperationCoordinator>>,
    pub(crate) control_lane: Arc<tokio::sync::Mutex<()>>,
    pub(crate) state_dir: Arc<PathBuf>,
    pub(crate) push: Arc<PushHub>,
    pub(crate) browser_session: Arc<BrowserSession>,
    pub(crate) cloudflare_access: Arc<Option<CloudflareAccessConfig>>,
    pub(crate) last_browser_cockpit_at: Arc<Mutex<Option<Instant>>>,
    pub(crate) dev_deploy: Arc<dev_deploy::SharedDevDeploySlot>,
    pub(crate) stt_provider: Arc<Mutex<MoonshineProvider>>,
    pub(crate) stt_finalization_timeout_ms: u64,
    pub(crate) stt_phrase_end_silence_ms: u64,
    pub(crate) stt_pause_grace_period_ms: u64,
    pub(crate) stt_language: String,
    pub(crate) task_session_directory: Arc<TaskSessionDirectory>,
}

pub(crate) struct WebSharedState<C, B> {
    pub(crate) context: CommandContext<InMemoryRegistry>,
    pub(crate) runner: C,
    pub(crate) bridge: B,
    pub(crate) revision: u64,
    pub(crate) cockpit_cache: Option<CockpitCacheEntry>,
}

#[derive(Clone)]
pub(crate) struct CockpitCacheEntry {
    pub(crate) response: Response,
    pub(crate) cached_at: Instant,
    pub(crate) revision: u64,
}

impl<C, B> Clone for WebAppState<C, B> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
            operations: Arc::clone(&self.operations),
            control_lane: Arc::clone(&self.control_lane),
            state_dir: Arc::clone(&self.state_dir),
            push: Arc::clone(&self.push),
            browser_session: Arc::clone(&self.browser_session),
            cloudflare_access: Arc::clone(&self.cloudflare_access),
            last_browser_cockpit_at: Arc::clone(&self.last_browser_cockpit_at),
            dev_deploy: Arc::clone(&self.dev_deploy),
            stt_provider: Arc::clone(&self.stt_provider),
            stt_finalization_timeout_ms: self.stt_finalization_timeout_ms,
            stt_phrase_end_silence_ms: self.stt_phrase_end_silence_ms,
            stt_pause_grace_period_ms: self.stt_pause_grace_period_ms,
            stt_language: self.stt_language.clone(),
            task_session_directory: Arc::clone(&self.task_session_directory),
        }
    }
}

impl<C, B> WebAppState<C, B>
where
    C: Clone + CommandRunner,
    B: Clone + RuntimeBridge<C>,
{
    /// Run `operate` against a clone of the shared state without holding the
    /// `shared` lock across the call, then commit the result only if no other
    /// request advanced the revision in the meantime. A losing writer leaves
    /// shared state untouched and returns a `409` conflict unless the operate
    /// closure reports a durable persist, in which case shared state reloads
    /// from disk and returns the operate response with a fresh cockpit view.
    pub(crate) fn run_optimistic(
        &self,
        request_id: Option<&str>,
        conflict_message: &str,
        operate: impl FnOnce(&mut CommandContext<InMemoryRegistry>, &mut C, &mut B) -> (Response, bool),
    ) -> Response {
        let (mut context, mut runner, mut bridge, base_revision) = {
            let guard = self.shared();
            (
                guard.context.clone(),
                guard.runner.clone(),
                guard.bridge.clone(),
                guard.revision,
            )
        };
        let (response, durable) = operate(&mut context, &mut runner, &mut bridge);
        let mut guard = self.shared();
        if guard.revision == base_revision {
            guard.context = context;
            guard.runner = runner;
            guard.bridge = bridge;
            guard.revision = guard.revision.saturating_add(1);
            guard.cockpit_cache = None;
            response
        } else if durable {
            let reload_result = {
                let WebSharedState {
                    context, bridge, ..
                } = &mut *guard;
                bridge.reload_registry_from_disk(context)
            };
            match reload_result {
                Err(error) => return response_from_web_error(error, request_id),
                Ok(false) => {
                    guard.context = context;
                    guard.runner = runner;
                    guard.bridge = bridge;
                }
                Ok(true) => {}
            }
            guard.revision = guard.revision.saturating_add(1);
            guard.cockpit_cache = None;
            response_with_fresh_cockpit(response, &guard.context, request_id)
        } else {
            operation_response_with_request_id(
                json_response(
                    409,
                    serde_json::json!({ "ok": false, "error": conflict_message, "code": "conflict" }),
                )
                .unwrap_or_else(|error| response_from_web_error(error, request_id)),
                request_id,
            )
        }
    }

    /// Run `operate` against a clone without holding the shared lock. The HTTP
    /// response is always returned; when `metadata_changed` is true, observed
    /// PR metadata is persisted best-effort and merged into shared state only
    /// when no concurrent writer advanced the revision.
    pub(crate) fn run_read(
        &self,
        operate: impl FnOnce(&mut CommandContext<InMemoryRegistry>, &mut C, &mut B) -> (Response, bool),
    ) -> Response {
        let (mut context, mut runner, mut bridge, base_revision) = {
            let guard = self.shared();
            (
                guard.context.clone(),
                guard.runner.clone(),
                guard.bridge.clone(),
                guard.revision,
            )
        };
        let (response, metadata_changed) = operate(&mut context, &mut runner, &mut bridge);
        if metadata_changed {
            let _ = bridge.persist_registry_snapshot(&mut context);
            let mut guard = self.shared();
            if guard.revision == base_revision {
                guard.context = context;
                guard.runner = runner;
                guard.bridge = bridge;
                guard.revision = guard.revision.saturating_add(1);
                guard.cockpit_cache = None;
            }
        }
        response
    }

    /// Report an ACP turn transition as task evidence.
    ///
    /// A provisioned task has no agent pane, so this host is the only observer
    /// of its work; without this the dashboard, task page, TUI and `ajax
    /// status` show a pane-derived `Waiting` through an entire turn. Failures
    /// are swallowed by the caller: evidence reporting must never take down a
    /// live turn.
    pub(crate) fn report_task_session_activity(
        &self,
        handle: &str,
        activity: crate::slices::web_session::SessionActivity,
    ) -> Result<(), String> {
        let handle = handle.to_string();
        let state = self.clone();
        // Same lane discipline as session-model persistence: called from a
        // Tokio worker, and control_lane takes a blocking lock (#962).
        tokio::task::block_in_place(|| {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                state.report_task_session_activity_on_control_lane(&handle, activity)
            })) {
                Ok(result) => result,
                Err(_) => Err("session activity report panicked".to_string()),
            }
        })
    }

    fn report_task_session_activity_on_control_lane(
        &self,
        handle: &str,
        activity: crate::slices::web_session::SessionActivity,
    ) -> Result<(), String> {
        // Best-effort: never block the per-task session loop waiting for a
        // cockpit refresh that may itself be waiting on that loop (#1083).
        let _lane = self
            .control_lane
            .try_lock()
            .map_err(|_| "control lane busy; session activity report deferred".to_string())?;
        let (mut context, runner, bridge, base_revision) = {
            let guard = self.shared();
            (
                guard.context.clone(),
                guard.runner.clone(),
                guard.bridge.clone(),
                guard.revision,
            )
        };
        crate::slices::web_session::record_session_activity(
            &mut context,
            handle,
            activity,
            std::time::SystemTime::now(),
        )?;
        let mut guard = self.shared();
        if guard.revision == base_revision {
            guard.context = context;
            guard.runner = runner;
            guard.bridge = bridge;
            guard.revision = guard.revision.saturating_add(1);
            guard.cockpit_cache = None;
            let mut persisted_bridge = guard.bridge.clone();
            let _ = persisted_bridge.persist_registry_snapshot(&mut guard.context);
            Ok(())
        } else {
            Err("cockpit state changed while reporting session activity".to_string())
        }
    }

    fn wire_session_activity_reporter(&self)
    where
        C: Send + Sync + 'static,
        B: Send + Sync + 'static,
    {
        let state = self.clone();
        self.task_session_directory
            .set_report_session_activity(Arc::new(move |handle, activity| {
                // Best-effort: a lost race with another writer must not disturb
                // the turn this evidence described (#1069).
                state.report_task_session_activity(handle, activity).is_ok()
            }));
    }

    /// Persist desired session model metadata before the host replaces an ACP child.
    pub(crate) fn persist_task_session_model(
        &self,
        handle: &str,
        model: &str,
    ) -> Result<(), String> {
        let handle = handle.to_string();
        let model = model.to_string();
        let state = self.clone();
        // WebSocket set_model invokes this from a Tokio worker; control_lane uses
        // blocking_lock and must run inside block_in_place (issue #962).
        tokio::task::block_in_place(|| {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                state.persist_task_session_model_on_control_lane(&handle, &model)
            })) {
                Ok(result) => result,
                Err(_) => Err(SESSION_MODEL_PERSIST_RUNTIME_ERROR.to_string()),
            }
        })
    }

    fn persist_task_session_model_on_control_lane(
        &self,
        handle: &str,
        model: &str,
    ) -> Result<(), String> {
        let _lane = self.control_lane.blocking_lock();
        let (mut context, runner, bridge, base_revision) = {
            let guard = self.shared();
            (
                guard.context.clone(),
                guard.runner.clone(),
                guard.bridge.clone(),
                guard.revision,
            )
        };
        crate::slices::operate::set_task_session_model(&mut context, handle, model)
            .map_err(|error| crate::slices::operate::format_operate_error(&error))?;
        let mut guard = self.shared();
        if guard.revision == base_revision {
            guard.context = context;
            guard.runner = runner;
            guard.bridge = bridge;
            guard.revision = guard.revision.saturating_add(1);
            guard.cockpit_cache = None;
            let mut persisted_bridge = guard.bridge.clone();
            let _ = persisted_bridge.persist_registry_snapshot(&mut guard.context);
            Ok(())
        } else {
            Err("cockpit state changed while updating session model".to_string())
        }
    }
}

impl<C, B> WebAppState<C, B> {
    pub(crate) fn shared(&self) -> std::sync::MutexGuard<'_, WebSharedState<C, B>> {
        self.shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn operations(&self) -> std::sync::MutexGuard<'_, OperationCoordinator> {
        self.operations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn new(
        context: CommandContext<InMemoryRegistry>,
        runner: C,
        bridge: B,
        state_dir: PathBuf,
    ) -> Self
    where
        C: Clone + CommandRunner + Send + Sync + 'static,
        B: Clone + RuntimeBridge<C> + Send + Sync + 'static,
    {
        let stt_provider = moonshine_provider_from_config(&context.config);
        let stt_finalization_timeout_ms = context.config.stt.finalization_timeout_ms;
        let stt_phrase_end_silence_ms = context.config.stt.phrase_end_silence_ms;
        let stt_pause_grace_period_ms = context.config.stt.pause_grace_period_ms;
        let stt_language = context.config.stt.language.clone();
        let state_dir = Arc::new(state_dir.clone());
        let hub_dir = state_dir.as_ref().clone();
        let task_session_directory = TaskSessionDirectory::new(hub_dir);
        task_session_directory.prune_stale_persisted(&owned_session_handles(&context));
        let state = Self {
            shared: Arc::new(Mutex::new(WebSharedState {
                context,
                runner,
                bridge,
                revision: 0,
                cockpit_cache: None,
            })),
            operations: Arc::new(Mutex::new(OperationCoordinator::default())),
            control_lane: Arc::new(tokio::sync::Mutex::new(())),
            state_dir,
            push: PushHub::ephemeral(),
            browser_session: Arc::new(BrowserSession::test_default()),
            cloudflare_access: Arc::new(None),
            last_browser_cockpit_at: Arc::new(Mutex::new(None)),
            dev_deploy: Arc::new(Mutex::new(dev_deploy::DevDeploySlot::default())),
            stt_provider,
            stt_finalization_timeout_ms,
            stt_phrase_end_silence_ms,
            stt_pause_grace_period_ms,
            stt_language,
            task_session_directory,
        };
        state.wire_session_activity_reporter();
        state
    }

    pub fn mark_browser_cockpit_seen(&self) {
        *self
            .last_browser_cockpit_at
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Instant::now());
    }

    pub fn browser_connected(&self) -> bool {
        self.last_browser_cockpit_at
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some_and(|at| at.elapsed() < BROWSER_CONNECTED_TTL)
    }

    #[cfg(test)]
    pub(crate) fn set_browser_cockpit_seen_at_for_test(&self, at: Instant) {
        *self
            .last_browser_cockpit_at
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(at);
    }

    pub fn load_or_create(
        context: CommandContext<InMemoryRegistry>,
        runner: C,
        bridge: B,
        state_dir: PathBuf,
    ) -> Result<Self, WebError>
    where
        C: Clone + CommandRunner + Send + Sync + 'static,
        B: Clone + RuntimeBridge<C> + Send + Sync + 'static,
    {
        let browser_session = BrowserSession::load_or_create(&state_dir)?;
        let push = PushHub::load_or_create(&state_dir).map_err(WebError::CommandFailed)?;
        let cloudflare_access = CloudflareAccessConfig::from_env()?;
        let stt_provider = moonshine_provider_from_config(&context.config);
        let stt_finalization_timeout_ms = context.config.stt.finalization_timeout_ms;
        let stt_phrase_end_silence_ms = context.config.stt.phrase_end_silence_ms;
        let stt_pause_grace_period_ms = context.config.stt.pause_grace_period_ms;
        let stt_language = context.config.stt.language.clone();
        let hub_dir = state_dir.clone();
        let state_dir = Arc::new(state_dir);
        let task_session_directory = TaskSessionDirectory::new(hub_dir);
        task_session_directory.prune_stale_persisted(&owned_session_handles(&context));
        let state = Self {
            shared: Arc::new(Mutex::new(WebSharedState {
                context,
                runner,
                bridge,
                revision: 0,
                cockpit_cache: None,
            })),
            operations: Arc::new(Mutex::new(OperationCoordinator::default())),
            control_lane: Arc::new(tokio::sync::Mutex::new(())),
            state_dir,
            push,
            browser_session: Arc::new(browser_session),
            cloudflare_access: Arc::new(cloudflare_access),
            last_browser_cockpit_at: Arc::new(Mutex::new(None)),
            dev_deploy: Arc::new(Mutex::new(dev_deploy::DevDeploySlot::default())),
            stt_provider,
            stt_finalization_timeout_ms,
            stt_phrase_end_silence_ms,
            stt_pause_grace_period_ms,
            stt_language,
            task_session_directory,
        };
        state.wire_session_activity_reporter();
        Ok(state)
    }

    #[cfg(test)]
    pub(crate) fn with_cloudflare_access_for_test(self, config: CloudflareAccessConfig) -> Self {
        Self {
            cloudflare_access: Arc::new(Some(config)),
            ..self
        }
    }

    pub(crate) fn cached_cockpit_response(&self) -> Option<Response> {
        let guard = self.shared();
        let cache = guard.cockpit_cache.as_ref()?;
        if cache.revision != guard.revision {
            return None;
        }
        if cache.cached_at.elapsed() > COCKPIT_REFRESH_CACHE_TTL {
            return None;
        }
        Some(cache.response.clone())
    }
}

/// Construct the per-attach terminal input acknowledgment sink. The sink
/// locks shared state, split-borrows `context` and `bridge`, and calls
/// `bridge.acknowledge_operator_input(context, task_handle)`. On `Ok(true)`
/// it bumps `revision` (saturating) and clears `cockpit_cache` so the next
/// cockpit fetch observes the acknowledgment. On `Ok(false)` or error,
/// revision and cache are left untouched (errors are dropped: the terminal
/// adapter must not propagate core failures back into the wire loop).
pub fn operator_input_sink<C, B>(
    state: &WebAppState<C, B>,
    task_handle: String,
) -> Arc<dyn Fn() + Send + Sync>
where
    C: CommandRunner + Clone + Send + Sync + 'static,
    B: RuntimeBridge<C> + Clone + Send + Sync + 'static,
{
    let state = state.clone();
    Arc::new(move || {
        // Typing in the PWA terminal is active presence; refresh the notify
        // suppress TTL even when cockpit polls have stalled.
        state.mark_browser_cockpit_seen();
        let mut guard = state.shared();
        let acknowledged = {
            let WebSharedState {
                context, bridge, ..
            } = &mut *guard;
            bridge
                .acknowledge_operator_input(context, &task_handle)
                .unwrap_or(false)
        };
        if acknowledged {
            guard.revision = guard.revision.saturating_add(1);
            guard.cockpit_cache = None;
        }
    })
}

#[derive(Default)]
pub(crate) struct OperationCoordinator {
    pub(crate) completed: HashMap<String, Response>,
    pub(crate) completed_request_ids: VecDeque<String>,
    pub(crate) in_flight_requests: BTreeSet<String>,
    pub(crate) in_flight_tasks: BTreeSet<String>,
}

/// Why a mutation could not enter the in-flight gate.
pub(crate) enum GateRejection {
    /// The request id already completed; replay its stored response.
    Replay(Response),
    /// Another mutation holds the gate.
    Conflict,
}

impl OperationCoordinator {
    pub(crate) fn completed_response(&self, request_id: &str) -> Option<Response> {
        self.completed.get(request_id).cloned()
    }

    pub(crate) fn has_in_flight_mutation(&self) -> bool {
        !self.in_flight_requests.is_empty() || !self.in_flight_tasks.is_empty()
    }

    /// Claim the single-mutation gate for this request/task pair, or explain
    /// why the caller must stop: idempotent replay or a 409 conflict.
    pub(crate) fn try_begin(
        &mut self,
        request_id: Option<&str>,
        task_key: &str,
    ) -> Result<(), GateRejection> {
        if let Some(request_id) = request_id {
            if let Some(response) = self.completed_response(request_id) {
                return Err(GateRejection::Replay(response));
            }
        }
        if self.has_in_flight_mutation() {
            return Err(GateRejection::Conflict);
        }
        if let Some(request_id) = request_id {
            if !self.in_flight_requests.insert(request_id.to_string()) {
                return Err(GateRejection::Conflict);
            }
        }
        if !self.in_flight_tasks.insert(task_key.to_string()) {
            if let Some(request_id) = request_id {
                self.in_flight_requests.remove(request_id);
            }
            return Err(GateRejection::Conflict);
        }
        Ok(())
    }

    /// Release the gate and record the response for idempotent replay.
    pub(crate) fn finish(&mut self, request_id: Option<&str>, task_key: &str, response: &Response) {
        self.in_flight_tasks.remove(task_key);
        if let Some(request_id) = request_id {
            self.in_flight_requests.remove(request_id);
            self.store_completed_response(request_id.to_string(), response.clone());
        }
    }

    pub(crate) fn store_completed_response(&mut self, request_id: String, response: Response) {
        if self
            .completed
            .insert(request_id.clone(), response)
            .is_some()
        {
            self.completed_request_ids
                .retain(|completed_id| completed_id != &request_id);
        }
        self.completed_request_ids.push_back(request_id);
        while self.completed_request_ids.len() > MAX_COMPLETED_OPERATIONS {
            if let Some(oldest_request_id) = self.completed_request_ids.pop_front() {
                self.completed.remove(&oldest_request_id);
            }
        }
    }
}
