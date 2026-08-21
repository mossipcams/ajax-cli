//! Unit tests for [`super::config_options`].

use super::config_option_descriptors::config_option_descriptors;
use super::config_options::*;
use agent_client_protocol::schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory, SessionConfigOptionValue,
    SessionConfigSelectOption, SessionConfigSelectOptions, SessionConfigValueId,
};
use serde_json::json;

fn parameterized_options() -> Vec<SessionConfigOption> {
    vec![
        SessionConfigOption::select(
            "model",
            "Model",
            "composer-2.5",
            vec![
                SessionConfigSelectOption::new("composer-2.5", "Composer"),
                SessionConfigSelectOption::new("grok-4.6", "Grok 4.6"),
            ],
        )
        .category(SessionConfigOptionCategory::Model),
        SessionConfigOption::select(
            "effort",
            "Effort",
            "high",
            vec![
                SessionConfigSelectOption::new("high", "High"),
                SessionConfigSelectOption::new("medium", "Medium"),
            ],
        )
        .category(SessionConfigOptionCategory::ThoughtLevel),
        SessionConfigOption::boolean("fast", "Fast", true)
            .category(SessionConfigOptionCategory::ModelConfig),
    ]
}

#[test]
fn read_model_applied_uses_model_current_value_only() {
    let options = parameterized_options();
    assert_eq!(
        read_model_applied(Some(&options)).as_deref(),
        Some("composer-2.5")
    );
}

#[test]
fn applied_model_id_for_persist_encodes_pipe_from_advertised_options() {
    let options = parameterized_options();
    assert_eq!(
        applied_model_id_for_persist(&options).as_deref(),
        Ok("composer-2.5|effort=high|fast=true")
    );

    let mut grok = reasoning_id_options();
    if let SessionConfigKind::Select(select) = &mut grok[0].kind {
        select.current_value = SessionConfigValueId::from("grok-4.6");
    }
    assert_eq!(
        applied_model_id_for_persist(&grok).as_deref(),
        Ok("grok-4.6|reasoning=high|fast=false")
    );

    if let SessionConfigKind::Select(select) = &mut grok[0].kind {
        select.current_value = SessionConfigValueId::from("default[]");
    }
    assert_eq!(applied_model_id_for_persist(&grok).as_deref(), Ok("auto"));
}

#[test]
fn wire_value_to_session_value_accepts_string_and_boolean() {
    assert_eq!(
        wire_value_to_session_value(SessionConfigValue::Select("composer-2.5".to_string())),
        SessionConfigOptionValue::value_id("composer-2.5")
    );
    assert_eq!(
        wire_value_to_session_value(SessionConfigValue::Boolean(true)),
        SessionConfigOptionValue::boolean(true)
    );
}

#[test]
fn option_triggers_model_persist_for_every_restart_axis() {
    let options = parameterized_options();
    assert!(option_triggers_model_persist(&options, "model"));
    assert!(option_triggers_model_persist(&options, "fast"));
    assert!(option_triggers_model_persist(&options, "effort"));
}

#[test]
fn validate_config_change_rejects_wrong_wire_types_before_acp() {
    let options = parameterized_options();
    assert!(
        validate_config_change(&options, "model", &SessionConfigOptionValue::boolean(true))
            .is_err()
    );
    assert!(validate_config_change(
        &options,
        "fast",
        &SessionConfigOptionValue::value_id("true")
    )
    .is_err());
    assert!(validate_config_change(
        &options,
        "model",
        &SessionConfigOptionValue::value_id("not-advertised")
    )
    .is_err());
    assert!(validate_config_change(
        &options,
        "model",
        &SessionConfigOptionValue::value_id("grok-4.6")
    )
    .is_ok());
}

#[test]
fn map_pin_to_split_apply_steps_issue_997() {
    let options = parameterized_options();
    let steps = map_pin_to_apply_steps(&options, "cursor-grok-4.6-high", true).expect("mapped");
    assert!(steps.iter().any(|step| {
        step.config_id == "model"
            && step.value
                == SessionConfigOptionValue::value_id(SessionConfigValueId::from("grok-4.6"))
    }));
    assert!(steps.iter().any(|step| {
        step.config_id == "effort" && step.value == SessionConfigOptionValue::value_id("high")
    }));
    assert!(steps.iter().any(|step| {
        step.config_id == "fast" && step.value == SessionConfigOptionValue::boolean(false)
    }));
}

#[test]
fn map_pin_rejects_catalog_id_as_config_value_issue_954() {
    let options = vec![SessionConfigOption::select(
        "model",
        "Model",
        "harness-default",
        vec![SessionConfigSelectOption::new("harness-default", "Default")],
    )];
    assert!(map_pin_to_apply_steps(&options, "cursor-grok-4.6-high", true).is_err());
}

#[test]
fn pin_satisfied_checks_each_mapped_option_current_value_issue_997() {
    let mut options = parameterized_options();
    if let SessionConfigKind::Select(select) = &mut options[0].kind {
        select.current_value = SessionConfigValueId::from("grok-4.6");
    }
    if let SessionConfigKind::Select(select) = &mut options[1].kind {
        select.current_value = SessionConfigValueId::from("high");
    }
    if let SessionConfigKind::Boolean(boolean) = &mut options[2].kind {
        boolean.current_value = false;
    }
    assert!(pin_satisfied(Some(&options), "cursor-grok-4.6-high", true));
    if let SessionConfigKind::Boolean(boolean) = &mut options[2].kind {
        boolean.current_value = true;
    }
    assert!(!pin_satisfied(Some(&options), "cursor-grok-4.6-high", true));
}

#[test]
fn boolean_fast_never_uses_string_false_on_wire() {
    let options = parameterized_options();
    let steps =
        map_pin_to_apply_steps(&options, "grok-4.6|effort=high|fast=false", true).expect("mapped");
    let fast = steps
        .iter()
        .find(|step| step.config_id == "fast")
        .expect("fast step");
    assert_eq!(fast.value, SessionConfigOptionValue::boolean(false));
}

fn model_step_is(steps: &[ConfigApplyStep], value: &str) -> bool {
    let want = SessionConfigOptionValue::value_id(value.to_string());
    steps.iter().any(|step| step.config_id == "model" && step.value == want)
}

fn thinking_pin() -> &'static str {
    "claude-opus-5-thinking|effort=high|fast=false"
}

fn exploded_thinking_options() -> Vec<SessionConfigOption> {
    vec![SessionConfigOption::select(
        "model",
        "Model",
        "composer-2.5",
        vec![
            SessionConfigSelectOption::new("composer-2.5", "Composer"),
            SessionConfigSelectOption::new(
                "claude-opus-5-thinking-high",
                "Claude Opus 5 Thinking High",
            ),
            SessionConfigSelectOption::new(
                "claude-opus-5-thinking-high-fast",
                "Claude Opus 5 Thinking High Fast",
            ),
            SessionConfigSelectOption::new("claude-opus-5-high", "Claude Opus 5 High"),
        ],
    )
    .category(SessionConfigOptionCategory::Model)]
}

fn split_thinking_options() -> Vec<SessionConfigOption> {
    vec![
        SessionConfigOption::select(
            "model",
            "Model",
            "composer-2.5",
            vec![
                SessionConfigSelectOption::new("composer-2.5", "Composer"),
                SessionConfigSelectOption::new("claude-opus-5-thinking", "Claude Opus 5 Thinking"),
            ],
        )
        .category(SessionConfigOptionCategory::Model),
        SessionConfigOption::select(
            "effort",
            "Effort",
            "medium",
            vec![
                SessionConfigSelectOption::new("high", "High"),
                SessionConfigSelectOption::new("medium", "Medium"),
            ],
        )
        .category(SessionConfigOptionCategory::ThoughtLevel),
        SessionConfigOption::boolean("fast", "Fast", true)
            .category(SessionConfigOptionCategory::ModelConfig),
    ]
}

fn both_thinking_variants_options() -> Vec<SessionConfigOption> {
    let mut options = split_thinking_options();
    if let SessionConfigKind::Select(select) = &mut options[0].kind {
        select.options = SessionConfigSelectOptions::Ungrouped(vec![
            SessionConfigSelectOption::new("composer-2.5", "Composer"),
            SessionConfigSelectOption::new("claude-opus-5-thinking", "Claude Opus 5 Thinking"),
            SessionConfigSelectOption::new(
                "claude-opus-5-thinking-high",
                "Claude Opus 5 Thinking High",
            ),
        ]);
    }
    options
}

#[test]
fn map_split_axis_sends_base_effort_and_fast() {
    let options = split_thinking_options();
    let steps = map_pin_to_apply_steps(&options, thinking_pin(), true).expect("mapped");
    assert!(model_step_is(&steps, "claude-opus-5-thinking"));
    assert!(steps.iter().any(|step| {
        step.config_id == "effort" && step.value == SessionConfigOptionValue::value_id("high")
    }));
    assert!(steps.iter().any(|step| {
        step.config_id == "fast" && step.value == SessionConfigOptionValue::boolean(false)
    }));
}

#[test]
fn map_exploded_ids_only_sends_full_intent_match() {
    let options = exploded_thinking_options();
    let steps = map_pin_to_apply_steps(&options, thinking_pin(), true).expect("mapped");
    assert!(model_step_is(&steps, "claude-opus-5-thinking-high"));
    assert_eq!(steps.len(), 1);
}

#[test]
fn map_prefers_split_axis_when_base_and_exploded_are_both_advertised() {
    let options = both_thinking_variants_options();
    let steps = map_pin_to_apply_steps(&options, thinking_pin(), true).expect("mapped");
    assert!(model_step_is(&steps, "claude-opus-5-thinking"));
    assert!(!model_step_is(&steps, "claude-opus-5-thinking-high"));
    assert!(steps.iter().any(|step| step.config_id == "effort"));
}

#[test]
fn map_rejects_when_effort_variant_is_missing() {
    let mut options = exploded_thinking_options();
    if let SessionConfigKind::Select(select) = &mut options[0].kind {
        select.options = SessionConfigSelectOptions::Ungrouped(vec![
            SessionConfigSelectOption::new("composer-2.5", "Composer"),
            SessionConfigSelectOption::new(
                "claude-opus-5-thinking-medium",
                "Claude Opus 5 Thinking Medium",
            ),
        ]);
    }
    assert!(map_pin_to_apply_steps(&options, thinking_pin(), true).is_err());

    let mut split = split_thinking_options();
    if let SessionConfigKind::Select(select) = &mut split[1].kind {
        select.options = SessionConfigSelectOptions::Ungrouped(vec![
            SessionConfigSelectOption::new("medium", "Medium"),
        ]);
    }
    assert!(map_pin_to_apply_steps(&split, thinking_pin(), true).is_err());
}

#[test]
fn map_exploded_fast_true_and_false() {
    let options = exploded_thinking_options();
    let off = map_pin_to_apply_steps(&options, thinking_pin(), true).expect("off");
    assert!(model_step_is(&off, "claude-opus-5-thinking-high"));
    let on = map_pin_to_apply_steps(
        &options,
        "claude-opus-5-thinking|effort=high|fast=true",
        true,
    )
    .expect("on");
    assert!(model_step_is(&on, "claude-opus-5-thinking-high-fast"));
}

#[test]
fn map_catalog_exploded_id_sends_advertised_exploded_value() {
    let options = exploded_thinking_options();
    let steps =
        map_pin_to_apply_steps(&options, "claude-opus-5-thinking-high", true).expect("mapped");
    assert!(model_step_is(&steps, "claude-opus-5-thinking-high"));
}

#[test]
fn persist_collapses_exploded_current_value_to_ajax_pipe_form() {
    let mut options = exploded_thinking_options();
    if let SessionConfigKind::Select(select) = &mut options[0].kind {
        select.current_value = SessionConfigValueId::from("claude-opus-5-thinking-high");
    }
    assert_eq!(
        applied_model_id_for_persist(&options).as_deref(),
        Ok(thinking_pin())
    );
    assert!(pin_satisfied(Some(&options), thinking_pin(), true));
}

#[test]
fn catalog_pin_requires_advertised_handshake_not_spawn_argv_echo_issue_997() {
    let options = vec![SessionConfigOption::select(
        "model",
        "Model",
        "cursor-grok-4.6-high",
        vec![
            SessionConfigSelectOption::new("composer-2.5[fast=true]", "Composer Fast"),
            SessionConfigSelectOption::new("grok-4.6[effort=high,fast=true]", "Grok High Fast"),
        ],
    )
    .category(SessionConfigOptionCategory::Model)];
    assert!(!pin_satisfied(
        Some(&options),
        "cursor-grok-4.6-high",
        true
    ));
}

fn reasoning_id_options() -> Vec<SessionConfigOption> {
    vec![
        SessionConfigOption::select(
            "model",
            "Model",
            "grok-4.6",
            vec![
                SessionConfigSelectOption::new("grok-4.6", "Grok 4.6"),
                SessionConfigSelectOption::new("composer-2.5", "Composer"),
            ],
        )
        .category(SessionConfigOptionCategory::Model),
        SessionConfigOption::select(
            "reasoning",
            "Effort",
            "high",
            vec![
                SessionConfigSelectOption::new("high", "High"),
                SessionConfigSelectOption::new("low", "Low"),
            ],
        )
        .category(SessionConfigOptionCategory::ThoughtLevel),
        SessionConfigOption::boolean("fast", "Fast", false)
            .category(SessionConfigOptionCategory::ModelConfig),
    ]
}

#[test]
fn map_pipe_form_uses_advertised_reasoning_id() {
    let options = reasoning_id_options();
    let steps = map_pin_to_apply_steps(&options, "grok-4.6|reasoning=high|fast=false", true)
        .expect("mapped");
    assert!(steps.iter().any(|step| {
        step.config_id == "reasoning" && step.value == SessionConfigOptionValue::value_id("high")
    }));
    assert!(steps.iter().any(|step| {
        step.config_id == "fast" && step.value == SessionConfigOptionValue::boolean(false)
    }));
}

#[test]
fn split_apply_rejects_when_effort_is_not_advertised() {
    let options = vec![
        SessionConfigOption::select(
            "model",
            "Model",
            "composer-2.5",
            vec![
                SessionConfigSelectOption::new("composer-2.5", "Composer"),
                SessionConfigSelectOption::new("grok-4.6", "Grok 4.6"),
            ],
        )
        .category(SessionConfigOptionCategory::Model),
        SessionConfigOption::boolean("fast", "Fast", true)
            .category(SessionConfigOptionCategory::ModelConfig),
    ];
    assert!(map_pin_to_apply_steps(&options, "cursor-grok-4.6-high", true).is_err());
}

#[test]
fn find_option_by_category_prefers_effort_over_reasoning_issue_1010() {
    let options = vec![
        SessionConfigOption::select(
            "reasoning",
            "Reasoning",
            "high",
            vec![SessionConfigSelectOption::new("high", "High")],
        )
        .category(SessionConfigOptionCategory::ThoughtLevel),
        SessionConfigOption::select(
            "effort",
            "Effort",
            "high",
            vec![SessionConfigSelectOption::new("high", "High")],
        )
        .category(SessionConfigOptionCategory::ThoughtLevel),
    ];
    assert_eq!(
        thought_level_option(&options).map(|option| option.id.0.as_ref()),
        Some("effort")
    );
}

#[test]
fn find_option_by_category_prefers_category_over_id() {
    let options = vec![
        SessionConfigOption::select(
            "mode",
            "Mode",
            "default",
            vec![SessionConfigSelectOption::new("default", "Default")],
        )
        .category(SessionConfigOptionCategory::Mode),
        SessionConfigOption::select(
            "effort",
            "Thinking",
            "high",
            vec![SessionConfigSelectOption::new("high", "High")],
        )
        .category(SessionConfigOptionCategory::ThoughtLevel),
    ];
    assert_eq!(
        thought_level_option(&options).map(|option| option.id.0.as_ref()),
        Some("effort")
    );
}

/// Split-catalog fixture for issue #997 sibling-reset retry: advertises every High
/// pin base model value id without changing [`parameterized_options`].
fn split_catalog_high_pin_options() -> Vec<SessionConfigOption> {
    vec![
        SessionConfigOption::select(
            "model",
            "Model",
            "composer-2.5",
            vec![
                SessionConfigSelectOption::new("composer-2.5", "Composer"),
                SessionConfigSelectOption::new("grok-4.6", "Grok 4.6"),
                SessionConfigSelectOption::new("gpt-5.6-sol", "GPT 5.6 Sol"),
                SessionConfigSelectOption::new("claude-opus-5", "Claude Opus 5"),
            ],
        )
        .category(SessionConfigOptionCategory::Model),
        SessionConfigOption::select(
            "effort",
            "Effort",
            "high",
            vec![
                SessionConfigSelectOption::new("high", "High"),
                SessionConfigSelectOption::new("medium", "Medium"),
            ],
        )
        .category(SessionConfigOptionCategory::ThoughtLevel),
        SessionConfigOption::boolean("fast", "Fast", true)
            .category(SessionConfigOptionCategory::ModelConfig),
    ]
}

#[test]
fn apply_steps_needing_send_recovers_when_model_set_resets_siblings_issue_997() {
    let cases = [
        ("cursor-grok-4.6-high", "grok-4.6"),
        ("gpt-5.6-sol-high", "gpt-5.6-sol"),
        ("claude-opus-5-high", "claude-opus-5"),
    ];
    for (catalog_pin, model_value) in cases {
        let base_options = split_catalog_high_pin_options();
        let target_steps =
            map_pin_to_apply_steps(&base_options, catalog_pin, true).expect("mapped");
        let mut options = base_options;
        if let SessionConfigKind::Select(select) = &mut options[0].kind {
            select.current_value = SessionConfigValueId::from("composer-2.5");
        }
        if let SessionConfigKind::Select(select) = &mut options[1].kind {
            select.current_value = SessionConfigValueId::from("high");
        }
        if let SessionConfigKind::Boolean(boolean) = &mut options[2].kind {
            boolean.current_value = false;
        }
        let first_pending = apply_steps_needing_send(&options, &target_steps);
        assert_eq!(first_pending.len(), 1, "{catalog_pin}: only model pending");
        assert_eq!(first_pending[0].config_id, "model");

        if let SessionConfigKind::Select(select) = &mut options[0].kind {
            select.current_value = SessionConfigValueId::from(model_value);
        }
        if let SessionConfigKind::Select(select) = &mut options[1].kind {
            select.current_value = SessionConfigValueId::from("medium");
        }
        if let SessionConfigKind::Boolean(boolean) = &mut options[2].kind {
            boolean.current_value = true;
        }
        let second_pending = apply_steps_needing_send(&options, &target_steps);
        assert!(
            second_pending.iter().any(|step| step.config_id == "effort"),
            "{catalog_pin}: effort must be re-sent after model set_config reset"
        );
        assert!(
            second_pending.iter().any(|step| step.config_id == "fast"),
            "{catalog_pin}: fast must be re-sent after model set_config reset"
        );
        assert!(
            !pin_satisfied(Some(&options), catalog_pin, true),
            "{catalog_pin}: pin must stay unsatisfied until siblings match"
        );

        if let SessionConfigKind::Select(select) = &mut options[1].kind {
            select.current_value = SessionConfigValueId::from("high");
        }
        if let SessionConfigKind::Boolean(boolean) = &mut options[2].kind {
            boolean.current_value = false;
        }
        assert!(
            apply_steps_needing_send(&options, &target_steps).is_empty(),
            "{catalog_pin}: no steps left once siblings match"
        );
        assert!(
            pin_satisfied(Some(&options), catalog_pin, true),
            "{catalog_pin}: pin satisfied when all mapped options match"
        );
    }
}

#[test]
fn config_option_descriptors_include_boolean_current_value() {
    let descriptors = config_option_descriptors(&parameterized_options());
    let fast = descriptors
        .iter()
        .find(|descriptor| descriptor.id == "fast")
        .expect("fast descriptor");
    assert_eq!(fast.kind, "boolean");
    assert_eq!(fast.current_value, json!(true));
}

/// Cursor parameterized picker advertises Fast as a true/false select ([#1014]).
#[test]
fn fast_select_option_is_advertised_and_persistable_issue_1014() {
    let options = vec![
        SessionConfigOption::select(
            "model",
            "Model",
            "composer-2.5",
            vec![SessionConfigSelectOption::new(
                "composer-2.5",
                "Composer 2.5",
            )],
        )
        .category(SessionConfigOptionCategory::Model),
        SessionConfigOption::select(
            "fast",
            "Fast",
            "false",
            vec![
                SessionConfigSelectOption::new("false", "Off"),
                SessionConfigSelectOption::new("true", "Fast"),
            ],
        )
        .category(SessionConfigOptionCategory::ModelConfig),
    ];
    let fast = model_config_boolean_option(&options).expect("fast option");
    assert_eq!(read_fast_current_value(fast), Some(false));
    assert!(option_triggers_model_persist(&options, "fast"));
    assert_eq!(
        applied_model_id_for_persist(&options).as_deref(),
        Ok("composer-2.5|fast=false")
    );
}
