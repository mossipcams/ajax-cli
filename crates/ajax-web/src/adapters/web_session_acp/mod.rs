//! Per-harness ACP stdio host for Ajax Web Session orchestration chat.

mod apply_model;
mod catalog;
mod client;
mod config_option_descriptors;
mod config_options;
mod sdk_connection;

pub use config_option_descriptors::{config_option_descriptors, ConfigOptionDescriptor};
pub use config_options::is_unspecified_model;

#[cfg(test)]
mod apply_model_tests;

#[cfg(test)]
mod client_spawn_model_tests;

#[cfg(test)]
mod client_tests;

#[cfg(test)]
mod config_options_tests;

#[cfg(test)]
mod spawn_tests;

pub use apply_model::{
    apply_model_pin, operator_pin_satisfied, read_applied_model, ApplyModelOutcome,
};
pub use catalog::{read_agent_model_catalog, AgentModelCatalog};
pub use client::{AcpClientEvent, AcpStdioClient, SpawnReport};

#[cfg(test)]
pub(crate) use client::{set_test_acp_command, with_test_acp_extra_args, with_test_acp_program};
