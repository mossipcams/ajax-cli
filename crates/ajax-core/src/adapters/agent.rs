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
    /// Catalog parse keeps `-thinking` in `base` ([#1004]); bracket ids use the
    /// family stem without that suffix.
    pub base: String,
    /// `true` for `-thinking` catalog suffixes or `thinking=true` bracket/pipe
    /// rows; `false` for explicit `thinking=false` or omitted thinking.
    pub thinking: Option<bool>,
    pub effort: Option<String>,
    pub fast: Option<bool>,
}

/// Family stem for matching: strip a trailing `-thinking` from catalog bases.
pub fn cursor_family_stem(base: &str) -> &str {
    base.strip_suffix("-thinking").unwrap_or(base)
}

impl CursorModelIntent {
    pub fn canonical_thinking(&self) -> bool {
        self.thinking
            .unwrap_or_else(|| self.base.ends_with("-thinking"))
    }
}

fn finalize_cursor_model_intent(
    base: String,
    thinking: Option<bool>,
    effort: Option<String>,
    fast: Option<bool>,
) -> CursorModelIntent {
    CursorModelIntent {
        thinking: thinking.or_else(|| Some(base.ends_with("-thinking"))),
        base,
        effort,
        fast: Some(fast.unwrap_or(false)),
    }
}

/// Parse a Cursor catalog id, pipe-form selection, or ACP bracket id into intent.
pub fn parse_cursor_model_intent(raw: &str) -> Option<CursorModelIntent> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "auto" || raw == "default" {
        return None;
    }
    if raw.contains('|') {
        let selection = parse_model_selection(raw)?;
        let mut thinking = None;
        let mut effort = None;
        let mut fast = None;
        for (key, value) in &selection.options {
            match key.as_str() {
                "effort" | "reasoning" => effort = Some(value.clone()),
                "fast" => fast = Some(value == "true"),
                "thinking" => thinking = Some(value == "true"),
                _ => {}
            }
        }
        return Some(finalize_cursor_model_intent(
            selection.model,
            thinking,
            effort,
            fast,
        ));
    }
    if let Some((base, bracket)) = raw.split_once('[') {
        let bracket = bracket.strip_suffix(']')?;
        let mut thinking = None;
        let mut effort = None;
        let mut fast = None;
        for part in bracket.split(',') {
            let (key, value) = part.split_once('=')?;
            match key.trim() {
                "effort" | "reasoning" => effort = Some(value.trim().to_string()),
                "fast" => fast = Some(value.trim() == "true"),
                "thinking" => thinking = Some(value.trim() == "true"),
                _ => {}
            }
        }
        return Some(finalize_cursor_model_intent(
            base.to_string(),
            thinking,
            effort,
            fast,
        ));
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
                return Some(finalize_cursor_model_intent(
                    format!("grok-{version}"),
                    Some(false),
                    Some(effort.to_string()),
                    Some(fast),
                ));
            }
        }
        if !rest.is_empty() {
            return Some(finalize_cursor_model_intent(
                format!("grok-{rest}"),
                Some(false),
                None,
                Some(fast),
            ));
        }
    }

    if let Some((prefix, effort)) = stem.rsplit_once('-') {
        if prefix.ends_with("-thinking") && CURSOR_EFFORT_SUFFIXES.contains(&effort) {
            return Some(finalize_cursor_model_intent(
                prefix.to_string(),
                Some(true),
                Some(effort.to_string()),
                Some(fast),
            ));
        }
    }

    for effort in CURSOR_EFFORT_SUFFIXES {
        if let Some(base) = stem.strip_suffix(&format!("-{effort}")) {
            return Some(finalize_cursor_model_intent(
                base.to_string(),
                Some(base.ends_with("-thinking")),
                Some(effort.to_string()),
                Some(fast),
            ));
        }
    }

    Some(finalize_cursor_model_intent(
        stem.to_string(),
        Some(stem.ends_with("-thinking")),
        None,
        Some(fast),
    ))
}

/// True when catalog pin and advertised intents share the same family stem and
/// thinking axis ([#1013](https://github.com/mossipcams/ajax-cli/issues/1013)).
pub fn cursor_thinking_bases_match(
    desired_base: &str,
    applied_base: &str,
    _applied_raw: &str,
) -> bool {
    cursor_family_stem(desired_base) == cursor_family_stem(applied_base)
}

/// True when `applied` satisfies `desired` on every canonical axis.
pub fn cursor_model_intents_match(
    desired: &CursorModelIntent,
    applied: &CursorModelIntent,
) -> bool {
    cursor_model_intents_match_with_raw(desired, applied, "")
}

/// Like [`cursor_model_intents_match`]; `applied_raw` is ignored — matching uses
/// canonical axes parsed from the advertised id ([#1013]).
pub fn cursor_model_intents_match_with_raw(
    desired: &CursorModelIntent,
    applied: &CursorModelIntent,
    _applied_raw: &str,
) -> bool {
    if desired.canonical_thinking() != applied.canonical_thinking() {
        return false;
    }
    if cursor_family_stem(&desired.base) != cursor_family_stem(&applied.base) {
        return false;
    }
    if desired.effort != applied.effort {
        return false;
    }
    desired.fast.unwrap_or(false) == applied.fast.unwrap_or(false)
}

/// Reconstruct an exploded Cursor catalog id from a parsed intent.
///
/// Pipe-form picker selections persist base / effort / fast separately ([#991]);
/// spawn argv must receive the catalog id Cursor accepts on `--model` ([#989]).
/// Only `grok-*` bases receive the `cursor-grok-*` prefix; effort and fast suffixes
/// are appended deterministically without inferring thinking variants.
fn compose_cursor_catalog_id_from_intent(intent: &CursorModelIntent) -> String {
    let fast = intent.fast.unwrap_or(false);
    let mut id = if let Some(version) = intent.base.strip_prefix("grok-") {
        format!("cursor-grok-{version}")
    } else if intent.canonical_thinking() && !intent.base.ends_with("-thinking") {
        format!("{}-thinking", intent.base)
    } else {
        intent.base.clone()
    };
    if let Some(effort) = &intent.effort {
        id.push('-');
        id.push_str(effort);
    }
    if fast {
        id.push_str("-fast");
    }
    id
}

/// Encode canonical Cursor intent as Ajax task `session_model` pipe storage.
///
/// Storage keeps the family base (without `-thinking`), optional `thinking=true`,
/// effort, and fast axes — never catalog ids or ACP bracket tokens.
pub fn encode_cursor_intent_to_storage_pipe(intent: &CursorModelIntent) -> String {
    let base = intent
        .base
        .strip_suffix("-thinking")
        .unwrap_or(intent.base.as_str())
        .to_string();
    let mut options = Vec::new();
    if intent.canonical_thinking() {
        options.push(("thinking".to_string(), "true".to_string()));
    }
    if let Some(effort) = &intent.effort {
        options.push(("effort".to_string(), effort.clone()));
    }
    if let Some(fast) = intent.fast {
        options.push((
            "fast".to_string(),
            if fast { "true" } else { "false" }.to_string(),
        ));
    }
    ModelSelection {
        model: base,
        options,
    }
    .encode()
}

/// Map an Ajax catalog id or pipe-form selection to the token Cursor accepts on spawn `--model`.
///
/// Live Cursor honors catalog ids from `agent models` on spawn argv ([#989]).
/// Pipe-form selections reconstruct those catalog ids; bracket synthesis is reserved
/// for in-band apply via [`cursor_catalog_to_acp_in_band_token`].
pub fn cursor_catalog_to_acp_spawn_token(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.contains('|') {
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
        || catalog_id.starts_with("cursor-grok-")
        || catalog_id.starts_with("composer-")
        || catalog_id.contains("-thinking-")
        || catalog_id.contains('[');
    if !uses_acp_brackets {
        return catalog_id.to_string();
    }
    let fast = intent.fast.unwrap_or(false);
    let bracket_base = cursor_family_stem(&intent.base);
    let mut options = Vec::new();
    if intent.canonical_thinking() {
        options.push("thinking=true".to_string());
    }
    if let Some(effort) = &intent.effort {
        options.push(format!("effort={effort}"));
    }
    options.push(format!("fast={fast}"));
    format!("{bracket_base}[{}]", options.join(","))
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

/// True when the operator did not pin a specific harness model id.
pub fn is_unspecified_acp_model(raw: Option<&str>) -> bool {
    matches!(
        raw.map(str::trim),
        None | Some("") | Some("auto") | Some("default")
    )
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
    if is_unspecified_acp_model(Some(applied_model)) {
        return true;
    }
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
mod tests {
    use super::{
        cursor_catalog_to_acp_in_band_token, cursor_catalog_to_acp_spawn_token, cursor_family_stem,
        cursor_model_intents_match, cursor_model_intents_match_with_raw,
        cursor_thinking_bases_match, encode_cursor_intent_to_storage_pipe,
        parse_cursor_model_intent, CursorModelIntent,
    };

    #[test]
    fn parse_cursor_model_intent_reads_forum_reasoning_and_skips_auto_default() {
        assert!(parse_cursor_model_intent("auto").is_none());
        assert!(parse_cursor_model_intent("default").is_none());

        let gpt =
            parse_cursor_model_intent("gpt-5.5[context=272k,reasoning=medium,fast=false]").unwrap();
        assert_eq!(gpt.base, "gpt-5.5");
        assert_eq!(gpt.effort.as_deref(), Some("medium"));
        assert_eq!(gpt.fast, Some(false));
        assert!(!gpt.canonical_thinking());

        let claude = parse_cursor_model_intent(
            "claude-opus-4-8[thinking=true,context=300k,effort=high,fast=false]",
        )
        .unwrap();
        assert_eq!(claude.base, "claude-opus-4-8");
        assert_eq!(claude.effort.as_deref(), Some("high"));
        assert_eq!(claude.fast, Some(false));
        assert!(claude.canonical_thinking());

        let pipe = parse_cursor_model_intent("gpt-5.2|reasoning=medium|fast=false").unwrap();
        assert_eq!(pipe.base, "gpt-5.2");
        assert_eq!(pipe.effort.as_deref(), Some("medium"));
        assert!(!pipe.canonical_thinking());

        let non_thinking =
            parse_cursor_model_intent("claude-sonnet-4[thinking=false,context=200k]").unwrap();
        assert!(!non_thinking.canonical_thinking());
    }

    #[test]
    fn parse_cursor_model_intent_accepts_pipe_form_issue_991() {
        let intent = parse_cursor_model_intent("grok-4.6|effort=high|fast=false").unwrap();
        assert_eq!(intent.base, "grok-4.6");
        assert_eq!(intent.effort.as_deref(), Some("high"));
        assert_eq!(intent.fast, Some(false));

        let fast = parse_cursor_model_intent("grok-4.6|effort=high|fast=true").unwrap();
        assert_eq!(fast.fast, Some(true));
    }

    #[test]
    fn parse_cursor_model_intent_keeps_thinking_in_base_issue_1004() {
        let thinking = parse_cursor_model_intent("claude-opus-5-thinking-high").unwrap();
        assert_eq!(thinking.base, "claude-opus-5-thinking");
        assert_eq!(thinking.effort.as_deref(), Some("high"));
        assert!(thinking.canonical_thinking());

        let plain = parse_cursor_model_intent("claude-opus-5-high").unwrap();
        assert_eq!(plain.base, "claude-opus-5");
        assert_eq!(plain.effort.as_deref(), Some("high"));
        assert!(!plain.canonical_thinking());
    }

    #[test]
    fn parse_cursor_model_intent_maps_effortless_cursor_grok_to_grok_base() {
        let base = parse_cursor_model_intent("cursor-grok-4.6").unwrap();
        assert_eq!(base.base, "grok-4.6");
        assert_eq!(base.effort, None);
        assert_eq!(base.fast, Some(false));

        let fast = parse_cursor_model_intent("cursor-grok-4.6-fast").unwrap();
        assert_eq!(fast.base, "grok-4.6");
        assert_eq!(fast.effort, None);
        assert_eq!(fast.fast, Some(true));
    }

    #[test]
    fn cursor_family_stem_strips_trailing_thinking_suffix() {
        assert_eq!(
            cursor_family_stem("claude-opus-5-thinking"),
            "claude-opus-5"
        );
        assert_eq!(cursor_family_stem("claude-opus-5"), "claude-opus-5");
    }

    #[test]
    fn cursor_model_intents_match_requires_matching_fast_issue_979() {
        let desired = parse_cursor_model_intent("cursor-grok-4.6-high").unwrap();
        let non_fast = parse_cursor_model_intent("grok-4.6[effort=high,fast=false]").unwrap();
        let fast = parse_cursor_model_intent("grok-4.6[effort=high,fast=true]").unwrap();
        let composer_fast = parse_cursor_model_intent("composer-2.5[fast=true]").unwrap();
        assert!(cursor_model_intents_match(&desired, &non_fast));
        assert!(!cursor_model_intents_match(&desired, &fast));
        assert!(!cursor_model_intents_match(&desired, &composer_fast));
    }

    // Regression for #1013: thinking=true bracket rows match -thinking catalog suffixes.
    #[test]
    fn cursor_thinking_bases_match_maps_bracket_thinking_to_catalog_suffix_issue_1013() {
        let bracket = "claude-opus-5[thinking=true,context=200k,effort=medium,fast=false]";
        let applied = parse_cursor_model_intent(bracket).unwrap();
        assert_eq!(applied.base, "claude-opus-5");
        assert_eq!(applied.effort.as_deref(), Some("medium"));
        assert!(applied.canonical_thinking());

        let pin = parse_cursor_model_intent("claude-opus-5-thinking-medium").unwrap();
        assert_eq!(pin.base, "claude-opus-5-thinking");
        assert!(pin.canonical_thinking());
        assert!(cursor_thinking_bases_match(
            "claude-opus-5-thinking",
            "claude-opus-5",
            bracket
        ));
        assert!(cursor_model_intents_match_with_raw(&pin, &applied, bracket));

        let pipe =
            parse_cursor_model_intent("claude-opus-5|thinking=true|effort=medium|fast=false")
                .unwrap();
        assert_eq!(pipe.base, "claude-opus-5");
        assert!(pipe.canonical_thinking());
        assert!(cursor_thinking_bases_match(
            "claude-opus-5-thinking",
            "claude-opus-5",
            "claude-opus-5|thinking=true|effort=medium|fast=false"
        ));

        let plain = parse_cursor_model_intent("claude-opus-5-high").unwrap();
        assert!(!plain.canonical_thinking());
        assert!(!cursor_model_intents_match_with_raw(
            &plain, &applied, bracket
        ));
    }

    #[test]
    fn cursor_model_intents_match_separates_thinking_and_non_thinking_families() {
        let thinking_row = parse_cursor_model_intent(
            "claude-opus-4-8[thinking=true,context=300k,effort=high,fast=false]",
        )
        .unwrap();
        let thinking_pin = parse_cursor_model_intent("claude-opus-4-8-thinking-high").unwrap();
        let non_thinking_pin = parse_cursor_model_intent("claude-opus-4-8-high").unwrap();
        assert!(cursor_model_intents_match(&thinking_pin, &thinking_row));
        assert!(!cursor_model_intents_match(
            &non_thinking_pin,
            &thinking_row
        ));

        let non_thinking_row =
            parse_cursor_model_intent("claude-sonnet-4[thinking=false,context=200k]").unwrap();
        let non_thinking_catalog = parse_cursor_model_intent("claude-sonnet-4").unwrap();
        let thinking_catalog = parse_cursor_model_intent("claude-sonnet-4-thinking").unwrap();
        assert!(cursor_model_intents_match(
            &non_thinking_catalog,
            &non_thinking_row
        ));
        assert!(!cursor_model_intents_match(
            &thinking_catalog,
            &non_thinking_row
        ));
    }

    #[test]
    fn cursor_catalog_to_acp_in_band_token_uses_stem_and_thinking_for_catalog_pins() {
        assert_eq!(
            cursor_catalog_to_acp_in_band_token("claude-opus-5-thinking-medium"),
            "claude-opus-5[thinking=true,effort=medium,fast=false]"
        );
    }

    #[test]
    fn encode_cursor_intent_to_storage_pipe_uses_structured_axes() {
        let grok = parse_cursor_model_intent("grok-4.6|effort=high|fast=false").unwrap();
        assert_eq!(
            encode_cursor_intent_to_storage_pipe(&grok),
            "grok-4.6|effort=high|fast=false"
        );

        let thinking =
            parse_cursor_model_intent("claude-opus-5|thinking=true|effort=medium|fast=false")
                .unwrap();
        assert_eq!(
            encode_cursor_intent_to_storage_pipe(&thinking),
            "claude-opus-5|thinking=true|effort=medium|fast=false"
        );
        assert_eq!(
            cursor_catalog_to_acp_spawn_token(
                "claude-opus-5|thinking=true|effort=medium|fast=false"
            ),
            "claude-opus-5-thinking-medium"
        );
    }
}
