//! ACP session `configOptions` helpers (Agent of Empires contract).
//! Find advertised options, map operator pins, and build typed set requests.

use agent_client_protocol::schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory, SessionConfigOptionValue,
    SessionConfigSelect, SessionConfigSelectOptions, SetSessionConfigOptionRequest,
};
use ajax_core::adapters::{
    canonical_cursor_model_intent, cursor_bracket_token_from_intent, cursor_model_intents_match,
    cursor_unspecified_spawn_satisfied, parse_cursor_model_intent, parse_model_selection,
    CursorModelIntent, ModelSelection, CURSOR_DEFAULT_SPAWN_MODEL,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Wire value for one live `session/set_config_option` change.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum SessionConfigValue {
    Select(String),
    Boolean(bool),
}

/// One in-band `session/set_config_option` change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigApplyStep {
    pub config_id: String,
    pub value: SessionConfigOptionValue,
}

/// Kept for the stacked UI branch (`config_option_descriptors`); unused on PR #999 alone.
#[allow(dead_code)]
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
    // Cursor advertises `reasoning` then rejects it on set_config_option (#1010). Prefer `effort`.
    if let Some(effort) = options
        .iter()
        .find(|option| option.id.0.as_ref() == "effort")
    {
        return Some(effort);
    }
    find_option_by_category(
        options,
        SessionConfigOptionCategory::ThoughtLevel,
        &["thought_level", "reasoning"],
    )
}

fn fast_option_advertised(option: &SessionConfigOption) -> bool {
    match &option.kind {
        SessionConfigKind::Boolean(_) => true,
        SessionConfigKind::Select(select) => {
            select_value_advertised(select, "true") && select_value_advertised(select, "false")
        }
        _ => false,
    }
}

pub fn model_config_boolean_option(
    options: &[SessionConfigOption],
) -> Option<&SessionConfigOption> {
    options
        .iter()
        .find(|option| {
            option.category.as_ref() == Some(&SessionConfigOptionCategory::ModelConfig)
                && fast_option_advertised(option)
        })
        .or_else(|| {
            options
                .iter()
                .find(|option| option.id.0.as_ref() == "fast" && fast_option_advertised(option))
        })
}

pub fn read_fast_current_value(option: &SessionConfigOption) -> Option<bool> {
    if let Some(value) = read_boolean_current_value(option) {
        return Some(value);
    }
    let SessionConfigKind::Select(select) = &option.kind else {
        return None;
    };
    match select.current_value.0.as_ref() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn fast_apply_value(option: &SessionConfigOption, want: bool) -> SessionConfigOptionValue {
    match &option.kind {
        SessionConfigKind::Boolean(_) => SessionConfigOptionValue::boolean(want),
        SessionConfigKind::Select(_) => {
            SessionConfigOptionValue::value_id(if want { "true" } else { "false" })
        }
        _ => SessionConfigOptionValue::boolean(want),
    }
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

/// Convert the typed browser value without widening the accepted JSON shape.
pub fn wire_value_to_session_value(value: SessionConfigValue) -> SessionConfigOptionValue {
    match value {
        SessionConfigValue::Select(value) => SessionConfigOptionValue::value_id(value),
        SessionConfigValue::Boolean(value) => SessionConfigOptionValue::boolean(value),
    }
}

/// Validate the exact advertised id, kind, and choice before ACP I/O.
pub fn validate_config_change(
    options: &[SessionConfigOption],
    config_id: &str,
    value: &SessionConfigOptionValue,
) -> Result<(), String> {
    let option = options
        .iter()
        .find(|option| option.id.0.as_ref() == config_id)
        .ok_or_else(|| format!("config option {config_id} is not advertised on this session"))?;
    match (&option.kind, value) {
        (SessionConfigKind::Select(select), SessionConfigOptionValue::ValueId { value })
            if select_value_advertised(select, value.0.as_ref()) =>
        {
            Ok(())
        }
        (SessionConfigKind::Select(_), SessionConfigOptionValue::ValueId { value }) => {
            Err(format!(
                "config option {config_id}={} is not advertised",
                value.0.as_ref()
            ))
        }
        (SessionConfigKind::Boolean(_), SessionConfigOptionValue::Boolean { .. }) => Ok(()),
        (SessionConfigKind::Select(_), _) => {
            Err(format!("config option {config_id} requires a string value"))
        }
        (SessionConfigKind::Boolean(_), _) => Err(format!(
            "config option {config_id} requires a boolean value"
        )),
        _ => Err(format!("config option {config_id} cannot be changed")),
    }
}

fn option_is_model_category(options: &[SessionConfigOption], config_id: &str) -> bool {
    options.iter().any(|option| {
        option.id.0.as_ref() == config_id
            && (option.category.as_ref() == Some(&SessionConfigOptionCategory::Model)
                || config_id == "model")
    })
}

/// True when a successful apply of `config_id` should persist pipe storage ([#1014]).
pub fn option_triggers_model_persist(options: &[SessionConfigOption], config_id: &str) -> bool {
    option_is_model_category(options, config_id)
        || thought_level_option(options).is_some_and(|option| option.id.0.as_ref() == config_id)
        || model_config_boolean_option(options)
            .is_some_and(|option| option.id.0.as_ref() == config_id)
}

/// Storage pipe for task `session_model` after a successful model-option apply.
pub fn applied_model_id_for_persist(options: &[SessionConfigOption]) -> Result<String, String> {
    let model_id = read_model_applied(Some(options))
        .ok_or_else(|| "confirmed config options omitted the model value".to_string())?;
    if is_unspecified_model(Some(&model_id)) {
        return Ok("auto".to_string());
    }
    let selection = model_selection_from_advertised_options(options)
        .ok_or_else(|| "confirmed config options omitted the model value".to_string())?;
    let encoded = selection.encode();
    if parse_model_selection(&encoded).as_ref() != Some(&selection) {
        return Err("confirmed model options cannot be encoded for restart".to_string());
    }
    Ok(encoded)
}

fn ajax_pipe_model_base(intent: &CursorModelIntent) -> String {
    if intent.thinking == Some(true) && !intent.base.ends_with("-thinking") {
        format!("{}-thinking", intent.base)
    } else {
        intent.base.clone()
    }
}

fn model_selection_from_advertised_options(
    options: &[SessionConfigOption],
) -> Option<ModelSelection> {
    let model = read_model_applied(Some(options))?;
    let parsed = parse_cursor_model_intent(&model);
    // Exploded ACP ids collapse to Ajax pipe-form so restart storage stays canonical.
    if let Some(intent) = parsed.as_ref().filter(|intent| intent.effort.is_some()) {
        let mut extras = Vec::new();
        if let Some(effort) = &intent.effort {
            extras.push(("effort".to_string(), effort.clone()));
        }
        extras.push((
            "fast".to_string(),
            if intent.fast.unwrap_or(false) {
                "true"
            } else {
                "false"
            }
            .to_string(),
        ));
        return Some(ModelSelection {
            model: ajax_pipe_model_base(intent),
            options: extras,
        });
    }
    let model = parsed
        .as_ref()
        .map(|intent| intent.base.clone())
        .unwrap_or(model);
    let mut extras = Vec::new();
    if let Some(thought) = thought_level_option(options) {
        if let Some(level) = read_select_current_value(thought) {
            extras.push((thought.id.0.to_string(), level));
        }
    }
    if let Some(fast) = model_config_boolean_option(options) {
        if let Some(on) = read_fast_current_value(fast) {
            extras.push((
                fast.id.0.to_string(),
                if on { "true" } else { "false" }.to_string(),
            ));
        }
    }
    Some(ModelSelection {
        model,
        options: extras,
    })
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

pub(crate) fn step_matches_current(
    options: &[SessionConfigOption],
    step: &ConfigApplyStep,
) -> bool {
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
        (SessionConfigKind::Select(select), SessionConfigOptionValue::Boolean { value }) => {
            select.current_value.0.as_ref() == if *value { "true" } else { "false" }
        }
        (SessionConfigKind::Boolean(boolean), SessionConfigOptionValue::ValueId { value }) => {
            boolean.current_value == (value.0.as_ref() == "true")
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
            return read_fast_current_value(fast) == Some(false);
        }
        return true;
    }
    if cursor_canonical_pin(desired) {
        if let Some(wanted) = parse_cursor_model_intent(desired) {
            if split_axis_contract_satisfies(options, &wanted) {
                return map_pin_to_apply_steps(options, desired, model_pins_at_spawn)
                    .ok()
                    .is_some_and(|steps| {
                        steps.iter().all(|step| step_matches_current(options, step))
                    });
            }
            if let Some(applied) = applied_cursor_intent(options) {
                if read_model_applied(Some(options))
                    .is_some_and(|applied_id| model_select_value_advertised(options, &applied_id))
                {
                    if cursor_model_intents_match(&wanted, &applied) {
                        return true;
                    }
                    if partial_effort_pin_satisfied(options, &wanted, &applied) {
                        return true;
                    }
                }
            }
        }
    } else if let Some(wanted) = parse_cursor_model_intent(desired.trim()) {
        if let Some(applied_id) = read_model_applied(Some(options)) {
            if model_select_value_advertised(options, &applied_id) {
                if let Some(applied) = applied_cursor_intent(options) {
                    if cursor_model_intents_match(&wanted, &applied) {
                        return true;
                    }
                    if partial_effort_pin_satisfied(options, &wanted, &applied) {
                        return true;
                    }
                }
            }
        }
    }
    map_pin_to_apply_steps(options, desired, model_pins_at_spawn)
        .ok()
        .is_some_and(|steps| steps.iter().all(|step| step_matches_current(options, step)))
}

fn cursor_canonical_pin(raw: &str) -> bool {
    // ponytail: bare catalog ids parse as model-only selections; require pipe-form
    // so intent matching stays on Ajax canonical pins, not spawn argv tokens.
    raw.contains('|')
        && parse_model_selection(raw).is_some_and(|selection| {
            !selection.options.is_empty()
                && selection
                    .options
                    .iter()
                    .all(|(key, _)| key == "effort" || key == "fast")
        })
}

fn applied_cursor_intent(options: &[SessionConfigOption]) -> Option<CursorModelIntent> {
    let model = read_model_applied(Some(options))?;
    let mut intent = parse_cursor_model_intent(&model)?;
    if intent.effort.is_some() {
        return Some(intent);
    }
    if let Some(thought) = thought_level_option(options) {
        if let Some(level) = read_select_current_value(thought) {
            intent.effort = Some(level);
        }
    }
    if let Some(fast) = model_config_boolean_option(options) {
        if let Some(on) = read_fast_current_value(fast) {
            intent.fast = Some(on);
        }
    }
    Some(intent)
}

fn split_axis_model_base(options: &[SessionConfigOption], intent: &CursorModelIntent) -> String {
    if intent.base.ends_with("-thinking") {
        if model_option(options).is_some_and(|model| option_value_advertised(model, &intent.base)) {
            return intent.base.clone();
        }
    }
    let canonical = canonical_cursor_model_intent(intent);
    if canonical.thinking == Some(true) {
        canonical.base
    } else {
        intent.base.clone()
    }
}

fn model_select_value_advertised(options: &[SessionConfigOption], value: &str) -> bool {
    model_option(options).is_some_and(|model| option_value_advertised(model, value))
}

fn effort_axis_advertised(options: &[SessionConfigOption], effort: &str) -> bool {
    thought_level_option(options).is_some_and(|thought| option_value_advertised(thought, effort))
}

fn effort_tier_advertised(options: &[SessionConfigOption], intent: &CursorModelIntent) -> bool {
    let Some(effort) = &intent.effort else {
        return false;
    };
    if effort_axis_advertised(options, effort) {
        return true;
    }
    let wanted = canonical_cursor_model_intent(intent);
    model_option(options).is_some_and(|model| {
        let SessionConfigKind::Select(select) = &model.kind else {
            return false;
        };
        select_option_ids(select).into_iter().any(|id| {
            parse_cursor_model_intent(&id).is_some_and(|parsed| {
                let parsed = canonical_cursor_model_intent(&parsed);
                parsed.base == wanted.base
                    && parsed.thinking.unwrap_or(false) == wanted.thinking.unwrap_or(false)
                    && parsed.effort.as_deref() == Some(effort.as_str())
            })
        })
    })
}

fn partial_effort_apply_ok(
    options: &[SessionConfigOption],
    wanted: &CursorModelIntent,
    applied: &CursorModelIntent,
) -> bool {
    let wanted = canonical_cursor_model_intent(wanted);
    let applied = canonical_cursor_model_intent(applied);
    if wanted.base != applied.base {
        return false;
    }
    if wanted.thinking.unwrap_or(false) != applied.thinking.unwrap_or(false) {
        return false;
    }
    match &wanted.effort {
        Some(_) if effort_tier_advertised(options, &wanted) => false,
        Some(_) => true,
        None => false,
    }
}

fn partial_effort_pin_satisfied(
    options: &[SessionConfigOption],
    wanted: &CursorModelIntent,
    applied: &CursorModelIntent,
) -> bool {
    if !partial_effort_apply_ok(options, wanted, applied) {
        return false;
    }
    let wanted = canonical_cursor_model_intent(wanted);
    let applied = canonical_cursor_model_intent(applied);
    wanted.fast.unwrap_or(false) == applied.fast.unwrap_or(false)
}

fn split_axis_contract_satisfies(
    options: &[SessionConfigOption],
    intent: &CursorModelIntent,
) -> bool {
    let Some(model) = model_option(options) else {
        return false;
    };
    let model_base = split_axis_model_base(options, intent);
    if !option_value_advertised(model, &model_base) {
        return false;
    }
    if let Some(effort) = &intent.effort {
        if !effort_axis_advertised(options, effort) {
            return false;
        }
    }
    if intent.fast == Some(true) && model_config_boolean_option(options).is_none() {
        return false;
    }
    true
}

/// True when the operator did not pin a specific harness model id.
pub fn is_unspecified_model(raw: Option<&str>) -> bool {
    ajax_core::adapters::is_unspecified_acp_model(raw)
        || matches!(raw.map(str::trim), Some("default" | "default[]"))
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
        if !cursor_canonical_pin(raw) {
            return map_selection_to_steps(options, &selection);
        }
    }
    if let Some(intent) = parse_cursor_model_intent(raw) {
        if split_axis_contract_satisfies(options, &intent) {
            return map_intent_to_split_steps(options, &intent);
        }
        if let Ok(steps) = map_intent_to_model_select_steps(options, &intent, model_pins_at_spawn) {
            return Ok(steps);
        }
        if let Some(applied) = applied_cursor_intent(options) {
            if partial_effort_apply_ok(options, &intent, &applied) {
                return map_partial_effort_steps(options, &intent);
            }
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
        if read_fast_current_value(fast) != Some(false) {
            steps.push(ConfigApplyStep {
                config_id: fast.id.0.to_string(),
                value: fast_apply_value(fast, false),
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
                        thinking: None,
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
    let model_base = split_axis_model_base(options, intent);
    if !option_value_advertised(model, &model_base) {
        return Err(format!(
            "harness did not advertise model value {model_base}"
        ));
    }
    let mut steps = vec![ConfigApplyStep {
        config_id: model.id.0.to_string(),
        value: SessionConfigOptionValue::value_id(model_base),
    }];
    if let Some(effort) = &intent.effort {
        let thought = thought_level_option(options)
            .ok_or_else(|| format!("harness did not advertise effort value {effort}"))?;
        if !option_value_advertised(thought, effort) {
            return Err(format!("harness did not advertise effort value {effort}"));
        }
        steps.push(ConfigApplyStep {
            config_id: thought.id.0.to_string(),
            value: SessionConfigOptionValue::value_id(effort.clone()),
        });
    }
    if let Some(fast) = model_config_boolean_option(options) {
        let want = intent.fast.unwrap_or(false);
        if read_fast_current_value(fast) != Some(want) {
            steps.push(ConfigApplyStep {
                config_id: fast.id.0.to_string(),
                value: fast_apply_value(fast, want),
            });
        }
    } else if intent.fast == Some(true) {
        return Err("harness did not advertise a Fast config option".to_string());
    }
    Ok(steps)
}

fn map_partial_effort_steps(
    options: &[SessionConfigOption],
    intent: &CursorModelIntent,
) -> Result<Vec<ConfigApplyStep>, String> {
    let mut steps = Vec::new();
    if let Some(fast) = model_config_boolean_option(options) {
        let want = intent.fast.unwrap_or(false);
        if read_fast_current_value(fast) != Some(want) {
            steps.push(ConfigApplyStep {
                config_id: fast.id.0.to_string(),
                value: fast_apply_value(fast, want),
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
        && intent.thinking != Some(true)
    {
        return Some(intent.base.clone());
    }
    let bracket = cursor_bracket_token_from_intent(intent);
    if ids.iter().any(|id| id == &bracket) {
        return Some(bracket);
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
    selection: &ModelSelection,
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
