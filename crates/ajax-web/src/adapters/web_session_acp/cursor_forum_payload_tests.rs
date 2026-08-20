//! Live Cursor ACP `session/set_config_option` shape from
//! <https://forum.cursor.com/t/acp-composer-2-5-model-is-always-used-when-auto-is-selected/162832>.
//!
//! Auto is `{ name: "Auto", value: "default" }`. Models are exploded bracket
//! tokens. GPT/Codex bake effort in `reasoning=`; Claude uses `effort=`.
//! There is no sibling `reasoning` / `effort` / Fast option — apply only
//! `configId: "model"` with an advertised value verbatim.

use super::config_options::{map_pin_to_apply_steps, pin_satisfied, ConfigApplyStep};
use agent_client_protocol::schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory, SessionConfigOptionValue,
    SessionConfigSelectOption,
};

const COMPOSER: &str = "composer-2.5[fast=false]";
const OPUS: &str = "claude-opus-4-8[thinking=true,context=300k,effort=high,fast=false]";
const GPT_55: &str = "gpt-5.5[context=272k,reasoning=medium,fast=false]";
const GPT_52: &str = "gpt-5.2[reasoning=medium,fast=false]";
const GPT_52_CODEX: &str = "gpt-5.2-codex[reasoning=medium,fast=false]";
const GPT_54_MINI: &str = "gpt-5.4-mini[reasoning=medium]";
const GEMINI: &str = "gemini-3.1-pro";
const OPUS_45: &str = "claude-opus-4-5[thinking=true]";
const HAIKU_45: &str = "claude-haiku-4-5[thinking=true]";
const SONNET_4: &str = "claude-sonnet-4[thinking=false,context=200k]";
const OPUS_5_THINKING: &str = "claude-opus-5[thinking=true,context=200k,effort=medium,fast=false]";
const GROK: &str = "grok-4.6[effort=high,fast=false]";

fn forum_model_choices() -> Vec<SessionConfigSelectOption> {
    vec![
        SessionConfigSelectOption::new("default", "Auto"),
        SessionConfigSelectOption::new(COMPOSER, "Composer 2.5"),
        SessionConfigSelectOption::new(OPUS, "Opus 4.8"),
        SessionConfigSelectOption::new(GPT_55, "GPT-5.5"),
        SessionConfigSelectOption::new(OPUS_45, "Opus 4.5"),
        SessionConfigSelectOption::new(GPT_52, "GPT-5.2"),
        SessionConfigSelectOption::new(GPT_52_CODEX, "GPT-5.2 Codex"),
        SessionConfigSelectOption::new(GPT_54_MINI, "GPT-5.4 Mini"),
        SessionConfigSelectOption::new(GEMINI, "Gemini 3.1 Pro"),
        SessionConfigSelectOption::new(HAIKU_45, "Haiku 4.5 Thinking"),
        SessionConfigSelectOption::new(SONNET_4, "Sonnet 4"),
    ]
}

fn forum_cursor_options(current: &'static str) -> Vec<SessionConfigOption> {
    vec![
        SessionConfigOption::select(
            "mode",
            "Mode",
            "ask",
            vec![
                SessionConfigSelectOption::new("agent", "Agent"),
                SessionConfigSelectOption::new("plan", "Plan"),
                SessionConfigSelectOption::new("ask", "Ask"),
            ],
        )
        .category(SessionConfigOptionCategory::Mode),
        SessionConfigOption::select("model", "Model", current, forum_model_choices())
            .category(SessionConfigOptionCategory::Model),
    ]
}

fn forum_cursor_options_with_grok(current: &'static str) -> Vec<SessionConfigOption> {
    let mut choices = forum_model_choices();
    choices.push(SessionConfigSelectOption::new(GROK, "Grok 4.6"));
    vec![
        SessionConfigOption::select(
            "mode",
            "Mode",
            "ask",
            vec![SessionConfigSelectOption::new("ask", "Ask")],
        )
        .category(SessionConfigOptionCategory::Mode),
        SessionConfigOption::select("model", "Model", current, choices)
            .category(SessionConfigOptionCategory::Model),
    ]
}

fn assert_only_model_value(steps: &[ConfigApplyStep], value: &str) {
    assert_eq!(steps.len(), 1, "{steps:?}");
    assert_eq!(steps[0].config_id, "model", "{steps:?}");
    assert_eq!(
        steps[0].value,
        SessionConfigOptionValue::value_id(value.to_string()),
        "{steps:?}"
    );
}

fn map_forum(current: &'static str, desired: &str) -> Vec<ConfigApplyStep> {
    map_pin_to_apply_steps(&forum_cursor_options(current), desired, true).expect("mapped")
}

fn forum_options_with_current(advertised: &'static str) -> Vec<SessionConfigOption> {
    let mut options = forum_cursor_options(advertised);
    if let SessionConfigKind::Select(select) = &mut options[1].kind {
        select.current_value =
            agent_client_protocol::schema::v1::SessionConfigValueId::from(advertised);
    }
    options
}

#[test]
fn forum_auto_applies_default_not_composer() {
    for desired in ["auto", "default", ""] {
        let steps = map_forum(COMPOSER, desired);
        assert_only_model_value(&steps, "default");
        assert!(
            !steps.iter().any(|step| step.config_id == "reasoning"),
            "must not send configId reasoning: {steps:?}"
        );
    }
}

#[test]
fn forum_auto_is_satisfied_only_by_default() {
    let on_auto = forum_cursor_options("default");
    assert!(pin_satisfied(Some(&on_auto), "auto", true));
    assert!(map_pin_to_apply_steps(&on_auto, "auto", true)
        .expect("mapped")
        .is_empty());

    let on_composer = forum_cursor_options(COMPOSER);
    assert!(!pin_satisfied(Some(&on_composer), "auto", true));
}

#[test]
fn forum_catalog_pin_mapping_table() {
    let cases = [
        ("composer-2.5", COMPOSER),
        ("claude-opus-4-8-thinking-high", OPUS),
        ("gpt-5.5-medium", GPT_55),
        ("gpt-5.2-high", GPT_52),
        ("gpt-5.2-codex-high", GPT_52_CODEX),
        ("gpt-5.4-mini-medium", GPT_54_MINI),
        ("gemini-3.1-pro", GEMINI),
        ("claude-opus-4-5-thinking", OPUS_45),
        ("claude-opus-4-5-thinking-high", OPUS_45),
        ("claude-haiku-4-5-thinking", HAIKU_45),
        ("claude-sonnet-4", SONNET_4),
    ];
    for (pin, advertised) in cases {
        let steps = map_forum("default", pin);
        assert_only_model_value(&steps, advertised);
        assert!(
            !steps.iter().any(|step| step.config_id == "reasoning"),
            "{pin} must not send configId reasoning: {steps:?}"
        );
        let applied = forum_options_with_current(advertised);
        assert!(
            pin_satisfied(Some(&applied), pin, true),
            "{pin} must be satisfied by {advertised}"
        );
    }
}

#[test]
fn forum_non_thinking_high_does_not_map_to_thinking_opus_row() {
    let error = map_pin_to_apply_steps(
        &forum_cursor_options("default"),
        "claude-opus-4-8-high",
        true,
    )
    .expect_err("non-thinking high has no matching row");
    assert!(
        error.contains("matching model value"),
        "expected typed refusal, got {error}"
    );
    let applied = forum_options_with_current(OPUS);
    assert!(
        !pin_satisfied(Some(&applied), "claude-opus-4-8-high", true),
        "thinking=true row must not satisfy non-thinking high pin"
    );
}

#[test]
fn forum_thinking_catalog_pin_does_not_take_non_thinking_sonnet_row() {
    let error = map_pin_to_apply_steps(
        &forum_cursor_options("default"),
        "claude-sonnet-4-thinking",
        true,
    )
    .expect_err("thinking pin has no matching row");
    assert!(
        error.contains("matching model value"),
        "expected typed refusal, got {error}"
    );
    let applied = forum_options_with_current(SONNET_4);
    assert!(
        !pin_satisfied(Some(&applied), "claude-sonnet-4-thinking", true),
        "thinking=false row must not satisfy thinking catalog pin"
    );
}

#[test]
fn forum_effortless_family_id_maps_to_unique_bracket_row() {
    let steps = map_forum("default", "gpt-5.2");
    assert_only_model_value(&steps, GPT_52);
}

#[test]
fn forum_grok_catalog_pin_sends_unique_advertised_bracket() {
    let options = forum_cursor_options_with_grok("default");
    let steps = map_pin_to_apply_steps(&options, "cursor-grok-4.6-high", true).expect("mapped");
    assert_only_model_value(&steps, GROK);
    assert!(
        !steps.iter().any(|step| step.config_id == "reasoning"),
        "{steps:?}"
    );
    assert!(pin_satisfied(
        Some(&forum_cursor_options_with_grok(GROK)),
        "cursor-grok-4.6-medium",
        true
    ));
}

#[test]
fn forum_grok_low_maps_to_unique_advertised_bracket() {
    let options = forum_cursor_options_with_grok("default");
    let steps = map_pin_to_apply_steps(&options, "cursor-grok-4.6-low", true).expect("mapped");
    assert_only_model_value(&steps, GROK);
    assert!(pin_satisfied(
        Some(&forum_cursor_options_with_grok(GROK)),
        "cursor-grok-4.6-low",
        true
    ));
}

#[test]
fn forum_unadvertised_spawn_catalog_id_rewrites_to_advertised_bracket() {
    let options = forum_cursor_options_with_grok("cursor-grok-4.6-high");
    assert!(
        !pin_satisfied(Some(&options), "cursor-grok-4.6-high", true),
        "spawn catalog currentValue is not an advertised model value"
    );
    let steps = map_pin_to_apply_steps(&options, "cursor-grok-4.6-high", true).expect("mapped");
    assert_only_model_value(&steps, GROK);
}

#[test]
fn forum_auto_is_not_satisfied_by_spawn_grok_when_default_is_advertised() {
    let options = forum_cursor_options_with_grok(GROK);
    assert!(!pin_satisfied(Some(&options), "auto", true));
    let steps = map_pin_to_apply_steps(&options, "auto", true).expect("mapped");
    assert_only_model_value(&steps, "default");
}

// Regression for #1013: -thinking catalog pins map to thinking=true bracket rows verbatim.
#[test]
fn forum_claude_opus_5_thinking_pin_sends_advertised_bracket_verbatim_issue_1013() {
    let mut choices = forum_model_choices();
    choices.push(SessionConfigSelectOption::new(
        OPUS_5_THINKING,
        "Opus 5 Thinking Medium",
    ));
    let mut options = vec![
        SessionConfigOption::select("model", "Model", "default", choices)
            .category(SessionConfigOptionCategory::Model),
    ];
    let steps =
        map_pin_to_apply_steps(&options, "claude-opus-5-thinking-medium", true).expect("mapped");
    assert_only_model_value(&steps, OPUS_5_THINKING);
    assert!(
        !steps.iter().any(|step| step.config_id == "reasoning"),
        "must not send configId reasoning: {steps:?}"
    );
    if let SessionConfigKind::Select(select) = &mut options[0].kind {
        select.current_value =
            agent_client_protocol::schema::v1::SessionConfigValueId::from(OPUS_5_THINKING);
    }
    assert!(
        pin_satisfied(Some(&options), "claude-opus-5-thinking-medium", true),
        "applied thinking bracket must satisfy the -thinking catalog pin"
    );
}

// Regression for #1013: do not look up the bare claude-opus-5-thinking select value.
#[test]
fn forum_claude_opus_5_thinking_pin_does_not_use_bare_thinking_select_issue_1013() {
    let mut choices = forum_model_choices();
    choices.push(SessionConfigSelectOption::new(
        "claude-opus-5-thinking",
        "Opus 5 Thinking",
    ));
    choices.push(SessionConfigSelectOption::new(
        OPUS_5_THINKING,
        "Opus 5 Thinking Medium",
    ));
    let options = vec![
        SessionConfigOption::select("model", "Model", "default", choices)
            .category(SessionConfigOptionCategory::Model),
    ];
    let steps =
        map_pin_to_apply_steps(&options, "claude-opus-5-thinking-medium", true).expect("mapped");
    assert_only_model_value(&steps, OPUS_5_THINKING);
}
