//! Unit tests for [`super::config_options`].

use super::config_option_descriptors::config_option_descriptors;
use super::config_options::*;
use agent_client_protocol::schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory, SessionConfigOptionValue,
    SessionConfigSelectOption, SessionConfigValueId,
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
fn session_model_for_task_persist_encodes_catalog_and_bracket_ids() {
    assert_eq!(
        session_model_for_task_persist("cursor-grok-4.6-high"),
        "grok-4.6|effort=high|fast=false",
    );
    assert_eq!(
        session_model_for_task_persist("claude-opus-5-thinking-high"),
        "claude-opus-5|thinking=true|effort=high|fast=false",
    );
    assert_eq!(
        session_model_for_task_persist("claude-opus-5[thinking=true,effort=high,fast=false]"),
        "claude-opus-5|thinking=true|effort=high|fast=false",
    );
    assert_eq!(session_model_for_task_persist("auto"), "auto");
    assert_eq!(session_model_for_task_persist("codex-model"), "codex-model");
}

#[test]
fn applied_model_id_for_persist_encodes_pipe_from_advertised_options() {
    let options = parameterized_options();
    assert_eq!(
        applied_model_id_for_persist(Some(&options)).as_deref(),
        Some("composer-2.5|effort=high|fast=true")
    );

    let mut grok = reasoning_id_options();
    if let SessionConfigKind::Select(select) = &mut grok[0].kind {
        select.current_value = SessionConfigValueId::from("grok-4.6");
    }
    assert_eq!(
        applied_model_id_for_persist(Some(&grok)).as_deref(),
        Some("grok-4.6|effort=high|fast=false")
    );
}

/// Cursor parameterized picker advertises Fast as a true/false select ([#1014]).
fn cursor_fast_select_options() -> Vec<SessionConfigOption> {
    vec![
        SessionConfigOption::select(
            "model",
            "Model",
            "composer-2.5",
            vec![
                SessionConfigSelectOption::new("composer-2.5", "Composer 2.5"),
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
    ]
}

#[test]
fn fast_select_option_is_advertised_and_persistable_issue_1014() {
    let options = cursor_fast_select_options();
    let fast = model_config_boolean_option(&options).expect("fast option");
    assert_eq!(read_fast_current_value(fast), Some(false));
    assert!(option_triggers_model_persist(&options, "fast"));
    assert!(!option_triggers_model_persist(&options, "effort"));
    let steps = map_pin_to_apply_steps(&options, "composer-2.5|effort=high|fast=true", true)
        .expect("mapped");
    let fast_step = steps
        .iter()
        .find(|step| step.config_id == "fast")
        .expect("fast step");
    assert_eq!(fast_step.value, SessionConfigOptionValue::value_id("true"));
    let mut on = options.clone();
    if let SessionConfigKind::Select(select) = &mut on[2].kind {
        select.current_value = SessionConfigValueId::from("true");
    }
    assert_eq!(
        applied_model_id_for_persist(Some(&on)).as_deref(),
        Some("composer-2.5|effort=high|fast=true")
    );
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
fn split_apply_skips_thought_level_when_not_advertised() {
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
    let steps = map_pin_to_apply_steps(&options, "cursor-grok-4.6-high", true).expect("mapped");
    assert!(steps.iter().any(|step| {
        step.config_id == "model"
            && step.value
                == SessionConfigOptionValue::value_id(SessionConfigValueId::from("grok-4.6"))
    }));
    assert!(steps.iter().any(|step| {
        step.config_id == "fast" && step.value == SessionConfigOptionValue::boolean(false)
    }));
    assert!(!steps
        .iter()
        .any(|step| step.config_id == "effort" || step.config_id == "reasoning"));
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
fn map_pin_skips_cursor_reasoning_option_issue_1010() {
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
        SessionConfigOption::select(
            "reasoning",
            "Reasoning",
            "high",
            vec![
                SessionConfigSelectOption::new("high", "High"),
                SessionConfigSelectOption::new("medium", "Medium"),
                SessionConfigSelectOption::new("low", "Low"),
            ],
        )
        .category(SessionConfigOptionCategory::ThoughtLevel),
        SessionConfigOption::boolean("fast", "Fast", true)
            .category(SessionConfigOptionCategory::ModelConfig),
    ];
    let steps = map_pin_to_apply_steps(&options, "cursor-grok-4.6-medium", true).expect("mapped");
    assert!(steps.iter().any(|step| {
        step.config_id == "model" && step.value == SessionConfigOptionValue::value_id("grok-4.6")
    }));
    assert!(
        !steps.iter().any(|step| step.config_id == "reasoning"),
        "Cursor reasoning is not a writable config option: {steps:?}"
    );
    assert!(steps.iter().any(|step| {
        step.config_id == "fast" && step.value == SessionConfigOptionValue::boolean(false)
    }));
}

#[test]
fn map_pin_prefers_effort_over_advertised_reasoning_issue_1010() {
    let options = vec![
        SessionConfigOption::select(
            "model",
            "Model",
            "grok-4.6",
            vec![
                SessionConfigSelectOption::new("composer-2.5", "Composer"),
                SessionConfigSelectOption::new("grok-4.6", "Grok 4.6"),
            ],
        )
        .category(SessionConfigOptionCategory::Model),
        SessionConfigOption::select(
            "reasoning",
            "Reasoning",
            "high",
            vec![
                SessionConfigSelectOption::new("high", "High"),
                SessionConfigSelectOption::new("medium", "Medium"),
            ],
        )
        .category(SessionConfigOptionCategory::ThoughtLevel),
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
        SessionConfigOption::boolean("fast", "Fast", false)
            .category(SessionConfigOptionCategory::ModelConfig),
    ];
    let steps = map_pin_to_apply_steps(&options, "cursor-grok-4.6-medium", true).expect("mapped");
    assert!(steps.iter().any(|step| {
        step.config_id == "effort" && step.value == SessionConfigOptionValue::value_id("medium")
    }));
    assert!(
        !steps.iter().any(|step| step.config_id == "reasoning"),
        "must not send Cursor's rejected reasoning id: {steps:?}"
    );
}

#[test]
fn map_pin_to_apply_steps_issue_1010_exploded_catalog_skips_reasoning() {
    let options = vec![
        SessionConfigOption::select(
            "model",
            "Model",
            "composer-2.5",
            vec![
                SessionConfigSelectOption::new("composer-2.5", "Composer"),
                SessionConfigSelectOption::new("gpt-5.2-high", "GPT 5.2 High"),
                SessionConfigSelectOption::new("gpt-5.2", "GPT 5.2"),
            ],
        )
        .category(SessionConfigOptionCategory::Model),
        SessionConfigOption::select(
            "reasoning",
            "Reasoning",
            "high",
            vec![
                SessionConfigSelectOption::new("high", "High"),
                SessionConfigSelectOption::new("low", "Low"),
            ],
        )
        .category(SessionConfigOptionCategory::ThoughtLevel),
        SessionConfigOption::boolean("fast", "Fast", false)
            .category(SessionConfigOptionCategory::ModelConfig),
    ];
    let steps = map_pin_to_apply_steps(&options, "gpt-5.2-high", true).expect("mapped");
    assert!(steps.iter().any(|step| {
        step.config_id == "model"
            && step.value == SessionConfigOptionValue::value_id("gpt-5.2-high")
    }));
    assert!(
        !steps
            .iter()
            .any(|step| step.config_id == "reasoning" || step.config_id == "effort"),
        "exploded catalog id must not produce a thought_level step: {steps:?}"
    );
}

/// Model select only (no thought_level, no Fast boolean): Fast tier lives in exploded ids.
fn model_select_fast_only_options() -> Vec<SessionConfigOption> {
    vec![SessionConfigOption::select(
        "model",
        "Model",
        "grok-4.6",
        vec![
            SessionConfigSelectOption::new("grok-4.6", "Grok 4.6"),
            SessionConfigSelectOption::new("grok-4.6-fast", "Grok 4.6 Fast"),
            SessionConfigSelectOption::new("composer-2.5", "Composer"),
        ],
    )
    .category(SessionConfigOptionCategory::Model)]
}

#[test]
fn map_pin_model_select_only_skips_unadvertised_effort_issue_1011() {
    let options = model_select_fast_only_options();
    let steps = map_pin_to_apply_steps(&options, "cursor-grok-4.6-high", true).expect("mapped");
    assert_eq!(steps.len(), 1);
    assert!(steps.iter().any(|step| {
        step.config_id == "model" && step.value == SessionConfigOptionValue::value_id("grok-4.6")
    }));
    assert!(
        !steps
            .iter()
            .any(|step| step.value == SessionConfigOptionValue::value_id("grok-4.6-fast")),
        "non-Fast pin must not map to the Fast exploded id: {steps:?}"
    );
}

#[test]
fn map_pin_high_maps_to_base_when_high_exists_on_other_models_issue_1011() {
    let options = vec![SessionConfigOption::select(
        "model",
        "Model",
        "grok-4.6",
        vec![
            SessionConfigSelectOption::new("grok-4.6", "Grok 4.6"),
            SessionConfigSelectOption::new("grok-4.6-fast", "Grok 4.6 Fast"),
            SessionConfigSelectOption::new("gpt-5.6-sol-high", "GPT 5.6 Sol High"),
            SessionConfigSelectOption::new("composer-2.5", "Composer"),
        ],
    )
    .category(SessionConfigOptionCategory::Model)];
    let steps = map_pin_to_apply_steps(&options, "cursor-grok-4.6-high", true).expect("mapped");
    assert_eq!(steps.len(), 1);
    assert!(steps.iter().any(|step| {
        step.config_id == "model" && step.value == SessionConfigOptionValue::value_id("grok-4.6")
    }));
    assert!(
        !steps
            .iter()
            .any(|step| step.value == SessionConfigOptionValue::value_id("gpt-5.6-sol-high")),
        "Grok High pin must not map to a different model's High id: {steps:?}"
    );
    assert!(
        pin_satisfied(Some(&options), "cursor-grok-4.6-high", true),
        "effort-less grok-4.6 must satisfy Grok High when Grok High is unadvertised"
    );
}

#[test]
fn map_pin_high_maps_to_base_when_only_fast_high_is_advertised_issue_1011() {
    let options = vec![SessionConfigOption::select(
        "model",
        "Model",
        "grok-4.6",
        vec![
            SessionConfigSelectOption::new("grok-4.6", "Grok 4.6"),
            SessionConfigSelectOption::new("grok-4.6-high-fast", "Grok 4.6 High Fast"),
            SessionConfigSelectOption::new("grok-4.6-fast", "Grok 4.6 Fast"),
        ],
    )
    .category(SessionConfigOptionCategory::Model)];
    let steps = map_pin_to_apply_steps(&options, "cursor-grok-4.6-high", true).expect("mapped");
    assert_eq!(steps.len(), 1);
    assert!(steps.iter().any(|step| {
        step.config_id == "model" && step.value == SessionConfigOptionValue::value_id("grok-4.6")
    }));
    assert!(
        !steps
            .iter()
            .any(|step| { step.value == SessionConfigOptionValue::value_id("grok-4.6-high-fast") }),
        "non-Fast High pin must not map to Fast High: {steps:?}"
    );
}

#[test]
fn map_pin_catalog_family_id_maps_to_grok_base_issue_1011() {
    let options = vec![SessionConfigOption::select(
        "model",
        "Model",
        "cursor-grok-4.6",
        vec![
            SessionConfigSelectOption::new("cursor-grok-4.6", "Grok 4.6"),
            SessionConfigSelectOption::new("cursor-grok-4.6-fast", "Grok 4.6 Fast"),
            SessionConfigSelectOption::new("composer-2.5", "Composer"),
        ],
    )
    .category(SessionConfigOptionCategory::Model)];
    let steps = map_pin_to_apply_steps(&options, "cursor-grok-4.6-high", true).expect("mapped");
    assert_eq!(steps.len(), 1);
    assert!(steps.iter().any(|step| {
        step.config_id == "model"
            && step.value == SessionConfigOptionValue::value_id("cursor-grok-4.6")
    }));
}

#[test]
fn map_pin_live_handshake_fast_high_only_still_refuses_non_fast_issue_997() {
    let options = vec![SessionConfigOption::select(
        "model",
        "Model",
        "composer-2.5[fast=true]",
        vec![
            SessionConfigSelectOption::new("harness-default", "Harness default"),
            SessionConfigSelectOption::new("composer-2.5", "Composer 2.5"),
            SessionConfigSelectOption::new("gpt-5.6-sol[medium]", "GPT-5.6-Sol (medium)"),
            SessionConfigSelectOption::new("composer-2.5[fast=true]", "Composer Fast"),
            SessionConfigSelectOption::new(
                "gpt-5.6-sol[effort=high,fast=false]",
                "GPT-5.6-Sol High",
            ),
            SessionConfigSelectOption::new("grok-4.6[effort=high,fast=true]", "Grok High Fast"),
        ],
    )
    .category(SessionConfigOptionCategory::Model)];
    let error = map_pin_to_apply_steps(&options, "cursor-grok-4.6-high", true)
        .expect_err("missing non-Fast Grok High must refuse");
    assert!(
        error.contains("matching model value"),
        "refuse must stay typed, got {error}"
    );
}

fn model_select_partial_effort_options() -> Vec<SessionConfigOption> {
    vec![SessionConfigOption::select(
        "model",
        "Model",
        "grok-4.6",
        vec![
            SessionConfigSelectOption::new("grok-4.6", "Grok 4.6"),
            SessionConfigSelectOption::new("grok-4.6-high", "Grok 4.6 High"),
            SessionConfigSelectOption::new("grok-4.6-fast", "Grok 4.6 Fast"),
        ],
    )
    .category(SessionConfigOptionCategory::Model)]
}

#[test]
fn map_pin_medium_maps_to_base_when_effort_partially_advertised_issue_1011() {
    let options = model_select_partial_effort_options();
    let steps = map_pin_to_apply_steps(&options, "cursor-grok-4.6-medium", true).expect("mapped");
    assert_eq!(steps.len(), 1);
    assert!(steps.iter().any(|step| {
        step.config_id == "model" && step.value == SessionConfigOptionValue::value_id("grok-4.6")
    }));
    assert!(
        !steps
            .iter()
            .any(|step| step.value == SessionConfigOptionValue::value_id("grok-4.6-fast")),
        "non-Fast pin must not map to the Fast exploded id: {steps:?}"
    );
    assert!(
        !steps
            .iter()
            .any(|step| step.value == SessionConfigOptionValue::value_id("grok-4.6-high")),
        "Medium pin must not map to a different advertised effort: {steps:?}"
    );
}

#[test]
fn pin_satisfied_medium_accepts_base_rejects_high_when_medium_unadvertised_issue_1011() {
    let mut options = model_select_partial_effort_options();
    assert!(
        pin_satisfied(Some(&options), "cursor-grok-4.6-medium", true),
        "default currentValue grok-4.6 must satisfy the Medium pin"
    );
    if let SessionConfigKind::Select(select) = &mut options[0].kind {
        select.current_value = SessionConfigValueId::from("grok-4.6-high");
    }
    assert!(
        !pin_satisfied(Some(&options), "cursor-grok-4.6-medium", true),
        "High currentValue must not satisfy a Medium pin"
    );
}

#[test]
fn pin_satisfied_accepts_spawn_applied_base_when_effort_unadvertised_issue_1011() {
    let mut options = model_select_fast_only_options();
    assert!(
        pin_satisfied(Some(&options), "cursor-grok-4.6-high", true),
        "default currentValue grok-4.6 must satisfy the High pin"
    );
    if let SessionConfigKind::Select(select) = &mut options[0].kind {
        select.current_value = SessionConfigValueId::from("grok-4.6-fast");
    }
    assert!(
        !pin_satisfied(Some(&options), "cursor-grok-4.6-high", true),
        "Fast exploded currentValue must not satisfy a non-Fast pin"
    );
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

/// Split handshake with non-writable `reasoning` and family-only model row ([#1013]).
fn grok_family_with_reasoning_options() -> Vec<SessionConfigOption> {
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
            "Reasoning",
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

// Regression for #1013: Low pins must not fail when effort is not a distinct model-select id.
#[test]
fn map_pin_grok_low_skips_unwritable_reasoning_issue_1013() {
    let options = grok_family_with_reasoning_options();
    for pin in ["cursor-grok-4.6-low", "cursor-grok-4.6-low-fast"] {
        let steps = map_pin_to_apply_steps(&options, pin, true)
            .unwrap_or_else(|error| panic!("mapped {pin}: {error}"));
        assert!(
            steps.iter().any(|step| {
                step.config_id == "model"
                    && step.value == SessionConfigOptionValue::value_id("grok-4.6")
            }),
            "{pin}: {steps:?}"
        );
        assert!(
            !steps.iter().any(|step| step.config_id == "reasoning"),
            "{pin} must not send configId reasoning: {steps:?}"
        );
        if pin.ends_with("-fast") {
            assert!(
                steps.iter().any(|step| {
                    step.config_id == "fast"
                        && step.value == SessionConfigOptionValue::boolean(true)
                }),
                "{pin}: fast step missing: {steps:?}"
            );
        } else {
            assert!(
                !steps.iter().any(|step| step.config_id == "fast"),
                "{pin}: non-Fast pin must not require a Fast step when already false: {steps:?}"
            );
        }
    }
}

#[test]
fn pin_satisfied_grok_low_accepts_family_row_with_reasoning_sibling_issue_1013() {
    let options = grok_family_with_reasoning_options();
    assert!(
        pin_satisfied(Some(&options), "cursor-grok-4.6-low", true),
        "family grok-4.6 must satisfy Low pin when effort is unadvertised on model select"
    );
    let mut fast_options = options.clone();
    if let SessionConfigKind::Boolean(boolean) = &mut fast_options[2].kind {
        boolean.current_value = true;
    }
    assert!(
        pin_satisfied(Some(&fast_options), "cursor-grok-4.6-low-fast", true),
        "family grok-4.6 + Fast must satisfy Low Fast pin"
    );
}

/// Split handshake with writable `effort` and family-only model row ([#1013]).
fn grok_family_with_effort_options() -> Vec<SessionConfigOption> {
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
            "effort",
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
fn pin_satisfied_grok_low_accepts_family_row_when_effort_still_high_issue_1013() {
    let options = grok_family_with_effort_options();
    assert!(
        pin_satisfied(Some(&options), "cursor-grok-4.6-low", true),
        "family grok-4.6 must satisfy Low pin when writable effort sibling is still high"
    );
    let steps = map_pin_to_apply_steps(&options, "cursor-grok-4.6-low", true)
        .unwrap_or_else(|error| panic!("mapped: {error}"));
    assert!(
        steps.iter().any(|step| {
            step.config_id == "effort" && step.value == SessionConfigOptionValue::value_id("low")
        }),
        "map may still include best-effort effort=low: {steps:?}"
    );
    assert!(
        !steps.iter().any(|step| step.config_id == "reasoning"),
        "must not send configId reasoning: {steps:?}"
    );
}
