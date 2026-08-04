//! Operate bridge trait and action failure helpers for the web runtime.

use crate::adapters::http::{json_response, Response};
use crate::slices::cockpit;
use crate::{slices::actions::supported_web_action, WebError};
use ajax_core::{
    adapters::CommandRunner, commands::CommandContext, models::OperatorAction,
    registry::InMemoryRegistry, runtime_refresh::RefreshTier,
};
use serde::Deserialize;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionFailure {
    pub message: String,
    pub state_changed: bool,
}

pub trait RuntimeBridge<C: CommandRunner> {
    fn refresh_cockpit(
        &mut self,
        context: &mut CommandContext<InMemoryRegistry>,
        runner: &mut C,
        tier: RefreshTier,
        deliver_notifications: bool,
    ) -> Result<bool, WebError>;

    fn execute_operate(
        &mut self,
        request: crate::slices::operate::OperateRequest,
        context: &mut CommandContext<InMemoryRegistry>,
        runner: &mut C,
    ) -> Result<crate::slices::operate::OperateOutcome, ActionFailure>;

    fn execute_start_task(
        &mut self,
        request: crate::slices::operate::StartTaskRequest,
        context: &mut CommandContext<InMemoryRegistry>,
        runner: &mut C,
    ) -> Result<crate::slices::operate::OperateOutcome, ActionFailure>;

    /// Acknowledge operator attention for `task_handle` (e.g. the operator typed
    /// in the Web Cockpit terminal). Returns `true` when the acknowledgment
    /// advanced the task state and so callers should invalidate any cached
    /// cockpit projection; `Ok(false)` means no-ack (recently acknowledged, no
    /// newer live evidence, etc.). Errors are dropped by the sink caller; the
    /// default body preserves existing (pre-ack) behavior.
    fn acknowledge_operator_input(
        &mut self,
        _context: &mut CommandContext<InMemoryRegistry>,
        _task_handle: &str,
    ) -> Result<bool, WebError> {
        Ok(false)
    }

    /// Persist registry mutations that are not part of operate/start/ack flows
    /// (e.g. Diff Review PR metadata observation). Default is a no-op for tests.
    fn persist_registry_snapshot(
        &mut self,
        _context: &mut CommandContext<InMemoryRegistry>,
    ) -> Result<(), WebError> {
        Ok(())
    }
}

#[derive(Clone, Deserialize, serde::Serialize)]
pub(crate) struct MobileActionRequest {
    #[serde(default)]
    pub(crate) request_id: Option<String>,
    pub(crate) task_handle: String,
    pub(crate) action: String,
    #[serde(default)]
    pub(crate) confirmed: bool,
    #[serde(default)]
    pub(crate) branch_adoption: Option<ajax_core::commands::BranchAdoptionPlan>,
}

pub(crate) fn handle_refreshed_cockpit_request<C: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut C,
    bridge: &mut impl RuntimeBridge<C>,
    tier: RefreshTier,
    deliver_notifications: bool,
) -> Result<Response, WebError> {
    let _state_changed = bridge.refresh_cockpit(context, runner, tier, deliver_notifications)?;
    json_response(
        200,
        serde_json::to_value(cockpit::browser_cockpit_view(context))
            .map_err(|error| WebError::JsonSerialization(error.to_string()))?,
    )
}

pub(crate) fn handle_action_request<C: CommandRunner>(
    request: MobileActionRequest,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut C,
    bridge: &mut impl RuntimeBridge<C>,
) -> Result<Response, WebError> {
    if let Some(failure) = unsupported_operate_action(&request.action) {
        return operation_error_response(failure, context);
    }

    match bridge.execute_operate(
        crate::slices::operate::OperateRequest {
            task_handle: request.task_handle,
            action: request.action,
            confirmed: request.confirmed,
            branch_adoption: request.branch_adoption,
        },
        context,
        runner,
    ) {
        Ok(outcome) => operation_success_response(outcome, context),
        Err(error) => operation_error_response(error, context),
    }
}

pub(crate) fn operation_success_response(
    outcome: crate::slices::operate::OperateOutcome,
    context: &CommandContext<InMemoryRegistry>,
) -> Result<Response, WebError> {
    json_response(
        200,
        serde_json::json!({
            "ok": true,
            "state_changed": outcome.state_changed,
            "output": outcome.output,
            "cockpit": cockpit::browser_cockpit_view(context),
        }),
    )
}

pub(crate) fn operation_error_response(
    error: ActionFailure,
    context: &CommandContext<InMemoryRegistry>,
) -> Result<Response, WebError> {
    json_response(
        409,
        serde_json::json!({
            "ok": false,
            "error": error.message,
            "state_changed": error.state_changed,
            "cockpit": cockpit::browser_cockpit_view(context),
        }),
    )
}

pub(crate) fn unsupported_operate_action(action: &str) -> Option<ActionFailure> {
    let operator_action = OperatorAction::from_label(action)?;
    if supported_web_action(operator_action) {
        return None;
    }
    let message = match operator_action {
        OperatorAction::Start => {
            "start uses the dedicated Web Cockpit new-task operation".to_string()
        }
        _ => format!("unsupported action: {action}"),
    };
    Some(ActionFailure {
        message,
        state_changed: false,
    })
}
