//! Per-harness ACP stdio host for Ajax Web Session orchestration chat.

mod apply_model;
mod available_command_descriptors;
mod catalog;
mod client;
mod config_option_descriptors;
mod config_options;
mod prompt_capability_descriptors;
mod sdk_connection;
pub(crate) mod sdk_elicitation;

pub use available_command_descriptors::{
    available_command_descriptors, AvailableCommandDescriptor,
};
pub use config_option_descriptors::{config_option_descriptors, ConfigOptionDescriptor};
pub use config_options::{
    applied_model_id_for_persist, is_unspecified_model, option_triggers_model_persist,
    wire_value_to_session_value, SessionConfigValue,
};
pub use prompt_capability_descriptors::{prompt_capability_descriptor, PromptCapabilityDescriptor};

#[cfg(test)]
mod apply_model_tests;

#[cfg(test)]
mod client_spawn_model_tests;

#[cfg(test)]
mod client_tests;

#[cfg(test)]
mod available_commands_tests;

#[cfg(test)]
mod config_options_tests;

#[cfg(test)]
mod spawn_tests;

pub use apply_model::{
    apply_config_option, apply_model_pin, operator_pin_satisfied, read_applied_model,
    ApplyModelOutcome,
};
pub use catalog::{read_agent_model_catalog, read_cursor_acp_model_labels, AgentModelCatalog};
pub use client::{AcpClientEvent, AcpStdioClient, SpawnReport};
pub(crate) use sdk_connection::CancelOutcome;

#[cfg(test)]
pub(crate) use client::{set_test_acp_command, with_test_acp_extra_args, with_test_acp_program};
