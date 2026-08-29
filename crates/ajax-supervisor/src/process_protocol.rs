use std::sync::Arc;

use ajax_core::events::AgentEvent;

pub type StdoutParser = Arc<dyn Fn(&str) -> Option<AgentEvent> + Send + Sync>;

pub trait ProcessProtocol {
    fn process_name(&self) -> &str;
    fn program(&self) -> &str;
    fn args(&self, prompt: &str) -> Vec<String>;
    fn stdout_parser(&self) -> StdoutParser;
}
