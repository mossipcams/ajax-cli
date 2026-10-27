use crate::adapters::http::{
    json_response, operation_response_with_request_id, response_from_web_error, Response,
};
use crate::runtime::bridge::{response_with_fresh_cockpit, RuntimeBridge};
use crate::runtime::operation_outcomes::{
    now_unix_secs, PendingOutcomeWrite, MAX_OUTCOME_AGE_SECS,
};
use crate::{
    adapters::{
        browser_session::BrowserSession, cloudflare_access::CloudflareAccessConfig,
        stt_provider::MoonshineProvider,
    },
    slices::{
        dev_deploy,
        push::PushHub,
        training::{SshTrainingRunner, TrainingCommandRunner},
        web_session::{owned_session_handles, SessionStorage, TaskSessionDirectory},
    },
    WebError,
};
use ajax_core::{
    adapters::CommandRunner, commands::CommandContext, config::Config, registry::InMemoryRegistry,
    registry::Registry,
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
pub(crate) const SESSION_MODEL_PERSIST_RUNTIME_ERROR: &str =
    "Could not save the session model. Try again in a moment.";
const CONTROL_LANE_BUSY_ERROR: &str = "control lane busy; session activity report deferred";
/// Session activity reports that found the control lane busy, in arrival order.
pub(crate) type DeferredActivityReports =
    Mutex<VecDeque<(String, crate::slices::web_session::SessionActivity)>>;

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
    /// Single-flight for cockpit refresh probes; the probes run outside `control_lane`.
    pub(crate) refresh_flight: Arc<tokio::sync::Mutex<()>>,
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
    pub(crate) training_runner: Arc<dyn TrainingCommandRunner>,
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
            refresh_flight: Arc::clone(&self.refresh_flight),
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
            training_runner: Arc::clone(&self.training_runner),
        }
    }
}

impl<C, B> WebAppState<C, B>
where
    C: Clone + CommandRunner,
    B: Clone + RuntimeBridge<C>,
{
    /// After a registry reload removed owned tasks, shuts down live chat
    /// sessions for handles this host no longer owns. Ownership is re-read from
    /// the current shared context when cleanup runs, so a task recreated under
    /// the same handle keeps its session. Spawned so the shared lock is never
    /// held across the cleanup; without a runtime there are no live actors.
    pub(crate) fn cleanup_unowned_sessions(&self)
    where
        C: Send + 'static,
        B: Send + 'static,
    {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let shared = Arc::clone(&self.shared);
        let directory = Arc::clone(&self.task_session_directory);
        runtime.spawn(async move {
            directory
                .cleanup_unowned(|handle| {
                    let guard = shared
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    owned_session_handles(&guard.context).contains(handle)
                })
                .await
        });
    }

    pub(crate) fn run_optimistic(
        &self,
        request_id: Option<&str>,
        conflict_message: &str,
        operate: impl FnOnce(&mut CommandContext<InMemoryRegistry>, &mut C, &mut B) -> (Response, bool),
    ) -> Response
    where
        C: Send + 'static,
        B: Send + 'static,
    {
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
                Ok(true) => self.cleanup_unowned_sessions(),
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

    pub(crate) fn report_task_session_activity(
        &self,
        handle: &str,
        activity: crate::slices::web_session::SessionActivity,
    ) -> Result<(), String> {
        let handle = handle.to_string();
        let state = self.clone();
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
        let _lane = self
            .control_lane
            .try_lock()
            .map_err(|_| CONTROL_LANE_BUSY_ERROR.to_string())?;
        self.record_task_session_activity_under_lane(handle, activity)
    }

    /// Applies a session activity report; the caller must already hold the control lane.
    fn record_task_session_activity_under_lane(
        &self,
        handle: &str,
        activity: crate::slices::web_session::SessionActivity,
    ) -> Result<(), String> {
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
        self.commit_durable(
            context,
            runner,
            bridge,
            base_revision,
            "cockpit state changed while reporting session activity",
        )
    }

    /// Reports session activity without dropping it when the control lane is busy (#1472).
    /// Reports queue behind any already-deferred ones so they apply in arrival order; a
    /// drainer thread applies the queue once the lane frees. Returns true once accepted.
    pub(crate) fn report_or_defer_task_session_activity(
        &self,
        deferred: &Arc<DeferredActivityReports>,
        handle: &str,
        activity: crate::slices::web_session::SessionActivity,
    ) -> bool
    where
        C: Send + Sync + 'static,
        B: Send + Sync + 'static,
    {
        let mut queue = deferred
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if queue.is_empty() {
            match self.report_task_session_activity(handle, activity) {
                Ok(()) => return true,
                Err(error) if error == CONTROL_LANE_BUSY_ERROR => {}
                Err(error) => {
                    tracing::warn!(
                        target: "ajax_web",
                        handle,
                        ?activity,
                        %error,
                        "session activity report failed"
                    );
                    return false;
                }
            }
        }
        let was_empty = queue.is_empty();
        queue.push_back((handle.to_string(), activity));
        if was_empty {
            self.spawn_deferred_activity_drainer(Arc::clone(deferred));
        }
        true
    }

    /// Applies queued reports on a std thread holding the control lane. Uses `blocking_lock`
    /// here, never on the session actor, so the actor never waits on the lane (#1472).
    fn spawn_deferred_activity_drainer(&self, deferred: Arc<DeferredActivityReports>)
    where
        C: Send + Sync + 'static,
        B: Send + Sync + 'static,
    {
        let state = self.clone();
        std::thread::spawn(move || {
            let _lane = state.control_lane.blocking_lock();
            loop {
                let next = deferred
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .pop_front();
                let Some((handle, activity)) = next else {
                    break;
                };
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    state.record_task_session_activity_under_lane(&handle, activity)
                })) {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => tracing::warn!(
                        target: "ajax_web",
                        handle = %handle,
                        ?activity,
                        %error,
                        "deferred session activity report failed"
                    ),
                    Err(_) => tracing::warn!(
                        target: "ajax_web",
                        handle = %handle,
                        ?activity,
                        "deferred session activity report panicked"
                    ),
                }
            }
        });
    }

    fn wire_session_activity_reporter(&self)
    where
        C: Send + Sync + 'static,
        B: Send + Sync + 'static,
    {
        let state = self.clone();
        let deferred: Arc<DeferredActivityReports> = Arc::new(Mutex::new(VecDeque::new()));
        self.task_session_directory
            .set_report_session_activity(Arc::new(move |handle, activity| {
                state.report_or_defer_task_session_activity(&deferred, handle, activity)
            }));
    }

    pub(crate) fn persist_task_session_model(
        &self,
        handle: &str,
        model: &str,
    ) -> Result<(), String> {
        let handle = handle.to_string();
        let model = model.to_string();
        let state = self.clone();
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
        self.commit_durable(
            context,
            runner,
            bridge,
            base_revision,
            "cockpit state changed while updating session model",
        )
    }

    pub(crate) fn prepare_task_session_attach(
        &self,
        handle: &str,
        model: &str,
    ) -> Result<
        crate::slices::web_session::SessionAttachPlan,
        crate::slices::web_session::SessionRouteError,
    > {
        let _lane = self.control_lane.blocking_lock();
        let (mut context, mut runner, bridge, base_revision) = {
            let guard = self.shared();
            (
                guard.context.clone(),
                guard.runner.clone(),
                guard.bridge.clone(),
                guard.revision,
            )
        };
        let skip_before = context
            .registry
            .get_task(&ajax_core::models::TaskId::new(handle))
            .is_some_and(|task| task.skip_interactive_agent());
        let plan = crate::slices::web_session::prepare_task_session(
            &mut context,
            &mut runner,
            handle,
            model,
        )?;
        let promoted = !skip_before
            && context
                .registry
                .get_task(&ajax_core::models::TaskId::new(handle))
                .is_some_and(|task| task.skip_interactive_agent());
        let mut guard = self.shared();
        if guard.revision != base_revision {
            return Err(crate::slices::web_session::SessionRouteError::NotOrchestrationChat);
        }
        // A promotion that is not on disk would revert on restart (#1225).
        if promoted
            && bridge
                .clone()
                .persist_registry_snapshot(&mut context)
                .is_err()
        {
            return Err(crate::slices::web_session::SessionRouteError::PromotionNotSaved);
        }
        guard.context = context;
        guard.runner = runner;
        guard.bridge = bridge;
        guard.revision = guard.revision.saturating_add(1);
        guard.cockpit_cache = None;
        Ok(plan)
    }

    /// Commit a control-lane mutation only once its registry snapshot is on
    /// disk, so success means durable and shared state never runs ahead of it.
    fn commit_durable(
        &self,
        mut context: CommandContext<InMemoryRegistry>,
        runner: C,
        bridge: B,
        base_revision: u64,
        conflict_message: &str,
    ) -> Result<(), String> {
        let mut guard = self.shared();
        if guard.revision != base_revision {
            return Err(conflict_message.to_string());
        }
        bridge
            .clone()
            .persist_registry_snapshot(&mut context)
            .map_err(|error| format!("registry state was not saved: {error}"))?;
        guard.context = context;
        guard.runner = runner;
        guard.bridge = bridge;
        guard.revision = guard.revision.saturating_add(1);
        guard.cockpit_cache = None;
        Ok(())
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

    /// Records a finished operation's outcome, then writes it with the
    /// operations lock already released so disk I/O never blocks other requests.
    pub(crate) fn finish_operation(
        &self,
        request_id: Option<&str>,
        task_key: &str,
        response: &Response,
    ) {
        let pending = self.operations().finish(request_id, task_key, response);
        if let Some(pending) = pending {
            pending.persist();
        }
    }

    pub fn new(
        context: CommandContext<InMemoryRegistry>,
        runner: C,
        bridge: B,
        storage: impl Into<SessionStorage>,
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
        let storage = storage.into();
        let state_dir = Arc::new(storage.state_dir.clone());
        let task_session_directory = TaskSessionDirectory::new(storage);
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
            refresh_flight: Arc::new(tokio::sync::Mutex::new(())),
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
            training_runner: Arc::new(SshTrainingRunner::from_env()),
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
        storage: impl Into<SessionStorage>,
    ) -> Result<Self, WebError>
    where
        C: Clone + CommandRunner + Send + Sync + 'static,
        B: Clone + RuntimeBridge<C> + Send + Sync + 'static,
    {
        let storage = storage.into();
        let state_dir = storage.state_dir.clone();
        let browser_session = BrowserSession::load_or_create(&state_dir)?;
        let push = PushHub::load_or_create(&state_dir).map_err(WebError::CommandFailed)?;
        let cloudflare_access = CloudflareAccessConfig::from_env()?;
        let stt_provider = moonshine_provider_from_config(&context.config);
        let stt_finalization_timeout_ms = context.config.stt.finalization_timeout_ms;
        let stt_phrase_end_silence_ms = context.config.stt.phrase_end_silence_ms;
        let stt_pause_grace_period_ms = context.config.stt.pause_grace_period_ms;
        let stt_language = context.config.stt.language.clone();
        let operations = OperationCoordinator::load_persistent(&state_dir);
        let state_dir = Arc::new(state_dir);
        let task_session_directory = TaskSessionDirectory::new(storage);
        task_session_directory.prune_stale_persisted(&owned_session_handles(&context));
        let state = Self {
            shared: Arc::new(Mutex::new(WebSharedState {
                context,
                runner,
                bridge,
                revision: 0,
                cockpit_cache: None,
            })),
            operations: Arc::new(Mutex::new(operations)),
            control_lane: Arc::new(tokio::sync::Mutex::new(())),
            refresh_flight: Arc::new(tokio::sync::Mutex::new(())),
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
            training_runner: Arc::new(SshTrainingRunner::from_env()),
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
        state.mark_browser_cockpit_seen();
        if !{
            let guard = state.shared();
            guard
                .bridge
                .needs_operator_acknowledgment(&guard.context, &task_handle)
        } {
            return;
        }
        let (mut context, mut bridge, base_revision) = {
            let guard = state.shared();
            (guard.context.clone(), guard.bridge.clone(), guard.revision)
        };
        if !bridge
            .acknowledge_operator_input(&mut context, &task_handle)
            .unwrap_or(false)
        {
            return;
        }
        let mut guard = state.shared();
        if guard.revision == base_revision {
            guard.context = context;
            guard.bridge = bridge;
        } else {
            let WebSharedState {
                context, bridge, ..
            } = &mut *guard;
            if !matches!(bridge.reload_registry_from_disk(context), Ok(true)) {
                return;
            }
            state.cleanup_unowned_sessions();
        }
        guard.revision = guard.revision.saturating_add(1);
        guard.cockpit_cache = None;
    })
}

#[derive(Default)]
pub(crate) struct OperationCoordinator {
    pub(crate) completed: HashMap<String, Response>,
    pub(crate) completed_at: HashMap<String, u64>,
    pub(crate) completed_request_ids: VecDeque<String>,
    pub(crate) in_flight_requests: BTreeSet<String>,
    pub(crate) in_flight_tasks: BTreeSet<String>,
    pub(crate) store_path: Option<PathBuf>,
    /// Sequence number of the newest outcome snapshot captured for persistence.
    pub(crate) write_seq: u64,
    /// Sequence number of the newest snapshot written; held only for the write.
    pub(crate) persisted_seq: Arc<Mutex<u64>>,
}

pub(crate) enum GateRejection {
    Replay(Response),
    Conflict,
}

impl OperationCoordinator {
    /// An outcome older than the replay window is treated as absent, in memory
    /// as well as on disk, so a long-lived host cannot replay it indefinitely.
    pub(crate) fn completed_response(&self, request_id: &str) -> Option<Response> {
        let completed_at = *self.completed_at.get(request_id)?;
        if now_unix_secs().saturating_sub(completed_at) > MAX_OUTCOME_AGE_SECS {
            return None;
        }
        self.completed.get(request_id).cloned()
    }

    pub(crate) fn has_in_flight_mutation(&self) -> bool {
        !self.in_flight_requests.is_empty() || !self.in_flight_tasks.is_empty()
    }

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

    /// Releases the in-flight markers and records the outcome. The returned
    /// write must be persisted by the caller after the operations lock is dropped.
    pub(crate) fn finish(
        &mut self,
        request_id: Option<&str>,
        task_key: &str,
        response: &Response,
    ) -> Option<PendingOutcomeWrite> {
        self.in_flight_tasks.remove(task_key);
        let request_id = request_id?;
        self.in_flight_requests.remove(request_id);
        self.store_completed_response(request_id.to_string(), response.clone())
    }

    pub(crate) fn store_completed_response(
        &mut self,
        request_id: String,
        response: Response,
    ) -> Option<PendingOutcomeWrite> {
        self.remember_completed_response(request_id, response, now_unix_secs());
        self.pending_write()
    }

    pub(crate) fn remember_completed_response(
        &mut self,
        request_id: String,
        response: Response,
        completed_at: u64,
    ) {
        if self
            .completed
            .insert(request_id.clone(), response)
            .is_some()
        {
            self.completed_request_ids
                .retain(|completed_id| completed_id != &request_id);
        }
        self.completed_at.insert(request_id.clone(), completed_at);
        self.completed_request_ids.push_back(request_id);
        while self.completed_request_ids.len() > MAX_COMPLETED_OPERATIONS {
            if let Some(oldest_request_id) = self.completed_request_ids.pop_front() {
                self.completed.remove(&oldest_request_id);
                self.completed_at.remove(&oldest_request_id);
            }
        }
    }
}
