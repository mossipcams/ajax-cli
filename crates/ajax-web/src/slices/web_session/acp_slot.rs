use super::acp_drain::AcpDrainOutcome;
use super::acp_usage::UsageDeduper;
use crate::adapters::web_session_acp::{
    AvailableCommandDescriptor, ConfigOptionDescriptor, PromptCapabilityDescriptor, SessionClient,
};

pub(super) struct AcpSlot {
    pub client: Option<SessionClient>,
    pub model: String,
    pub applied_model: String,
    pub acp_alive: bool,
    pub restore_advertised: bool,
    pub session_config_options: Option<Vec<ConfigOptionDescriptor>>,
    pub pending_config_snapshot: Option<Vec<ConfigOptionDescriptor>>,
    pub session_available_commands: Option<Vec<AvailableCommandDescriptor>>,
    pub pending_commands_snapshot: Option<Vec<AvailableCommandDescriptor>>,
    pub session_prompt_capabilities: Option<PromptCapabilityDescriptor>,
    pub pending_capabilities_snapshot: Option<PromptCapabilityDescriptor>,
    pub pending_model_snapshot: Option<String>,
    pub session_title: Option<String>,
    pub pending_title_snapshot: bool,
    pub usage_deduper: UsageDeduper,
}

impl AcpSlot {
    pub(super) fn apply_drain_outcome(&mut self, outcome: &AcpDrainOutcome) {
        if let Some(model) = &outcome.applied_model {
            self.applied_model = model.clone();
            self.pending_model_snapshot = Some(self.applied_model.clone());
        }
        if let Some(options) = &outcome.session_config_options {
            self.session_config_options = Some(options.clone());
            self.pending_config_snapshot = Some(options.clone());
        }
        if let Some(commands) = &outcome.session_available_commands {
            self.session_available_commands = Some(commands.clone());
            self.pending_commands_snapshot = Some(commands.clone());
        }
        if let Some(title) = &outcome.session_title_update {
            self.session_title = title.clone();
            self.pending_title_snapshot = true;
        }
    }

    pub(super) fn host_gone(&mut self, host_exit_from_drain: bool) -> bool {
        host_exit_from_drain
            || self
                .client
                .as_mut()
                .is_some_and(|client| client.host_exited())
    }
}
