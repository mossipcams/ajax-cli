use super::command::CommandSpec;
use crate::models::AgentClient;

/// Default Cursor Agent model for Ajax-started tasks (not Fast).
pub const CURSOR_DEFAULT_MODEL: &str = "cursor-grok-4.6-high";

/// Cursor ACP spawn argv when the operator leaves model unspecified / Auto.
///
/// Pro+ defaults Grok 4.6 to Fast when `--model` is omitted; this id selects
/// the standard non-Fast tier on the harness command line ([#979]).
pub const CURSOR_DEFAULT_SPAWN_MODEL: &str = "grok-4.6";

/// Effort suffixes on Cursor catalog ids (`gpt-5.6-sol-high`, `cursor-grok-4.6-xhigh`, …).
const CURSOR_EFFORT_SUFFIXES: [&str; 6] = ["xhigh", "high", "medium", "low", "none", "max"];

/// Semantic pieces shared by Ajax catalog ids and Cursor ACP handshake ids.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorModelIntent {
    pub base: String,
    pub effort: Option<String>,
    pub fast: Option<bool>,
    /// Live Cursor bracket ids carry `-thinking` in Ajax catalog bases separately.
    pub thinking: Option<bool>,
}

/// Parse a Cursor catalog id, pipe-form selection, or ACP bracket id into intent.
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

/// Normalize `-thinking` catalog bases onto `thinking=true` with a stripped base.
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

/// Bracket ACP id for a Cursor intent (`claude-opus-5[thinking=true,effort=high,fast=false]`).
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

/// True when `applied` satisfies `desired`, including thinking and Fast axes.
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

/// Reconstruct an exploded Cursor catalog id from a parsed intent.
///
/// Pipe-form picker selections persist base / effort / fast separately ([#991]);
/// spawn argv must receive the catalog id Cursor accepts on `--model` ([#989]).
/// Only `grok-*` bases receive the `cursor-grok-*` prefix; `thinking=true` folds
/// onto a `-thinking` base before effort and fast suffixes are appended.
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

/// Map an Ajax catalog id or pipe-form selection to the token Cursor accepts on spawn `--model`.
///
/// Live Cursor honors catalog ids and bare handshake bases from `agent models` on
/// spawn argv ([#989]). Pipe-form and bracket handshake ids reconstruct those
/// catalog ids; bare bases and exploded catalog ids pass through unchanged
/// ([#1079]). Bracket synthesis is reserved for in-band apply via
/// [`cursor_catalog_to_acp_in_band_token`].
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

/// Map an Ajax catalog id to the bracket ACP model id for in-band apply.
///
/// Unlike [`cursor_catalog_to_acp_spawn_token`], Grok catalog ids map to bracket
/// tokens here because `session/set_config_option` never accepts catalog ids ([#954]).
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
    /// Ajax Chat continuity requires restoring a stored session via
    /// `session/resume` or `loadSession` instead of silently starting fresh.
    pub requires_durable_restore: bool,
    /// Static audit: this harness advertises and honors durable restore.
    pub supports_durable_restore: bool,
}

impl AcpLaunch {
    /// True when `--model <id>` belongs on the spawn argv.
    pub fn model_pins_at_spawn(&self) -> bool {
        matches!(self.model_selection, AcpModelSelection::SpawnArg)
    }

    /// True when Ajax may offer resumable orchestration chat for this harness.
    pub fn admits_orchestration_chat(&self) -> bool {
        !self.requires_durable_restore || self.supports_durable_restore
    }
}

/// True when the harness may hold a resumable Ajax Chat session.
pub fn acp_admits_orchestration_chat(client: AgentClient) -> bool {
    acp_launch_for_agent(client).is_some_and(|launch| launch.admits_orchestration_chat())
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
            requires_durable_restore: true,
            supports_durable_restore: true,
        }),
        AgentClient::Codex => Some(AcpLaunch {
            candidates: &[("codex-acp", &[])],
            // Codex 0.147 speaks its own `app-server` protocol, not ACP.
            native_program: Some("codex"),
            model_selection: AcpModelSelection::ConfigOption,
            default_model: None,
            acp_package: Some("@agentclientprotocol/codex-acp"),
            install_hint: "npm install -g @agentclientprotocol/codex-acp",
            requires_durable_restore: true,
            supports_durable_restore: true,
        }),
        AgentClient::Claude => Some(AcpLaunch {
            candidates: &[("claude-agent-acp", &[])],
            // Claude Code 2.1.232 ships no ACP server.
            native_program: Some("claude"),
            model_selection: AcpModelSelection::ConfigOption,
            default_model: None,
            acp_package: Some("@agentclientprotocol/claude-agent-acp"),
            install_hint: "npm install -g @agentclientprotocol/claude-agent-acp",
            requires_durable_restore: true,
            supports_durable_restore: true,
        }),
        AgentClient::Pi => Some(AcpLaunch {
            candidates: &[("pi-acp", &[])],
            // Pi 0.80 exposes `--mode rpc`, its own protocol, not ACP.
            native_program: Some("pi"),
            model_selection: AcpModelSelection::ConfigOption,
            default_model: None,
            acp_package: Some("pi-acp"),
            install_hint: "npm install -g pi-acp",
            requires_durable_restore: true,
            supports_durable_restore: true,
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

/// True when the operator did not pin a specific harness model id.
pub fn is_unspecified_acp_model(raw: Option<&str>) -> bool {
    matches!(raw.map(str::trim), None | Some("") | Some("auto"))
}

/// Model id to place on a spawn-pinned harness argv, or `None` for bridge harnesses
/// with no operator pin.
///
/// Cursor with no operator pick still receives [`CURSOR_DEFAULT_SPAWN_MODEL`] on argv so
/// the CLI default Composer Fast is not used ([#979](https://github.com/mossipcams/ajax-cli/issues/979)).
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

/// True when a harness-reported model satisfies an unspecified / Auto Cursor attach.
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

#[cfg(test)]
mod acp_launch_restore_tests {
    use super::{
        acp_admits_orchestration_chat, acp_launch_for_agent, AcpLaunch, AcpModelSelection,
    };
    use crate::models::AgentClient;

    #[test]
    fn acp_launch_reports_durable_restore_capability_per_harness() {
        for client in [
            AgentClient::Cursor,
            AgentClient::Codex,
            AgentClient::Claude,
            AgentClient::Pi,
        ] {
            let launch = acp_launch_for_agent(client).expect("mapped harness");
            assert!(
                launch.requires_durable_restore,
                "{client:?} Ajax Chat requires durable restore"
            );
            assert!(
                launch.supports_durable_restore,
                "{client:?} must advertise durable restore support"
            );
            assert!(
                launch.admits_orchestration_chat(),
                "{client:?} should admit orchestration chat"
            );
            assert!(
                acp_admits_orchestration_chat(client),
                "{client:?} admission helper must match launch mapping"
            );
        }

        assert!(!acp_admits_orchestration_chat(AgentClient::Other));
        assert!(acp_launch_for_agent(AgentClient::Other).is_none());
    }

    #[test]
    fn acp_launch_blocks_orchestration_chat_when_restore_unsupported() {
        let launch = AcpLaunch {
            candidates: &[("fake-acp", &[])],
            native_program: None,
            model_selection: AcpModelSelection::ConfigOption,
            default_model: None,
            acp_package: Some("fake-acp"),
            install_hint: "install fake-acp",
            requires_durable_restore: true,
            supports_durable_restore: false,
        };
        assert!(
            !launch.admits_orchestration_chat(),
            "unsupported restore must not advertise resumable Ajax Chat"
        );
    }
}
