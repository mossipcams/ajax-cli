pub mod agent;
pub mod command;
pub mod environment;
pub mod git;
pub mod github;
pub mod process;
pub mod tmux;

pub use agent::{
    acp_adapter_packages, acp_admits_orchestration_chat, acp_args_for_candidate,
    acp_launch_for_agent, acp_spawn_model_for_argv, agent_launch_spec,
    canonical_cursor_model_intent, cursor_bracket_token_from_intent,
    cursor_catalog_to_acp_in_band_token, cursor_catalog_to_acp_spawn_token,
    cursor_model_intents_match, cursor_unspecified_spawn_satisfied, is_unspecified_acp_model,
    parse_cursor_model_intent, parse_model_selection, valid_cursor_model_id, AcpLaunch,
    AcpModelSelection, AgentLaunch, CursorModelIntent, ModelSelection, CURSOR_DEFAULT_MODEL,
    CURSOR_DEFAULT_SPAWN_MODEL,
};
pub use command::{
    CommandMode, CommandOutput, CommandRunError, CommandRunner, CommandSpec, RecordingCommandRunner,
};
pub use environment::{DoctorEnvironment, REQUIRED_DOCTOR_TOOLS};
pub use git::GitAdapter;
pub use github::{CiChecksObservation, CiChecksReport, CiChecksState, GithubChecksAdapter};
pub use process::{clear_ambient_git_env, ProcessCommandRunner, AMBIENT_GIT_ENV_VARS};
pub use tmux::TmuxAdapter;

#[cfg(test)]
mod tests;
