use super::*;

use super::{cockpit_json, handle_http_request, handle_http_request_with_runner_and_paths};
use ajax_core::runtime_refresh::RefreshTier;
use ajax_core::{
    adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
    commands::CommandContext,
    config::{Config, ManagedRepo},
    models::{AgentClient, GitStatus, LifecycleStatus, Task, TaskId, TaskWindowStatus, TmuxStatus},
    registry::{InMemoryRegistry, Registry, SqliteRegistryStore},
};
use ajax_web::runtime::{self, RuntimeBridge};
use axum::{body::Body, http::Request as AxumRequest};
use std::time::SystemTime;
use tower::util::ServiceExt;

include!("suite_1.rs");
include!("suite_2.rs");
