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

#[test]
fn apply_steps_needing_send_recovers_when_model_set_resets_siblings_issue_997() {
    let target_steps =
        map_pin_to_apply_steps(&parameterized_options(), "cursor-grok-4.6-high", true)
            .expect("mapped");
    let mut options = parameterized_options();
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
    assert_eq!(first_pending.len(), 1);
    assert_eq!(first_pending[0].config_id, "model");

    if let SessionConfigKind::Select(select) = &mut options[0].kind {
        select.current_value = SessionConfigValueId::from("grok-4.6");
    }
    if let SessionConfigKind::Select(select) = &mut options[1].kind {
        select.current_value = SessionConfigValueId::from("medium");
    }
    if let SessionConfigKind::Boolean(boolean) = &mut options[2].kind {
        boolean.current_value = true;
    }
    let second_pending = apply_steps_needing_send(&options, &target_steps);
    assert!(second_pending.iter().any(|step| step.config_id == "effort"));
    assert!(second_pending.iter().any(|step| step.config_id == "fast"));
    assert!(!pin_satisfied(Some(&options), "cursor-grok-4.6-high", true));

    if let SessionConfigKind::Select(select) = &mut options[1].kind {
        select.current_value = SessionConfigValueId::from("high");
    }
    if let SessionConfigKind::Boolean(boolean) = &mut options[2].kind {
        boolean.current_value = false;
    }
    assert!(apply_steps_needing_send(&options, &target_steps).is_empty());
    assert!(pin_satisfied(Some(&options), "cursor-grok-4.6-high", true));
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
