//! ACP session `configOptions` helpers (Agent of Empires contract).
//!
//! Advertised options are the live configuration surface: find by category,
//! read select/boolean current values, map operator pins, and build typed
//! `session/set_config_option` requests.

use agent_client_protocol::schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory, SessionConfigOptionValue,
    SessionConfigSelect, SessionConfigSelectOptions, SetSessionConfigOptionRequest,
};
use ajax_core::adapters::{
    cursor_model_intents_match, cursor_unspecified_spawn_satisfied, parse_cursor_model_intent,
    parse_model_selection, CursorModelIntent, CURSOR_DEFAULT_SPAWN_MODEL,
};
use serde_json::Value;

/// One in-band `session/set_config_option` change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigApplyStep {
    pub config_id: String,
    pub value: SessionConfigOptionValue,
}

pub fn category_name(category: &SessionConfigOptionCategory) -> &str {
    match category {
        SessionConfigOptionCategory::Mode => "mode",
        SessionConfigOptionCategory::Model => "model",
        SessionConfigOptionCategory::ModelConfig => "model_config",
        SessionConfigOptionCategory::ThoughtLevel => "thought_level",
        SessionConfigOptionCategory::Other(name) => name.as_str(),
        _ => "unknown",
    }
}

/// Find the first option matching `category`, else the first matching `fallback_ids`.
pub fn find_option_by_category<'a>(
    options: &'a [SessionConfigOption],
    category: SessionConfigOptionCategory,
    fallback_ids: &[&str],
) -> Option<&'a SessionConfigOption> {
    options
        .iter()
        .find(|option| option.category.as_ref() == Some(&category))
        .or_else(|| {
            fallback_ids
                .iter()
                .find_map(|id| options.iter().find(|option| option.id.0.as_ref() == *id))
        })
}

pub fn model_option(options: &[SessionConfigOption]) -> Option<&SessionConfigOption> {
    find_option_by_category(options, SessionConfigOptionCategory::Model, &["model"])
}

pub fn thought_level_option(options: &[SessionConfigOption]) -> Option<&SessionConfigOption> {
    find_option_by_category(
        options,
        SessionConfigOptionCategory::ThoughtLevel,
        &["reasoning", "effort"],
    )
}

pub fn model_config_boolean_option(
    options: &[SessionConfigOption],
) -> Option<&SessionConfigOption> {
    options
        .iter()
        .find(|option| {
            option.category.as_ref() == Some(&SessionConfigOptionCategory::ModelConfig)
                && matches!(option.kind, SessionConfigKind::Boolean(_))
        })
        .or_else(|| {
            options.iter().find(|option| {
                option.id.0.as_ref() == "fast"
                    && matches!(option.kind, SessionConfigKind::Boolean(_))
            })
        })
}

pub fn mode_option(options: &[SessionConfigOption]) -> Option<&SessionConfigOption> {
    find_option_by_category(options, SessionConfigOptionCategory::Mode, &["mode"])
}

pub fn read_select_current_value(option: &SessionConfigOption) -> Option<String> {
    let SessionConfigKind::Select(select) = &option.kind else {
        return None;
    };
    let value = select.current_value.0.as_ref();
    (!value.is_empty()).then(|| value.to_string())
}

pub fn read_boolean_current_value(option: &SessionConfigOption) -> Option<bool> {
    let SessionConfigKind::Boolean(boolean) = &option.kind else {
        return None;
    };
    Some(boolean.current_value)
}

/// Applied model id: the model option's advertised `currentValue` only.
pub fn read_model_applied(options: Option<&[SessionConfigOption]>) -> Option<String> {
    read_select_current_value(model_option(options?)?)
}

pub fn select_value_advertised(select: &SessionConfigSelect, value: &str) -> bool {
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

pub fn option_value_advertised(option: &SessionConfigOption, value: &str) -> bool {
    match &option.kind {
        SessionConfigKind::Select(select) => select_value_advertised(select, value),
        SessionConfigKind::Boolean(_) => value == "true" || value == "false",
        _ => false,
    }
}

pub fn option_boolean_advertised(option: &SessionConfigOption) -> bool {
    matches!(option.kind, SessionConfigKind::Boolean(_))
}

pub fn model_config_advertised(options: Option<&[SessionConfigOption]>) -> bool {
    options.is_some_and(|options| model_option(options).is_some())
}

pub fn build_set_config_request(
    session_id: &str,
    config_id: &str,
    value: SessionConfigOptionValue,
) -> SetSessionConfigOptionRequest {
    SetSessionConfigOptionRequest::new(session_id.to_string(), config_id.to_string(), value)
}

pub fn replace_config_options(
    stored: &mut Option<Vec<SessionConfigOption>>,
    next: Vec<SessionConfigOption>,
) {
    *stored = Some(next);
}

pub fn sync_session_result_config_options(
    session_result: &mut Value,
    config_options: &[SessionConfigOption],
) {
    if let Ok(json) = serde_json::to_value(config_options) {
        if let Value::Object(ref mut map) = session_result {
            map.insert("configOptions".to_string(), json);
        }
    }
    if let Some(model) = read_model_applied(Some(config_options)) {
        if let Value::Object(ref mut map) = session_result {
            if let Some(models) = map.get_mut("models").and_then(Value::as_object_mut) {
                models.insert("currentModelId".to_string(), Value::String(model));
            }
        }
    }
}

fn step_matches_current(options: &[SessionConfigOption], step: &ConfigApplyStep) -> bool {
    let Some(option) = options
        .iter()
        .find(|option| option.id.0.as_ref() == step.config_id)
    else {
        return false;
    };
    match (&option.kind, &step.value) {
        (SessionConfigKind::Select(select), SessionConfigOptionValue::ValueId { value }) => {
            select.current_value.0.as_ref() == value.0.as_ref()
        }
        (SessionConfigKind::Boolean(boolean), SessionConfigOptionValue::Boolean { value }) => {
            boolean.current_value == *value
        }
        _ => false,
    }
}

/// True when every mapped option's `currentValue` matches the operator pin.
pub fn pin_satisfied(
    options: Option<&[SessionConfigOption]>,
    desired: &str,
    model_pins_at_spawn: bool,
) -> bool {
    let Some(options) = options else {
        return false;
    };
    if is_unspecified_model(Some(desired)) {
        if !model_pins_at_spawn {
            return true;
        }
        let Some(applied) = read_model_applied(Some(options)) else {
            return false;
        };
        if !cursor_unspecified_spawn_satisfied(&applied) {
            return false;
        }
        if let Some(fast) = model_config_boolean_option(options) {
            return read_boolean_current_value(fast) == Some(false);
        }
        return true;
    }
    map_pin_to_apply_steps(options, desired, model_pins_at_spawn)
        .ok()
        .is_some_and(|steps| steps.iter().all(|step| step_matches_current(options, step)))
}

/// True when the operator did not pin a specific harness model id.
pub fn is_unspecified_model(raw: Option<&str>) -> bool {
    matches!(raw.map(str::trim), None | Some("") | Some("auto"))
}

pub fn map_pin_to_apply_steps(
    options: &[SessionConfigOption],
    desired: &str,
    model_pins_at_spawn: bool,
) -> Result<Vec<ConfigApplyStep>, String> {
    if is_unspecified_model(Some(desired)) {
        return map_unspecified_apply_steps(options, model_pins_at_spawn);
    }
    let raw = desired.trim();
    if raw.contains('|') {
        let selection = parse_model_selection(raw).ok_or_else(|| {
            format!(
                "session model {raw:?} is invalid — model id must not contain whitespace or exceed 128 chars"
            )
        })?;
        return map_selection_to_steps(options, &selection);
    }
    if let Some(intent) = parse_cursor_model_intent(raw) {
        if model_config_boolean_option(options).is_some() || thought_level_option(options).is_some()
        {
            return map_intent_to_split_steps(options, &intent);
        }
        return map_intent_to_model_select_steps(options, &intent, model_pins_at_spawn);
    }
    let selection = parse_model_selection(raw).ok_or_else(|| {
        format!(
            "session model {raw:?} is invalid — model id must not contain whitespace or exceed 128 chars"
        )
    })?;
    map_selection_to_steps(options, &selection)
}

fn map_unspecified_apply_steps(
    options: &[SessionConfigOption],
    model_pins_at_spawn: bool,
) -> Result<Vec<ConfigApplyStep>, String> {
    if !model_pins_at_spawn {
        return Ok(Vec::new());
    }
    let mut steps = Vec::new();
    if let Some(fast) = model_config_boolean_option(options) {
        if read_boolean_current_value(fast) != Some(false) {
            steps.push(ConfigApplyStep {
                config_id: fast.id.0.to_string(),
                value: SessionConfigOptionValue::boolean(false),
            });
        }
    }
    if let Some(model) = model_option(options) {
        let current = read_select_current_value(model).unwrap_or_default();
        if !cursor_unspecified_spawn_satisfied(&current) {
            if select_value_advertised(
                match &model.kind {
                    SessionConfigKind::Select(select) => select,
                    _ => return Ok(steps),
                },
                CURSOR_DEFAULT_SPAWN_MODEL,
            ) {
                steps.push(ConfigApplyStep {
                    config_id: model.id.0.to_string(),
                    value: SessionConfigOptionValue::value_id(CURSOR_DEFAULT_SPAWN_MODEL),
                });
            } else if let SessionConfigKind::Select(select) = &model.kind {
                let default_intent = parse_cursor_model_intent(CURSOR_DEFAULT_SPAWN_MODEL)
                    .unwrap_or(CursorModelIntent {
                        base: CURSOR_DEFAULT_SPAWN_MODEL.to_string(),
                        effort: None,
                        fast: Some(false),
                    });
                if let Some(id) = find_advertised_model_value(select, &default_intent, true) {
                    steps.push(ConfigApplyStep {
                        config_id: model.id.0.to_string(),
                        value: SessionConfigOptionValue::value_id(id),
                    });
                }
            }
        }
    }
    Ok(steps)
}

fn map_intent_to_split_steps(
    options: &[SessionConfigOption],
    intent: &CursorModelIntent,
) -> Result<Vec<ConfigApplyStep>, String> {
    let model = model_option(options)
        .ok_or_else(|| "harness did not advertise a model option".to_string())?;
    if !option_value_advertised(model, &intent.base) {
        return Err(format!(
            "harness did not advertise model value {}",
            intent.base
        ));
    }
    let mut steps = vec![ConfigApplyStep {
        config_id: model.id.0.to_string(),
        value: SessionConfigOptionValue::value_id(intent.base.clone()),
    }];
    if let Some(effort) = &intent.effort {
        if let Some(thought) = thought_level_option(options) {
            if !option_value_advertised(thought, effort) {
                return Err(format!("harness did not advertise thought level {effort}"));
            }
            steps.push(ConfigApplyStep {
                config_id: thought.id.0.to_string(),
                value: SessionConfigOptionValue::value_id(effort.clone()),
            });
        }
    }
    if let Some(fast) = model_config_boolean_option(options) {
        let want = intent.fast.unwrap_or(false);
        if read_boolean_current_value(fast) != Some(want) {
            steps.push(ConfigApplyStep {
                config_id: fast.id.0.to_string(),
                value: SessionConfigOptionValue::boolean(want),
            });
        }
    } else if intent.fast == Some(true) {
        return Err("harness did not advertise a Fast config option".to_string());
    }
    Ok(steps)
}

fn map_intent_to_model_select_steps(
    options: &[SessionConfigOption],
    intent: &CursorModelIntent,
    model_pins_at_spawn: bool,
) -> Result<Vec<ConfigApplyStep>, String> {
    let model = model_option(options)
        .ok_or_else(|| "harness did not advertise a model option".to_string())?;
    let SessionConfigKind::Select(select) = &model.kind else {
        return Err("model config option is not a select".to_string());
    };
    let advertised_id = find_advertised_model_value(select, intent, model_pins_at_spawn)
        .ok_or_else(|| "harness did not advertise a matching model value".to_string())?;
    Ok(vec![ConfigApplyStep {
        config_id: model.id.0.to_string(),
        value: SessionConfigOptionValue::value_id(advertised_id),
    }])
}

fn find_advertised_model_value(
    select: &SessionConfigSelect,
    intent: &CursorModelIntent,
    model_pins_at_spawn: bool,
) -> Option<String> {
    let ids = select_option_ids(select);
    if ids.iter().any(|id| id == &intent.base)
        && intent.effort.is_none()
        && intent.fast != Some(true)
    {
        return Some(intent.base.clone());
    }
    ids.into_iter().find(|id| {
        if model_pins_at_spawn {
            parse_cursor_model_intent(id)
                .is_some_and(|applied| cursor_model_intents_match(intent, &applied))
        } else {
            id == &intent.base
        }
    })
}

fn select_option_ids(select: &SessionConfigSelect) -> Vec<String> {
    match &select.options {
        SessionConfigSelectOptions::Ungrouped(options) => options
            .iter()
            .map(|option| option.value.0.to_string())
            .collect(),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| group.options.iter())
            .map(|option| option.value.0.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

fn map_selection_to_steps(
    options: &[SessionConfigOption],
    selection: &ajax_core::adapters::ModelSelection,
) -> Result<Vec<ConfigApplyStep>, String> {
    let model = model_option(options)
        .ok_or_else(|| "harness did not advertise a model option".to_string())?;
    if !option_value_advertised(model, &selection.model) {
        return Err(format!(
            "harness did not advertise model value {}",
            selection.model
        ));
    }
    let mut steps = vec![ConfigApplyStep {
        config_id: model.id.0.to_string(),
        value: SessionConfigOptionValue::value_id(selection.model.clone()),
    }];
    for (key, value) in &selection.options {
        let option = options
            .iter()
            .find(|option| option.id.0.as_ref() == key.as_str())
            .ok_or_else(|| format!("harness did not advertise config option {key}"))?;
        let apply_value = if option_boolean_advertised(option) {
            let enabled = value == "true";
            SessionConfigOptionValue::boolean(enabled)
        } else if option_value_advertised(option, value) {
            SessionConfigOptionValue::value_id(value.clone())
        } else {
            return Err(format!("harness did not advertise value {value} for {key}"));
        };
        steps.push(ConfigApplyStep {
            config_id: key.clone(),
            value: apply_value,
        });
    }
    Ok(steps)
}

pub fn apply_steps_needing_send(
    options: &[SessionConfigOption],
    steps: &[ConfigApplyStep],
) -> Vec<ConfigApplyStep> {
    steps
        .iter()
        .filter(|step| !step_matches_current(options, step))
        .cloned()
        .collect()
}
