//! Per-harness ACP stdio host for Ajax Web Session orchestration chat.

mod apply_model;
mod catalog;
mod client;
mod config_option_descriptors;
mod config_options;
mod option_catalog;
mod sdk_connection;

pub use config_option_descriptors::{config_option_descriptors, ConfigOptionDescriptor};
pub use config_options::{
    applied_model_id_for_persist, is_unspecified_model, option_is_model_category,
    option_triggers_model_persist, session_model_for_task_persist, wire_value_to_session_value,
};

#[cfg(test)]
mod apply_model_tests;

#[cfg(test)]
mod client_spawn_model_tests;

#[cfg(test)]
mod client_tests;

#[cfg(test)]
mod config_options_tests;

#[cfg(test)]
mod cursor_forum_payload_tests;

#[cfg(test)]
mod spawn_tests;

pub use apply_model::{
    apply_config_option, apply_model_pin, operator_pin_satisfied, read_applied_model,
    ApplyModelOutcome,
};
pub use catalog::{read_agent_model_catalog, AgentModelCatalog};
pub use client::{AcpClientEvent, AcpStdioClient, SpawnReport};
pub use option_catalog::{cached_harness_config_options, remember_harness_config_options};

#[cfg(test)]
pub(crate) use client::{set_test_acp_command, with_test_acp_extra_args, with_test_acp_program};
#[cfg(test)]
pub(crate) use option_catalog::clear_option_catalog_cache;
