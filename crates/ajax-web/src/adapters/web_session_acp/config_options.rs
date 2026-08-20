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
    cursor_model_intents_match_with_raw, cursor_unspecified_spawn_satisfied,
    encode_cursor_intent_to_storage_pipe, parse_cursor_model_intent, parse_model_selection,
    CursorModelIntent, ModelSelection, CURSOR_DEFAULT_SPAWN_MODEL,
};
use serde_json::Value;

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
    // Cursor advertises `reasoning` (thought_level) then rejects
    // session/set_config_option reasoning as unknown (#1010). Prefer `effort`.
    let found = find_option_by_category(
        options,
        SessionConfigOptionCategory::ThoughtLevel,
        &["effort", "thought_level", "reasoning"],
    )?;
    if found.id.0.as_ref() == "reasoning" {
        return options
            .iter()
            .find(|option| option.id.0.as_ref() == "effort");
    }
    Some(found)
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

/// Parse a browser WebSocket config-option value into an ACP wire value.
pub fn wire_value_to_session_value(raw: &Value) -> Result<SessionConfigOptionValue, String> {
    if let Some(value) = raw.as_str() {
        return Ok(SessionConfigOptionValue::value_id(value.to_string()));
    }
    if let Some(value) = raw.as_bool() {
        return Ok(SessionConfigOptionValue::boolean(value));
    }
    if let Some(map) = raw.as_object() {
        if map.get("type").and_then(Value::as_str) == Some("boolean") {
            if let Some(value) = map.get("value").and_then(Value::as_bool) {
                return Ok(SessionConfigOptionValue::boolean(value));
            }
        }
        if let Some(value) = map.get("value").and_then(Value::as_str) {
            return Ok(SessionConfigOptionValue::value_id(value.to_string()));
        }
    }
    Err("invalid config option value".to_string())
}

/// True when `config_id` names the advertised model selector.
pub fn option_is_model_category(options: &[SessionConfigOption], config_id: &str) -> bool {
    options.iter().any(|option| {
        option.id.0.as_ref() == config_id
            && (option.category.as_ref() == Some(&SessionConfigOptionCategory::Model)
                || config_id == "model")
    })
}

/// True when a successful apply of `config_id` should persist pipe storage ([#1014]).
pub fn option_triggers_model_persist(options: &[SessionConfigOption], config_id: &str) -> bool {
    if option_is_model_category(options, config_id) {
        return true;
    }
    options
        .iter()
        .any(|option| option.id.0.as_ref() == config_id && fast_option_advertised(option))
}

/// Normalize a compatibility `set_model` client id to Ajax pipe storage when parseable.
///
/// Catalog ids and advertised bracket tokens become pipe; non-Cursor / unparseable ids
/// pass through unchanged. Auto/unspecified stays as-is.
pub fn session_model_for_task_persist(raw: &str) -> String {
    let trimmed = raw.trim();
    if is_unspecified_model(Some(trimmed)) {
        return trimmed.to_string();
    }
    if looks_like_cursor_model_identity(trimmed) {
        if let Some(intent) = parse_cursor_model_intent(trimmed) {
            return encode_cursor_intent_to_storage_pipe(&intent);
        }
    }
    trimmed.to_string()
}

fn looks_like_cursor_model_identity(raw: &str) -> bool {
    if raw.contains('|') || raw.contains('[') {
        return true;
    }
    if raw.starts_with("cursor-grok-") || raw.contains("-thinking-") || raw.ends_with("-fast") {
        return true;
    }
    const EFFORTS: &[&str] = &["xhigh", "high", "medium", "low", "none", "max"];
    if EFFORTS
        .iter()
        .any(|effort| raw.ends_with(&format!("-{effort}")))
    {
        return true;
    }
    const PREFIXES: &[&str] = &["composer-", "gpt-", "claude-", "grok-", "gemini-"];
    PREFIXES.iter().any(|prefix| raw.starts_with(prefix))
}

/// Storage pipe for task `session_model` after a successful model-option apply.
///
/// Decodes advertised `currentValue` plus sibling effort/Fast options into structured
/// fields, then encodes Ajax pipe storage — never catalog ids or bracket tokens.
pub fn applied_model_id_for_persist(options: Option<&[SessionConfigOption]>) -> Option<String> {
    let options = options?;
    let model_id = read_model_applied(Some(options))?;
    if is_unspecified_model(Some(&model_id)) {
        return None;
    }
    if let Some(mut intent) = parse_cursor_model_intent(&model_id) {
        if let Some(level) = read_advertised_effort_level(options) {
            intent.effort = Some(level);
        }
        if let Some(fast) = model_config_boolean_option(options) {
            intent.fast = Some(read_fast_current_value(fast).unwrap_or(false));
        }
        return Some(encode_cursor_intent_to_storage_pipe(&intent));
    }
    model_selection_from_advertised_options(options).map(|selection| selection.encode())
}

fn read_advertised_effort_level(options: &[SessionConfigOption]) -> Option<String> {
    if let Some(thought) = thought_level_option(options) {
        return read_select_current_value(thought);
    }
    for id in ["effort", "reasoning", "thought_level"] {
        if let Some(option) = options.iter().find(|option| option.id.0.as_ref() == id) {
            if let Some(level) = read_select_current_value(option) {
                return Some(level);
            }
        }
    }
    None
}

fn effort_key_for_persist(options: &[SessionConfigOption]) -> &'static str {
    if options
        .iter()
        .any(|option| option.id.0.as_ref() == "reasoning")
    {
        "reasoning"
    } else {
        "effort"
    }
}
fn model_selection_from_advertised_options(
    options: &[SessionConfigOption],
) -> Option<ModelSelection> {
    let model = read_model_applied(Some(options))?;
    let mut extras = Vec::new();
    if let Some(level) = read_advertised_effort_level(options) {
        extras.push((effort_key_for_persist(options).to_string(), level));
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
        if cursor_auto_value(options).is_some() {
            return read_model_applied(Some(options)).as_deref() == Some("default");
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
    if applied_model_satisfies_pin(options, desired) {
        return true;
    }
    map_pin_to_apply_steps(options, desired, model_pins_at_spawn)
        .ok()
        .is_some_and(|steps| steps.iter().all(|step| step_matches_current(options, step)))
}

/// True when the operator did not pin a specific harness model id.
pub fn is_unspecified_model(raw: Option<&str>) -> bool {
    ajax_core::adapters::is_unspecified_acp_model(raw)
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
            // Issue #1010: when the model select advertises the full exploded id
            // (e.g. gpt-5.2-high), apply that value directly — effort lives in the
            // model value id, not a sibling thought_level/reasoning option.
            if let Some(model) = model_option(options) {
                if let SessionConfigKind::Select(select) = &model.kind {
                    if let Some(advertised_id) =
                        find_advertised_model_value(select, &intent, model_pins_at_spawn)
                    {
                        if advertised_id != intent.base {
                            return map_exploded_model_value_steps(
                                options,
                                &intent,
                                &advertised_id,
                            );
                        }
                    }
                }
            }
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

fn cursor_auto_value(options: &[SessionConfigOption]) -> Option<&'static str> {
    let model = model_option(options)?;
    option_value_advertised(model, "default").then_some("default")
}

fn map_unspecified_apply_steps(
    options: &[SessionConfigOption],
    model_pins_at_spawn: bool,
) -> Result<Vec<ConfigApplyStep>, String> {
    if !model_pins_at_spawn {
        return Ok(Vec::new());
    }
    if let Some(auto_id) = cursor_auto_value(options) {
        let model = model_option(options)
            .ok_or_else(|| "harness did not advertise a model option".to_string())?;
        if read_select_current_value(model).as_deref() == Some(auto_id) {
            return Ok(Vec::new());
        }
        return Ok(vec![ConfigApplyStep {
            config_id: model.id.0.to_string(),
            value: SessionConfigOptionValue::value_id(auto_id.to_string()),
        }]);
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
                        thinking: Some(false),
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

fn map_exploded_model_value_steps(
    options: &[SessionConfigOption],
    intent: &CursorModelIntent,
    advertised_id: &str,
) -> Result<Vec<ConfigApplyStep>, String> {
    let model = model_option(options)
        .ok_or_else(|| "harness did not advertise a model option".to_string())?;
    let mut steps = vec![ConfigApplyStep {
        config_id: model.id.0.to_string(),
        value: SessionConfigOptionValue::value_id(advertised_id.to_string()),
    }];
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
            if option_value_advertised(thought, effort) {
                steps.push(ConfigApplyStep {
                    config_id: thought.id.0.to_string(),
                    value: SessionConfigOptionValue::value_id(effort.clone()),
                });
            }
        }
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

fn compose_catalog_id_from_intent(intent: &CursorModelIntent) -> String {
    let fast = intent.fast.unwrap_or(false);
    let mut id = if let Some(version) = intent.base.strip_prefix("grok-") {
        format!("cursor-grok-{version}")
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

fn effort_advertised_in_model_select(
    select: &SessionConfigSelect,
    intent: &CursorModelIntent,
) -> bool {
    let Some(effort) = &intent.effort else {
        return false;
    };
    select_option_ids(select).iter().any(|id| {
        parse_cursor_model_intent(id).is_some_and(|applied| {
            applied.effort.as_deref() == Some(effort.as_str())
                && intent_matches_base_and_fast(intent, &applied, id)
        })
    })
}

fn intent_matches_base_and_fast(
    desired: &CursorModelIntent,
    applied: &CursorModelIntent,
    _applied_raw: &str,
) -> bool {
    cursor_model_intents_match_with_raw(
        &CursorModelIntent {
            base: desired.base.clone(),
            thinking: desired.thinking,
            effort: None,
            fast: desired.fast,
        },
        &CursorModelIntent {
            base: applied.base.clone(),
            thinking: applied.thinking,
            effort: None,
            fast: applied.fast,
        },
        "",
    )
}

fn applied_model_satisfies_pin(options: &[SessionConfigOption], desired: &str) -> bool {
    let Some(applied_id) = read_model_applied(Some(options)) else {
        return false;
    };
    let Some(desired_intent) = parse_cursor_model_intent(desired) else {
        return false;
    };
    let Some(applied_intent) = parse_cursor_model_intent(&applied_id) else {
        return false;
    };
    let Some(model) = model_option(options) else {
        return false;
    };
    let SessionConfigKind::Select(select) = &model.kind else {
        return false;
    };
    if !select_option_ids(select).iter().any(|id| id == &applied_id) {
        return false;
    }

    let fast_ok = if let Some(fast) = model_config_boolean_option(options) {
        read_fast_current_value(fast) == Some(desired_intent.fast.unwrap_or(false))
    } else {
        desired_intent.fast.unwrap_or(false) == applied_intent.fast.unwrap_or(false)
    };
    if !fast_ok {
        return false;
    }

    if cursor_model_intents_match_with_raw(&desired_intent, &applied_intent, &applied_id) {
        return true;
    }

    // Writable thought_level: when current matches this pin's effort, accept; otherwise
    // fall through to spawn-applied family-row satisfaction (#1013).
    if let Some(thought) = thought_level_option(options) {
        if let Some(effort) = &desired_intent.effort {
            if option_value_advertised(thought, effort)
                && read_select_current_value(thought).as_deref() == Some(effort.as_str())
                && intent_matches_base_and_fast(&desired_intent, &applied_intent, &applied_id)
            {
                return true;
            }
        }
    }

    // Issue #1011/#1013: spawn-applied model select values may omit unadvertised effort
    // even when a non-writable thought_level option is advertised.
    if desired_intent.effort.is_none() {
        return false;
    }
    if effort_advertised_in_model_select(select, &desired_intent) {
        return false;
    }
    if applied_intent.effort.is_none()
        && intent_matches_base_and_fast(&desired_intent, &applied_intent, &applied_id)
    {
        return true;
    }
    unique_advertised_base_fast(&select_option_ids(select), &desired_intent).as_deref()
        == Some(applied_id.as_str())
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
    if !model_pins_at_spawn {
        return ids.into_iter().find(|id| id == &intent.base);
    }

    let catalog_id = compose_catalog_id_from_intent(intent);
    if ids.iter().any(|id| id == &catalog_id) {
        return Some(catalog_id);
    }

    if let Some(id) = ids.iter().find(|id| {
        parse_cursor_model_intent(id)
            .is_some_and(|applied| cursor_model_intents_match_with_raw(intent, &applied, id))
    }) {
        return Some(id.clone());
    }

    // Issue #1011: when *this* pin's effort is not advertised for the same base+fast
    // (High on gpt-5.6-sol-high must not block a Grok High pin), map to effort-less base+fast.
    if intent.effort.is_some() && !effort_advertised_in_model_select(select, intent) {
        if let Some(id) = ids.iter().find(|id| {
            parse_cursor_model_intent(id).is_some_and(|applied| {
                applied.effort.is_none() && intent_matches_base_and_fast(intent, &applied, id)
            })
        }) {
            return Some(id.clone());
        }
        return unique_advertised_base_fast(&ids, intent);
    }

    unique_advertised_base_fast(&ids, intent)
}

fn unique_advertised_base_fast(ids: &[String], intent: &CursorModelIntent) -> Option<String> {
    let mut matches = ids.iter().filter(|id| {
        parse_cursor_model_intent(id)
            .is_some_and(|applied| intent_matches_base_and_fast(intent, &applied, id))
    });
    let first = matches.next()?.clone();
    matches.next().is_none().then_some(first)
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
