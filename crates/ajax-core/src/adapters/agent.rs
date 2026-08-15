use super::command::CommandSpec;
use crate::models::AgentClient;

/// Default Cursor Agent model for Ajax-started tasks (not Fast).
pub const CURSOR_DEFAULT_MODEL: &str = "cursor-grok-4.6-high";

/// Cursor model ids ride a launch command line and an ACP argv, so they must
/// stay a single bounded token. Bracketed option forms (`id[effort=high]`) pass.
pub fn valid_cursor_model_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && !id.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// How a harness is told which model to run.
///
/// Verified against the installed bridges: Codex, Claude, and Pi all answer
/// `session/set_config_option { configId, value }`, which also carries the
/// reasoning level they expose as a separate option. Cursor takes `--model`
/// before it speaks ACP at all, and bakes the level into the model id.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcpModelSelection {
    /// `--model <id>` on the spawn argv.
    SpawnArg,
    /// `session/set_config_option { sessionId, configId, value }` per option.
    ConfigOption,
}

/// A model choice plus the harness options that go with it, written
/// `opus|effort=high`. Cursor has no options and is just the bare id.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelSelection {
    pub model: String,
    /// Extra harness config options, e.g. `("effort", "high")`.
    pub options: Vec<(String, String)>,
}

impl ModelSelection {
    /// Rebuild the stored form. Round-trips with [`parse_model_selection`].
    pub fn encode(&self) -> String {
        let mut out = self.model.clone();
        for (key, value) in &self.options {
            out.push('|');
            out.push_str(key);
            out.push('=');
            out.push_str(value);
        }
        out
    }
}

/// Parse a stored selection. `None` when any piece is not a bounded token, so
/// the same check guards ids arriving from the browser.
pub fn parse_model_selection(raw: &str) -> Option<ModelSelection> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > 256 {
        return None;
    }
    let mut parts = raw.split('|');
    let model = parts.next()?.trim();
    if !valid_cursor_model_id(model) {
        return None;
    }
    let mut options = Vec::new();
    for part in parts {
        let (key, value) = part.split_once('=')?;
        let (key, value) = (key.trim(), value.trim());
        if !valid_cursor_model_id(key) || !valid_cursor_model_id(value) {
            return None;
        }
        options.push((key.to_string(), value.to_string()));
    }
    Some(ModelSelection {
        model: model.to_string(),
        options,
    })
}

/// How one harness is started as an ACP stdio agent.
///
/// Cursor speaks ACP natively (`agent acp`); Codex, Claude, and Pi each ship a
/// separate ACP bridge binary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcpLaunch {
    /// Programs to try in order, each with its base args.
    pub candidates: &'static [(&'static str, &'static [&'static str])],
    /// Harness CLI that would speak ACP itself, when it ever advertises an
    /// `acp` subcommand. Ajax prefers it over the packaged adapter. `None` when
    /// the candidates above are already the harness's own binary.
    pub native_program: Option<&'static str>,
    /// How this harness accepts a model choice.
    pub model_selection: AcpModelSelection,
    /// Model Ajax runs when the operator has picked none. `None` means the
    /// harness picks for itself.
    pub default_model: Option<&'static str>,
    /// npm package providing the ACP program, when the harness needs one.
    /// Ajax can run it on demand instead of requiring a global install.
    pub acp_package: Option<&'static str>,
    /// Shown when no candidate program can be spawned.
    pub install_hint: &'static str,
}

impl AcpLaunch {
    /// True when `--model <id>` belongs on the spawn argv.
    pub fn model_pins_at_spawn(&self) -> bool {
        matches!(self.model_selection, AcpModelSelection::SpawnArg)
    }
}

/// ACP entry point for a harness, or `None` when Ajax has no ACP mapping for it.
pub fn acp_launch_for_agent(client: AgentClient) -> Option<AcpLaunch> {
    match client {
        AgentClient::Cursor => Some(AcpLaunch {
            candidates: &[("agent", &["acp"]), ("cursor", &["agent", "acp"])],
            // `agent acp` is Cursor's own ACP server.
            native_program: None,
            model_selection: AcpModelSelection::SpawnArg,
            default_model: Some(CURSOR_DEFAULT_MODEL),
            acp_package: None,
            install_hint: "install the Cursor CLI (`agent`)",
        }),
        AgentClient::Codex => Some(AcpLaunch {
            candidates: &[("codex-acp", &[])],
            // Codex 0.147 speaks its own `app-server` protocol, not ACP.
            native_program: Some("codex"),
            model_selection: AcpModelSelection::ConfigOption,
            default_model: None,
            acp_package: Some("@agentclientprotocol/codex-acp"),
            install_hint: "npm install -g @agentclientprotocol/codex-acp",
        }),
        AgentClient::Claude => Some(AcpLaunch {
            candidates: &[("claude-agent-acp", &[])],
            // Claude Code 2.1.232 ships no ACP server.
            native_program: Some("claude"),
            model_selection: AcpModelSelection::ConfigOption,
            default_model: None,
            acp_package: Some("@agentclientprotocol/claude-agent-acp"),
            install_hint: "npm install -g @agentclientprotocol/claude-agent-acp",
        }),
        AgentClient::Pi => Some(AcpLaunch {
            candidates: &[("pi-acp", &[])],
            // Pi 0.80 exposes `--mode rpc`, its own protocol, not ACP.
            native_program: Some("pi"),
            model_selection: AcpModelSelection::ConfigOption,
            default_model: None,
            acp_package: Some("pi-acp"),
            install_hint: "npm install -g pi-acp",
        }),
        AgentClient::Other => None,
    }
}

/// Harnesses whose ACP support comes from a separate adapter package, with the
/// program each one installs. Used by `ajax doctor` and by the session host.
pub fn acp_adapter_packages() -> Vec<(AgentClient, &'static str, &'static str)> {
    [
        AgentClient::Codex,
        AgentClient::Claude,
        AgentClient::Pi,
        AgentClient::Cursor,
    ]
    .into_iter()
    .filter_map(|client| {
        let launch = acp_launch_for_agent(client)?;
        let package = launch.acp_package?;
        Some((client, launch.candidates[0].0, package))
    })
    .collect()
}

/// Argv for one ACP candidate, inserting `--model <id>` only where supported.
///
/// `agent acp` → `agent --model ID acp`; a bridge with no `acp` token appends.
pub fn acp_args_for_candidate(
    launch: AcpLaunch,
    base_args: &[&str],
    model: Option<&str>,
) -> Vec<String> {
    let mut args: Vec<String> = base_args.iter().map(|arg| (*arg).to_string()).collect();
    if !launch.model_pins_at_spawn() {
        return args;
    }
    let Some(model) = model
        .and_then(parse_model_selection)
        .map(|selection| selection.model)
    else {
        return args;
    };
    match args.iter().position(|arg| arg == "acp") {
        Some(acp_at) => {
            args.insert(acp_at, "--model".to_string());
            args.insert(acp_at + 1, model.to_string());
        }
        None => {
            args.push("--model".to_string());
            args.push(model.to_string());
        }
    }
    args
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentLaunch {
    pub worktree_path: String,
    pub prompt: String,
    /// Operator-chosen Cursor model; `None` uses [`CURSOR_DEFAULT_MODEL`].
    pub model: Option<String>,
}

pub fn agent_launch_spec(
    program: impl Into<String>,
    client: AgentClient,
    launch: &AgentLaunch,
) -> CommandSpec {
    let program = program.into();
    let cursor_model = launch
        .model
        .clone()
        .unwrap_or_else(|| CURSOR_DEFAULT_MODEL.to_string());
    let mut args = match client {
        AgentClient::Codex => {
            vec!["--cd".to_string(), launch.worktree_path.clone()]
        }
        AgentClient::Claude => vec!["--dangerously-skip-permissions".to_string()],
        AgentClient::Cursor if program == "cursor" => {
            vec!["agent".to_string(), "--model".to_string(), cursor_model]
        }
        AgentClient::Other if program == "cursor" => {
            vec!["agent".to_string(), "--model".to_string(), cursor_model]
        }
        AgentClient::Cursor | AgentClient::Pi | AgentClient::Other => Vec::new(),
    };
    if !launch.prompt.is_empty() {
        args.push(launch.prompt.clone());
    }
    CommandSpec {
        program,
        args,
        cwd: None,
        mode: super::command::CommandMode::Capture,
        timeout: None,
    }
}
