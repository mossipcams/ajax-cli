//! Apply operator model pins after ACP session/new or resume/load and read back
//! the harness-reported applied id ([#952](https://github.com/mossipcams/ajax-cli/issues/952)).

use super::catalog::parse_session_new_catalog;
use agent_client_protocol::schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigSelect, SessionConfigSelectOptions,
    SetSessionConfigOptionRequest,
};
use agent_client_protocol::{Agent, ConnectionTo};
use ajax_core::adapters::{
    cursor_model_intents_match, parse_cursor_model_intent, parse_model_selection, ModelSelection,
};
use serde_json::Value;

use super::client::HANDSHAKE_TIMEOUT;

pub struct ApplyModelOutcome {
    /// Model id the harness reports after handshake and any in-band apply.
    pub applied_model: String,
    /// Typed error when an explicit operator pin was refused or could not be proven.
    pub error: Option<String>,
}

/// True when the operator did not pin a specific harness model id.
pub fn is_unspecified_model(raw: Option<&str>) -> bool {
    matches!(raw.map(str::trim), None | Some("") | Some("auto"))
}

/// Read the model id a harness advertises as currently applied on the handshake.
pub fn read_applied_model(
    session_result: &Value,
    config_options: Option<&[SessionConfigOption]>,
) -> String {
    if let Some(options) = config_options {
        if let Some(option) = options
            .iter()
            .find(|option| option.id.0.as_ref() == "model")
        {
            if let SessionConfigKind::Select(select) = &option.kind {
                return select.current_value.0.to_string();
            }
        }
    }
    parse_session_new_catalog(session_result)
        .default_model
        .unwrap_or_default()
}

fn model_matches_pin(applied: &str, selection: &ModelSelection, spawn_pinned: bool) -> bool {
    if applied.is_empty() {
        return false;
    }
    if spawn_pinned {
        if let (Some(desired), Some(applied_intent)) = (
            parse_cursor_model_intent(&selection.model),
            parse_cursor_model_intent(applied),
        ) {
            return cursor_model_intents_match(&desired, &applied_intent);
        }
    }
    if applied != selection.model {
        return false;
    }
    for (key, value) in &selection.options {
        let needle = format!("{key}={value}");
        if !applied.contains(&needle) {
            return false;
        }
    }
    true
}

fn advertised_model_ids(
    session_result: &Value,
    config_options: Option<&[SessionConfigOption]>,
) -> Vec<String> {
    let mut ids = Vec::new();
    if let Some(config_options) = config_options {
        if let Some(option) = config_options
            .iter()
            .find(|option| option.id.0.as_ref() == "model")
        {
            if let SessionConfigKind::Select(select) = &option.kind {
                match &select.options {
                    SessionConfigSelectOptions::Ungrouped(options) => {
                        ids.extend(options.iter().map(|option| option.value.0.to_string()));
                    }
                    SessionConfigSelectOptions::Grouped(groups) => {
                        for group in groups {
                            ids.extend(
                                group
                                    .options
                                    .iter()
                                    .map(|option| option.value.0.to_string()),
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    if let Some(models) = session_result
        .get("models")
        .and_then(|models| models.get("availableModels"))
        .and_then(Value::as_array)
    {
        for entry in models {
            if let Some(id) = entry.get("modelId").and_then(Value::as_str) {
                if !ids.iter().any(|existing| existing == id) {
                    ids.push(id.to_string());
                }
            }
        }
    }
    ids
}

/// Pick the advertised handshake id that best matches a Cursor catalog pin.
fn resolve_cursor_pin_for_apply(
    session_result: &Value,
    config_options: Option<&[SessionConfigOption]>,
    catalog_id: &str,
) -> Option<ModelSelection> {
    let desired = parse_cursor_model_intent(catalog_id)?;
    let mut matches: Vec<(bool, String)> = advertised_model_ids(session_result, config_options)
        .into_iter()
        .filter_map(|id| {
            let applied = parse_cursor_model_intent(&id)?;
            cursor_model_intents_match(&desired, &applied)
                .then_some((applied.fast.unwrap_or(true), id))
        })
        .collect();
    matches.sort_by_key(|(fast, _)| *fast);
    matches.into_iter().next().map(|(_, id)| ModelSelection {
        model: id,
        options: Vec::new(),
    })
}

fn model_config_advertised(
    session_result: &Value,
    config_options: Option<&[SessionConfigOption]>,
) -> bool {
    if config_options
        .is_some_and(|options| options.iter().any(|option| option.id.0.as_ref() == "model"))
    {
        return true;
    }
    if session_result
        .get("configOptions")
        .and_then(Value::as_array)
        .is_some_and(|options| {
            options
                .iter()
                .any(|option| option.get("id").and_then(Value::as_str) == Some("model"))
        })
    {
        return true;
    }
    session_result
        .get("models")
        .and_then(|models| models.get("availableModels"))
        .and_then(Value::as_array)
        .is_some_and(|models| !models.is_empty())
}

async fn set_config_option(
    connection: &ConnectionTo<Agent>,
    session_id: &str,
    config_id: &str,
    value: &str,
) -> Result<Option<String>, String> {
    let request =
        SetSessionConfigOptionRequest::new(session_id.to_string(), config_id.to_string(), value);
    let response = tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        connection.send_request(request).block_task(),
    )
    .await
    .map_err(|_| format!("session/set_config_option {config_id} timed out"))?
    .map_err(|error| format!("session/set_config_option {config_id} failed: {error}"))?;
    let value = serde_json::to_value(response)
        .ok()
        .filter(|value| !value.is_null())
        .map(|value| read_applied_model(&value, None))
        .filter(|model| !model.is_empty());
    Ok(value)
}

async fn apply_in_band(
    connection: &ConnectionTo<Agent>,
    session_id: &str,
    selection: &ModelSelection,
) -> Result<String, String> {
    let mut applied = set_config_option(connection, session_id, "model", &selection.model)
        .await?
        .unwrap_or_else(|| selection.model.clone());
    for (config_id, value) in &selection.options {
        if let Some(next) = set_config_option(connection, session_id, config_id, value).await? {
            applied = next;
        }
    }
    Ok(applied)
}

fn select_value_advertised(select: &SessionConfigSelect, value: &str) -> bool {
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

fn available_model_advertised(session_result: &Value, model: &str) -> bool {
    session_result
        .get("models")
        .and_then(|models| models.get("availableModels"))
        .and_then(Value::as_array)
        .is_some_and(|models| {
            models
                .iter()
                .any(|entry| entry.get("modelId").and_then(Value::as_str) == Some(model))
        })
}

/// True when every piece of `selection` is an exact advertised handshake value.
pub fn selection_fully_advertised(
    session_result: &Value,
    config_options: Option<&[SessionConfigOption]>,
    selection: &ModelSelection,
) -> bool {
    let model_ok = config_option_value_advertised(config_options, "model", &selection.model)
        || available_model_advertised(session_result, &selection.model);
    if !model_ok {
        return false;
    }
    selection
        .options
        .iter()
        .all(|(config_id, value)| config_option_value_advertised(config_options, config_id, value))
}

/// Apply `desired_model` when advertised and return the harness-reported applied id.
pub async fn apply_model_pin(
    connection: &ConnectionTo<Agent>,
    session_id: &str,
    session_result: &Value,
    config_options: Option<&[SessionConfigOption]>,
    desired_model: Option<&str>,
    model_pins_at_spawn: bool,
) -> ApplyModelOutcome {
    let mut applied = read_applied_model(session_result, config_options);

    if is_unspecified_model(desired_model) {
        return ApplyModelOutcome {
            applied_model: applied,
            error: None,
        };
    }

    let raw = desired_model.unwrap_or_default().trim();
    let Some(selection) = parse_model_selection(raw) else {
        return ApplyModelOutcome {
            applied_model: applied,
            error: Some(format!(
                "session model {raw:?} is invalid — model id must not contain whitespace or exceed 128 chars"
            )),
        };
    };

    let apply_selection = (if model_pins_at_spawn {
        resolve_cursor_pin_for_apply(session_result, config_options, raw)
    } else {
        None
    })
    .filter(|selection| selection_fully_advertised(session_result, config_options, selection))
    .or_else(|| {
        selection_fully_advertised(session_result, config_options, &selection)
            .then_some(selection.clone())
    });
    if model_config_advertised(session_result, config_options) {
        if let Some(apply_selection) = apply_selection {
            match apply_in_band(connection, session_id, &apply_selection).await {
                Ok(next) => applied = next,
                Err(error) => {
                    return ApplyModelOutcome {
                        applied_model: applied,
                        error: Some(format!("session model {raw} was refused — {error}")),
                    };
                }
            }
        }
    }

    if model_matches_pin(&applied, &selection, model_pins_at_spawn) {
        return ApplyModelOutcome {
            applied_model: applied,
            error: None,
        };
    }

    ApplyModelOutcome {
        applied_model: applied.clone(),
        error: Some(if applied.is_empty() {
            if model_pins_at_spawn {
                format!(
                    "session model {raw} could not be verified — harness did not report an applied model after spawn argv pin"
                )
            } else {
                format!("session model {raw} could not be verified — harness did not report an applied model")
            }
        } else {
            format!("session model {raw} was refused — harness is running {applied}")
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn read_applied_model_from_config_options_shape() {
        let result = json!({
            "sessionId": "s1",
            "configOptions": [{
                "id": "model",
                "currentValue": "composer-2.5",
                "options": [{ "value": "composer-2.5", "name": "Composer" }]
            }]
        });
        assert_eq!(read_applied_model(&result, None), "composer-2.5");
    }

    #[test]
    fn read_applied_model_from_available_models_shape() {
        let result = json!({
            "sessionId": "s1",
            "models": {
                "currentModelId": "gpt-5.6-sol[medium]",
                "availableModels": [
                    { "modelId": "gpt-5.6-sol[medium]", "name": "GPT-5.6-Sol (medium)" }
                ]
            }
        });
        assert_eq!(read_applied_model(&result, None), "gpt-5.6-sol[medium]");
    }

    #[test]
    fn unspecified_model_sentinel() {
        assert!(is_unspecified_model(None));
        assert!(is_unspecified_model(Some("")));
        assert!(is_unspecified_model(Some("auto")));
        assert!(!is_unspecified_model(Some("composer-2.5")));
    }

    // Regression for #952: fake ACP must advertise model controls in SDK shape.
    #[test]
    fn session_config_option_json_shape_for_fake_acp() {
        use agent_client_protocol::schema::v1::{
            NewSessionResponse, SessionConfigOption, SessionConfigSelectOption,
        };
        let opt = SessionConfigOption::select(
            "model",
            "Model",
            "harness-default",
            vec![SessionConfigSelectOption::new("harness-default", "Default")],
        );
        let resp = NewSessionResponse::new("fake-sess-1").config_options(vec![opt]);
        let json = serde_json::to_value(&resp).expect("json");
        assert_eq!(
            json.pointer("/configOptions/0/type")
                .and_then(|v| v.as_str()),
            Some("select")
        );
    }

    // Regression for #952: snapshot authority must come from handshake evidence,
    // not from echoing the attach-plan pin when the harness reports something else.
    #[test]
    fn applied_model_prefers_handshake_evidence_over_desired_pin_issue_952() {
        let handshake = json!({
            "sessionId": "s1",
            "configOptions": [{
                "id": "model",
                "currentValue": "harness-default",
                "options": [
                    { "value": "harness-default", "name": "Default" },
                    { "value": "composer-2.5", "name": "Composer" }
                ]
            }]
        });
        assert_eq!(read_applied_model(&handshake, None), "harness-default");
        assert_ne!(read_applied_model(&handshake, None), "composer-2.5");
    }

    // Regression for #954: only exact advertised handshake values qualify for in-band apply.
    #[test]
    fn selection_fully_advertised_requires_exact_handshake_values_issue_954() {
        use agent_client_protocol::schema::v1::{SessionConfigOption, SessionConfigSelectOption};
        use ajax_core::adapters::parse_model_selection;

        let options = vec![SessionConfigOption::select(
            "model",
            "Model",
            "harness-default",
            vec![
                SessionConfigSelectOption::new("harness-default", "Default"),
                SessionConfigSelectOption::new("composer-2.5", "Composer"),
            ],
        )];
        let handshake = json!({ "sessionId": "s1" });
        let composer = parse_model_selection("composer-2.5").unwrap();
        let catalog = parse_model_selection("cursor-grok-4.6-high").unwrap();
        assert!(selection_fully_advertised(
            &handshake,
            Some(&options),
            &composer
        ));
        assert!(!selection_fully_advertised(
            &handshake,
            Some(&options),
            &catalog
        ));
    }

    // Regression for #979: spawn-pinned catalog ids must not silently accept a CLI default.
    #[test]
    fn spawn_pinned_catalog_mismatch_errors_issue_979() {
        use ajax_core::adapters::parse_model_selection;

        let handshake = json!({
            "sessionId": "s1",
            "configOptions": [{
                "id": "model",
                "currentValue": "composer-2.5[fast=true]",
                "options": [
                    { "value": "composer-2.5", "name": "Composer" },
                    { "value": "composer-2.5[fast=true]", "name": "Composer Fast" }
                ]
            }]
        });
        let pin = parse_model_selection("cursor-grok-4.6-high").unwrap();
        assert!(!model_matches_pin(
            &read_applied_model(&handshake, None),
            &pin,
            true
        ));
    }

    // Regression for #954: handshake ids in a different string form still match.
    #[test]
    fn catalog_pin_matches_advertised_acp_id_issue_954() {
        use ajax_core::adapters::parse_model_selection;

        let pin = parse_model_selection("cursor-grok-4.6-high").unwrap();
        assert!(model_matches_pin(
            "grok-4.6[effort=high,fast=true]",
            &pin,
            true
        ));
        assert!(model_matches_pin(
            "grok-4.6[effort=high,fast=false]",
            &pin,
            true
        ));
    }
}
