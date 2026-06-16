mod agent_runtime;
#[cfg(feature = "interactive")]
mod agent_status_cache;
mod app;
#[cfg(feature = "interactive")]
mod bgtmux;
mod cli;
#[cfg(feature = "interactive")]
mod cockpit_actions;
#[cfg(feature = "interactive")]
mod cockpit_backend;
mod context;
mod dispatch;
mod execution_dispatch;
mod render;
mod snapshot_dispatch;
#[cfg(feature = "supervisor")]
mod supervise;
#[cfg(feature = "interactive")]
mod task_session;
#[path = "web_backend.rs"]
mod web_companion_backend;

pub use app::*;

pub(crate) use app::{
    command_error, current_open_mode, new_task_request, task_arg, RenderedCommand,
};

#[cfg(test)]
#[cfg(feature = "interactive")]
pub(crate) use cockpit_actions::{
    execute_pending_cockpit_action, execute_pending_cockpit_action_with_task_session,
    handle_pending_cockpit_result, tui_cockpit_action, tui_cockpit_confirmed_action,
};
#[cfg(test)]
#[cfg(feature = "interactive")]
pub(crate) use cockpit_backend::{refresh_cockpit_snapshot, render_cockpit_command};
#[cfg(test)]
pub(crate) use dispatch::{render_drop_command, render_task_command};
#[cfg(test)]
pub(crate) use snapshot_dispatch::parent_directory_available;
