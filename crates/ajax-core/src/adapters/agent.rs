use super::command::CommandSpec;
use crate::models::AgentClient;

pub const CURSOR_DEFAULT_MODEL: &str = "cursor-grok-4.6-high";

pub const CURSOR_DEFAULT_SPAWN_MODEL: &str = "grok-4.6";

const CURSOR_EFFORT_SUFFIXES: [&str; 6] = ["xhigh", "high", "medium", "low", "none", "max"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorModelIntent {
    pub base: String,
    pub effort: Option<String>,
    pub fast: Option<bool>,
    pub thinking: Option<bool>,
}

pub fn parse_cursor_model_intent(raw: &str) -> Option<CursorModelIntent> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "auto" {
        return None;
    }
    if raw.contains('|') {
        let selection = parse_model_selection(raw)?;
        let mut effort = None;
        let mut fast = None;
        let mut thinking = None;
        for (key, value) in &selection.options {
            match key.as_str() {
                "effort" => effort = Some(value.clone()),
                "fast" => fast = Some(value == "true"),
                "thinking" => thinking = Some(value == "true"),
                _ => {}
            }
        }
        return Some(CursorModelIntent {
            base: selection.model,
            effort,
            fast,
            thinking,
        });
    }
    if let Some((base, bracket)) = raw.split_once('[') {
        let bracket = bracket.strip_suffix(']')?;
        let mut effort = None;
        let mut fast = None;
        let mut thinking = None;
        for part in bracket.split(',') {
            let (key, value) = part.split_once('=')?;
            match key.trim() {
                "effort" => effort = Some(value.trim().to_string()),
                "fast" => fast = Some(value.trim() == "true"),
                "thinking" => thinking = Some(value.trim() == "true"),
                _ => {}
            }
        }
        return Some(CursorModelIntent {
            base: base.to_string(),
            effort,
            fast,
            thinking,
        });
    }

    let fast = raw.ends_with("-fast");
    let stem = if fast {
        &raw[..raw.len().saturating_sub(5)]
    } else {
        raw
    };

    if let Some(rest) = stem.strip_prefix("cursor-grok-") {
        for effort in CURSOR_EFFORT_SUFFIXES {
            if let Some(version) = rest.strip_suffix(&format!("-{effort}")) {
                return Some(CursorModelIntent {
                    base: format!("grok-{version}"),
                    effort: Some(effort.to_string()),
                    fast: Some(fast),
                    thinking: None,
                });
            }
        }
    }

    if let Some((prefix, effort)) = stem.rsplit_once('-') {
        if prefix.ends_with("-thinking") && CURSOR_EFFORT_SUFFIXES.contains(&effort) {
            return Some(CursorModelIntent {
                base: prefix.to_string(),
                effort: Some(effort.to_string()),
                fast: Some(fast),
                thinking: Some(true),
            });
        }
    }

    for effort in CURSOR_EFFORT_SUFFIXES {
        if let Some(base) = stem.strip_suffix(&format!("-{effort}")) {
            return Some(CursorModelIntent {
                base: base.to_string(),
                effort: Some(effort.to_string()),
                fast: Some(fast),
                thinking: None,
            });
        }
    }

    Some(CursorModelIntent {
        base: stem.to_string(),
        effort: None,
        fast: Some(fast),
        thinking: None,
    })
}

pub fn canonical_cursor_model_intent(intent: &CursorModelIntent) -> CursorModelIntent {
    if intent.base.ends_with("-thinking") {
        CursorModelIntent {
            base: intent
                .base
                .strip_suffix("-thinking")
                .unwrap_or(&intent.base)
                .to_string(),
            effort: intent.effort.clone(),
            fast: intent.fast,
            thinking: Some(true),
        }
    } else {
        intent.clone()
    }
}

pub fn cursor_bracket_token_from_intent(intent: &CursorModelIntent) -> String {
    let canonical = canonical_cursor_model_intent(intent);
    let fast = canonical.fast.unwrap_or(false);
    let mut options = Vec::new();
    if canonical.thinking == Some(true) {
        options.push("thinking=true".to_string());
    }
    if let Some(effort) = &canonical.effort {
        options.push(format!("effort={effort}"));
    }
    options.push(format!("fast={fast}"));
    format!("{}[{}]", canonical.base, options.join(","))
}

pub fn cursor_model_intents_match(
    desired: &CursorModelIntent,
    applied: &CursorModelIntent,
) -> bool {
    let desired = canonical_cursor_model_intent(desired);
    let applied = canonical_cursor_model_intent(applied);
    if desired.base != applied.base || desired.effort != applied.effort {
        return false;
    }
    desired.thinking.unwrap_or(false) == applied.thinking.unwrap_or(false)
        && desired.fast.unwrap_or(false) == applied.fast.unwrap_or(false)
}

fn compose_cursor_catalog_id_from_intent(intent: &CursorModelIntent) -> String {
    let canonical = canonical_cursor_model_intent(intent);
    let fast = canonical.fast.unwrap_or(false);
    let mut id = if let Some(version) = canonical.base.strip_prefix("grok-") {
        format!("cursor-grok-{version}")
    } else {
        canonical.base.clone()
    };
    if canonical.thinking == Some(true) {
        id.push_str("-thinking");
    }
    if let Some(effort) = &canonical.effort {
        id.push('-');
        id.push_str(effort);
    }
    if fast {
        id.push_str("-fast");
    }
    id
}

pub fn cursor_catalog_to_acp_spawn_token(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed == CURSOR_DEFAULT_SPAWN_MODEL {
        return trimmed.to_string();
    }
    if trimmed.contains('|') || trimmed.contains('[') {
        return parse_cursor_model_intent(trimmed)
            .map(|intent| compose_cursor_catalog_id_from_intent(&intent))
            .unwrap_or_else(|| trimmed.to_string());
    }
    trimmed.to_string()
}

pub fn cursor_catalog_to_acp_in_band_token(catalog_id: &str) -> String {
    let Some(intent) = parse_cursor_model_intent(catalog_id) else {
        return catalog_id.to_string();
    };
    let uses_acp_brackets = intent.effort.is_some()
        || intent.thinking == Some(true)
        || catalog_id.starts_with("cursor-grok-")
        || catalog_id.starts_with("composer-")
        || catalog_id.contains("-thinking-")
        || catalog_id.contains('[');
    if !uses_acp_brackets {
        return catalog_id.to_string();
    }
    cursor_bracket_token_from_intent(&intent)
}

pub fn valid_cursor_model_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && !id.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcpModelSelection {
    SpawnArg,
    ConfigOption,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelSelection {
    pub model: String,
    pub options: Vec<(String, String)>,
}

impl ModelSelection {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HarnessTransport {
    Acp,
    PiRpc,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcpLaunch {
    pub candidates: &'static [(&'static str, &'static [&'static str])],
    pub native_program: Option<&'static str>,
    pub model_selection: AcpModelSelection,
    pub default_model: Option<&'static str>,
    pub acp_package: Option<&'static str>,
    pub install_hint: &'static str,
    pub transport: HarnessTransport,
}

impl AcpLaunch {
    pub fn model_pins_at_spawn(&self) -> bool {
        matches!(self.model_selection, AcpModelSelection::SpawnArg)
    }
}

pub fn acp_launch_for_agent(client: AgentClient) -> Option<AcpLaunch> {
    match client {
        AgentClient::Cursor => Some(AcpLaunch {
            candidates: &[("agent", &["acp"]), ("cursor", &["agent", "acp"])],
            native_program: None,
            model_selection: AcpModelSelection::SpawnArg,
            default_model: Some(CURSOR_DEFAULT_MODEL),
            acp_package: None,
            install_hint: "install the Cursor CLI (`agent`)",
            transport: HarnessTransport::Acp,
        }),
        AgentClient::Codex => Some(AcpLaunch {
            candidates: &[("codex-acp", &[])],
            native_program: Some("codex"),
            model_selection: AcpModelSelection::ConfigOption,
            default_model: None,
            acp_package: Some("@agentclientprotocol/codex-acp"),
            install_hint: "npm install -g @agentclientprotocol/codex-acp",
            transport: HarnessTransport::Acp,
        }),
        AgentClient::Claude => Some(AcpLaunch {
            candidates: &[("claude-agent-acp", &[])],
            native_program: Some("claude"),
            model_selection: AcpModelSelection::ConfigOption,
            default_model: None,
            acp_package: Some("@agentclientprotocol/claude-agent-acp"),
            install_hint: "npm install -g @agentclientprotocol/claude-agent-acp",
            transport: HarnessTransport::Acp,
        }),
        AgentClient::Pi => Some(AcpLaunch {
            candidates: &[("pi", &[])],
            // unused for HarnessTransport::PiRpc; kept so the native-CLI preference table stays uniform.
            native_program: Some("pi"),
            model_selection: AcpModelSelection::ConfigOption,
            default_model: None,
            acp_package: None,
            install_hint: "npm install -g @earendil-works/pi-coding-agent",
            transport: HarnessTransport::PiRpc,
        }),
        AgentClient::Other => None,
    }
}

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

pub fn rpc_harness_programs() -> Vec<(AgentClient, &'static str, &'static str)> {
    [
        AgentClient::Codex,
        AgentClient::Claude,
        AgentClient::Pi,
        AgentClient::Cursor,
    ]
    .into_iter()
    .filter_map(|client| {
        let launch = acp_launch_for_agent(client)?;
        if launch.transport == HarnessTransport::PiRpc {
            Some((client, launch.candidates[0].0, launch.install_hint))
        } else {
            None
        }
    })
    .collect()
}

pub fn is_unspecified_acp_model(raw: Option<&str>) -> bool {
    matches!(raw.map(str::trim), None | Some("") | Some("auto"))
}

pub fn acp_spawn_model_for_argv(launch: AcpLaunch, model: Option<&str>) -> Option<String> {
    if launch.model_pins_at_spawn() {
        let raw = if is_unspecified_acp_model(model) {
            CURSOR_DEFAULT_SPAWN_MODEL
        } else {
            model.map(str::trim)?
        };
        return Some(cursor_catalog_to_acp_spawn_token(raw));
    }
    if is_unspecified_acp_model(model) {
        None
    } else {
        model
            .map(str::trim)
            .and_then(|raw| parse_model_selection(raw).map(|selection| selection.model))
    }
}

pub fn acp_args_for_candidate(
    launch: AcpLaunch,
    base_args: &[&str],
    model: Option<&str>,
) -> Vec<String> {
    let mut args: Vec<String> = base_args.iter().map(|arg| (*arg).to_string()).collect();
    if !launch.model_pins_at_spawn() {
        return args;
    }
    let Some(model) = acp_spawn_model_for_argv(launch, model) else {
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

pub fn cursor_unspecified_spawn_satisfied(applied_model: &str) -> bool {
    let Some(applied_intent) = parse_cursor_model_intent(applied_model) else {
        return false;
    };
    if applied_intent.base.starts_with("composer-") && applied_intent.fast.unwrap_or(false) {
        return false;
    }
    if applied_intent.base.starts_with("grok-") && applied_intent.fast.unwrap_or(false) {
        return false;
    }
    if let Some(spawn_intent) = parse_cursor_model_intent(CURSOR_DEFAULT_SPAWN_MODEL) {
        if cursor_model_intents_match(&spawn_intent, &applied_intent) {
            return true;
        }
    }
    parse_cursor_model_intent(CURSOR_DEFAULT_MODEL)
        .is_some_and(|catalog_intent| cursor_model_intents_match(&catalog_intent, &applied_intent))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentLaunch {
    pub worktree_path: String,
    pub prompt: String,
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
