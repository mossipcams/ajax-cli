//! Per-harness ACP stdio host for Ajax Web Session orchestration chat.

mod bridge;
mod catalog;
mod client;
mod hub;
mod store;

#[cfg(test)]
mod client_tests;

#[cfg(test)]
mod hub_tests;

#[cfg(test)]
mod spawn_tests;

pub use bridge::bridge_task_session_socket;
pub use catalog::{read_agent_model_catalog, AgentModelCatalog};
pub use client::{AcpClientEvent, AcpStdioClient, SpawnReport};
pub use hub::WebSessionHub;

#[cfg(test)]
pub(crate) use client::{with_test_acp_extra_args, with_test_acp_program};
