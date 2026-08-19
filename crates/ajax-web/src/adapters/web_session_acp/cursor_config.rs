//! Cursor ACP parameterized model picker: split `model` / `effort` / `fast`
//! config options and reconstruct the harness-reported applied id ([#979]).

use agent_client_protocol::schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigSelectOptions,
};
use ajax_core::adapters::{parse_cursor_model_intent, ModelSelection, CURSOR_DEFAULT_SPAWN_MODEL};

/// True when Cursor advertises separate `fast` (and optionally `effort`) options.
pub fn cursor_parameterized_picker(config_options: Option<&[SessionConfigOption]>) -> bool {
    config_option_advertised(config_options, "fast")
}

fn config_option_advertised(
    config_options: Option<&[SessionConfigOption]>,
    config_id: &str,
) -> bool {
    config_options.is_some_and(|options| {
        options
            .iter()
            .any(|option| option.id.0.as_ref() == config_id)
    })
}

/// Config id Cursor uses for the semantic effort axis (`reasoning` or legacy `effort`).
fn effort_config_id(config_options: Option<&[SessionConfigOption]>) -> Option<&'static str> {
    if config_option_advertised(config_options, "reasoning") {
        Some("reasoning")
    } else if config_option_advertised(config_options, "effort") {
        Some("effort")
    } else {
        None
    }
}

fn read_effort_level(config_options: Option<&[SessionConfigOption]>) -> Option<String> {
    read_config_option_current_value(config_options, "reasoning")
        .or_else(|| read_config_option_current_value(config_options, "effort"))
        .filter(|value| !value.is_empty())
}

fn effort_option_advertised(config_options: Option<&[SessionConfigOption]>, value: &str) -> bool {
    effort_config_id(config_options)
        .is_some_and(|id| config_option_value_advertised(config_options, id, value))
}

pub fn read_config_option_current_value(
    config_options: Option<&[SessionConfigOption]>,
    config_id: &str,
) -> Option<String> {
    let option = config_options?
        .iter()
        .find(|option| option.id.0.as_ref() == config_id)?;
    let SessionConfigKind::Select(select) = &option.kind else {
        return None;
    };
    Some(select.current_value.0.to_string())
}

/// Reconstruct a bracket ACP id from split config options when parameterized.
pub fn reconstruct_applied_model(config_options: Option<&[SessionConfigOption]>) -> Option<String> {
    let config_options = config_options?;
    let model = read_config_option_current_value(Some(config_options), "model")?;
    if model.is_empty() {
        return None;
    }
    if model.contains('[') {
        return Some(model);
    }
    if !cursor_parameterized_picker(Some(config_options)) {
        return Some(model);
    }
    let mut options = Vec::new();
    if let Some(effort) = read_effort_level(Some(config_options)) {
        options.push(("effort".to_string(), effort));
    }
    if let Some(fast) = read_config_option_current_value(Some(config_options), "fast")
        .filter(|value| !value.is_empty())
    {
        options.push(("fast".to_string(), fast));
    }
    Some(parameterized_applied_id(&ModelSelection { model, options }))
}

fn format_cursor_bracket_id(base: &str, effort: Option<&str>, fast: Option<&str>) -> String {
    let mut parts = Vec::new();
    if let Some(effort) = effort.filter(|value| !value.is_empty()) {
        parts.push(format!("effort={effort}"));
    }
    if let Some(fast) = fast.filter(|value| !value.is_empty()) {
        parts.push(format!("fast={fast}"));
    }
    if parts.is_empty() {
        base.to_string()
    } else {
        format!("{base}[{}]", parts.join(","))
    }
}

/// Build split `session/set_config_option` values for a Cursor catalog pin.
pub fn resolve_parameterized_apply(
    catalog_id: &str,
    config_options: Option<&[SessionConfigOption]>,
) -> Option<ModelSelection> {
    if !cursor_parameterized_picker(config_options) {
        return None;
    }
    let desired = parse_cursor_model_intent(catalog_id)?;
    let base = desired.base.clone();
    if !config_option_value_advertised(config_options, "model", &base) {
        return None;
    }
    let mut options = Vec::new();
    if let Some(effort) = &desired.effort {
        let config_id = effort_config_id(config_options)?;
        if !config_option_value_advertised(config_options, config_id, effort) {
            return None;
        }
        options.push((config_id.to_string(), effort.clone()));
    }
    let fast = desired.fast.unwrap_or(false);
    if config_option_advertised(config_options, "fast") {
        let fast_value = if fast { "true" } else { "false" };
        if config_option_value_advertised(config_options, "fast", fast_value) {
            options.push(("fast".to_string(), fast_value.to_string()));
        } else {
            return None;
        }
    }
    Some(ModelSelection {
        model: base,
        options,
    })
}

/// True when every split apply piece is an exact advertised handshake value.
pub fn parameterized_selection_advertised(
    config_options: Option<&[SessionConfigOption]>,
    selection: &ModelSelection,
) -> bool {
    if !config_option_value_advertised(config_options, "model", &selection.model) {
        return false;
    }
    selection.options.iter().all(|(config_id, value)| {
        if config_id == "effort" || config_id == "reasoning" {
            return effort_option_advertised(config_options, value);
        }
        config_option_value_advertised(config_options, config_id, value)
    })
}

fn select_value_advertised(
    select: &agent_client_protocol::schema::v1::SessionConfigSelect,
    value: &str,
) -> bool {
    match &select.options {
        SessionConfigSelectOptions::Ungrouped(options) => options
            .iter()
            .any(|option| option.value.0.as_ref() == value),
        SessionConfigSelectOptions::Grouped(groups) => groups.iter().any(|group| {
            group
                .options
                .iter()
                .any(|option| option.value.0.as_ref() == value)
        }),
        _ => false,
    }
}

fn config_option_value_advertised(
    config_options: Option<&[SessionConfigOption]>,
    config_id: &str,
    value: &str,
) -> bool {
    let Some(config_options) = config_options else {
        return false;
    };
    let Some(option) = config_options
        .iter()
        .find(|option| option.id.0.as_ref() == config_id)
    else {
        return false;
    };
    let SessionConfigKind::Select(select) = &option.kind else {
        return false;
    };
    select_value_advertised(select, value)
}

/// Split apply for unspecified / Auto Cursor attach when the handshake starts Fast.
pub fn resolve_parameterized_unspecified_apply(
    config_options: Option<&[SessionConfigOption]>,
) -> Option<ModelSelection> {
    if !cursor_parameterized_picker(config_options) {
        return None;
    }
    let model =
        if config_option_value_advertised(config_options, "model", CURSOR_DEFAULT_SPAWN_MODEL) {
            CURSOR_DEFAULT_SPAWN_MODEL.to_string()
        } else {
            let current = read_config_option_current_value(config_options, "model")?;
            if current.is_empty() || current.contains('[') {
                return None;
            }
            current
        };
    if !config_option_value_advertised(config_options, "model", &model) {
        return None;
    }
    if !config_option_advertised(config_options, "fast")
        || !config_option_value_advertised(config_options, "fast", "false")
    {
        return None;
    }
    Some(ModelSelection {
        model,
        options: vec![("fast".to_string(), "false".to_string())],
    })
}

/// Encode the split apply as the bracket id used for snapshot matching.
pub fn parameterized_applied_id(selection: &ModelSelection) -> String {
    let fast = selection
        .options
        .iter()
        .find(|(key, _)| key == "fast")
        .map(|(_, value)| value.as_str());
    let effort = selection
        .options
        .iter()
        .find(|(key, _)| key == "effort" || key == "reasoning")
        .map(|(_, value)| value.as_str());
    format_cursor_bracket_id(&selection.model, effort, fast)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{SessionConfigOption, SessionConfigSelectOption};
    use ajax_core::adapters::parse_model_selection;

    fn parameterized_options(
        current_model: &str,
        effort: &str,
        fast: &str,
    ) -> Vec<SessionConfigOption> {
        vec![
            SessionConfigOption::select(
                "model",
                "Model",
                current_model.to_string(),
                vec![
                    SessionConfigSelectOption::new("composer-2.5", "Composer"),
                    SessionConfigSelectOption::new("grok-4.6", "Grok 4.6"),
                ],
            ),
            SessionConfigOption::select(
                "effort",
                "Effort",
                effort.to_string(),
                vec![
                    SessionConfigSelectOption::new("high", "High"),
                    SessionConfigSelectOption::new("medium", "Medium"),
                ],
            ),
            SessionConfigOption::select(
                "fast",
                "Fast",
                fast.to_string(),
                vec![
                    SessionConfigSelectOption::new("true", "Fast"),
                    SessionConfigSelectOption::new("false", "Standard"),
                ],
            ),
        ]
    }

    #[test]
    fn cursor_parameterized_picker_detects_fast_option() {
        let options = parameterized_options("grok-4.6", "high", "true");
        assert!(cursor_parameterized_picker(Some(&options)));
        assert!(!cursor_parameterized_picker(None));
        assert!(!cursor_parameterized_picker(Some(&[
            SessionConfigOption::select(
                "model",
                "Model",
                "composer-2.5",
                vec![SessionConfigSelectOption::new("composer-2.5", "Composer")],
            )
        ])));
    }

    #[test]
    fn reconstruct_applied_model_from_split_options_issue_979() {
        let options = parameterized_options("grok-4.6", "high", "false");
        assert_eq!(
            reconstruct_applied_model(Some(&options)).as_deref(),
            Some("grok-4.6[effort=high,fast=false]")
        );
        assert_ne!(
            reconstruct_applied_model(Some(&options)).as_deref(),
            Some("grok-4.6")
        );
    }

    #[test]
    fn resolve_parameterized_apply_grok_high_non_fast_issue_979() {
        let options = parameterized_options("composer-2.5", "high", "true");
        let resolved =
            resolve_parameterized_apply("cursor-grok-4.6-high", Some(&options)).expect("apply");
        assert_eq!(resolved.model, "grok-4.6");
        assert!(resolved
            .options
            .contains(&("effort".to_string(), "high".to_string())));
        assert!(resolved
            .options
            .contains(&("fast".to_string(), "false".to_string())));
        assert_eq!(
            parameterized_applied_id(&resolved),
            "grok-4.6[effort=high,fast=false]"
        );
    }

    #[test]
    fn resolve_parameterized_apply_rejects_missing_base_issue_979() {
        let options = parameterized_options("composer-2.5", "high", "true");
        assert!(resolve_parameterized_apply("gpt-5.6-sol-high", Some(&options)).is_none());
    }

    #[test]
    fn parameterized_selection_advertised_requires_exact_values_issue_979() {
        let options = parameterized_options("grok-4.6", "high", "true");
        let selection = parse_model_selection("grok-4.6|effort=high|fast=false").unwrap();
        assert!(parameterized_selection_advertised(
            Some(&options),
            &selection
        ));
        let unknown_model = parse_model_selection("gpt-5.6-sol|effort=high|fast=false").unwrap();
        assert!(!parameterized_selection_advertised(
            Some(&options),
            &unknown_model
        ));
    }

    #[test]
    fn resolve_parameterized_unspecified_apply_prefers_spawn_default_issue_979() {
        let options = parameterized_options("composer-2.5", "high", "true");
        let resolved = resolve_parameterized_unspecified_apply(Some(&options)).expect("apply");
        assert_eq!(resolved.model, "grok-4.6");
        assert_eq!(
            resolved.options,
            vec![("fast".to_string(), "false".to_string())]
        );
        assert_eq!(parameterized_applied_id(&resolved), "grok-4.6[fast=false]");
    }

    fn parameterized_options_with_reasoning(
        current_model: &str,
        reasoning: &str,
        fast: &str,
    ) -> Vec<SessionConfigOption> {
        vec![
            SessionConfigOption::select(
                "model",
                "Model",
                current_model.to_string(),
                vec![
                    SessionConfigSelectOption::new("composer-2.5", "Composer"),
                    SessionConfigSelectOption::new("gpt-5.6-sol", "GPT 5.6 Sol"),
                ],
            ),
            SessionConfigOption::select(
                "reasoning",
                "Reasoning",
                reasoning.to_string(),
                vec![
                    SessionConfigSelectOption::new("high", "High"),
                    SessionConfigSelectOption::new("medium", "Medium"),
                ],
            ),
            SessionConfigOption::select(
                "fast",
                "Fast",
                fast.to_string(),
                vec![
                    SessionConfigSelectOption::new("true", "Fast"),
                    SessionConfigSelectOption::new("false", "Standard"),
                ],
            ),
        ]
    }

    #[test]
    fn reconstruct_applied_model_reads_reasoning_config_issue_989() {
        let options = parameterized_options_with_reasoning("gpt-5.6-sol", "high", "false");
        assert_eq!(
            reconstruct_applied_model(Some(&options)).as_deref(),
            Some("gpt-5.6-sol[effort=high,fast=false]")
        );
    }

    #[test]
    fn resolve_parameterized_apply_gpt_sol_high_with_reasoning_config_issue_989() {
        let options = parameterized_options_with_reasoning("gpt-5.6-sol", "high", "false");
        let resolved =
            resolve_parameterized_apply("gpt-5.6-sol-high", Some(&options)).expect("apply");
        assert_eq!(resolved.model, "gpt-5.6-sol");
        assert!(resolved
            .options
            .contains(&("reasoning".to_string(), "high".to_string())));
        assert!(resolved
            .options
            .contains(&("fast".to_string(), "false".to_string())));
        assert_eq!(
            parameterized_applied_id(&resolved),
            "gpt-5.6-sol[effort=high,fast=false]"
        );
    }

    #[test]
    fn parameterized_selection_advertised_accepts_effort_when_reasoning_advertised_issue_989() {
        let options = parameterized_options_with_reasoning("gpt-5.6-sol", "high", "false");
        let selection = parse_model_selection("gpt-5.6-sol|effort=high|fast=false").unwrap();
        assert!(parameterized_selection_advertised(
            Some(&options),
            &selection
        ));
    }

    #[test]
    fn reconstruct_leaves_bracket_model_unchanged() {
        let options = vec![SessionConfigOption::select(
            "model",
            "Model",
            "composer-2.5[fast=true]",
            vec![SessionConfigSelectOption::new(
                "composer-2.5[fast=true]",
                "Composer Fast",
            )],
        )];
        assert_eq!(
            reconstruct_applied_model(Some(&options)).as_deref(),
            Some("composer-2.5[fast=true]")
        );
    }
}
