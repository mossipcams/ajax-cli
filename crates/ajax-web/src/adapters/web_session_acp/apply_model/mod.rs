//! Apply operator model pins after ACP session/new or resume/load and read back
//! the harness-reported applied id ([#952](https://github.com/mossipcams/ajax-cli/issues/952)).

use super::catalog::parse_session_new_catalog;
use super::config_options::{
    apply_steps_needing_send, build_set_config_request, is_unspecified_model,
    map_pin_to_apply_steps, model_config_advertised, option_value_advertised, pin_satisfied,
    read_model_applied, replace_config_options, step_matches_current,
    sync_session_result_config_options, ConfigApplyStep,
};
use agent_client_protocol::schema::v1::SessionConfigOptionValue;
use agent_client_protocol::schema::v1::{SessionConfigOption, SetSessionConfigOptionResponse};
use agent_client_protocol::{Agent, ConnectionTo};
use serde_json::Value;

use super::client::HANDSHAKE_TIMEOUT;

pub struct ApplyModelOutcome {
    /// Model option `currentValue` after handshake and any in-band apply.
    pub applied_model: String,
    /// Complete advertised list after the last successful set_config response.
    pub config_options: Option<Vec<SessionConfigOption>>,
    /// Typed error when an explicit operator pin was refused or could not be proven.
    pub error: Option<String>,
}

/// Apply one advertised `{ configId, value }` on a live session (AoE contract).
pub async fn apply_config_option(
    connection: &ConnectionTo<Agent>,
    session_id: &str,
    config_id: &str,
    value: SessionConfigOptionValue,
    config_options: Option<&[SessionConfigOption]>,
) -> ApplyModelOutcome {
    let mut stored = config_options.map(|options| options.to_vec());
    let applied = read_applied_model(&Value::Null, stored.as_deref());
    let step = ConfigApplyStep {
        config_id: config_id.to_string(),
        value,
    };
    let options = stored.as_deref().unwrap_or(&[]);
    if !options
        .iter()
        .any(|option| option.id.0.as_ref() == config_id)
    {
        return ApplyModelOutcome {
            applied_model: applied,
            config_options: stored,
            error: Some(format!(
                "config option {config_id} is not advertised on this session"
            )),
        };
    }
    if step_matches_current(options, &step) {
        return ApplyModelOutcome {
            applied_model: applied,
            config_options: stored,
            error: None,
        };
    }
    if let SessionConfigOptionValue::ValueId { value: id } = &step.value {
        if let Some(option) = options
            .iter()
            .find(|option| option.id.0.as_ref() == config_id)
        {
            if !option_value_advertised(option, id.0.as_ref()) {
                return ApplyModelOutcome {
                    applied_model: applied.clone(),
                    config_options: stored,
                    error: Some(format!(
                        "config option {config_id}={} is not advertised",
                        id.0.as_ref()
                    )),
                };
            }
        }
    }
    match apply_in_band(
        connection,
        session_id,
        std::slice::from_ref(&step),
        stored.clone(),
    )
    .await
    {
        Ok((next, options)) => {
            replace_config_options(&mut stored, options);
            ApplyModelOutcome {
                applied_model: next,
                config_options: stored,
                error: None,
            }
        }
        Err(error) => ApplyModelOutcome {
            applied_model: applied.clone(),
            config_options: stored,
            error: Some(format!("config option {config_id} was refused — {error}")),
        },
    }
}

/// True when the harness-reported applied id satisfies the operator pin (string fallback).
pub fn operator_pin_satisfied(
    operator_pin: &str,
    applied_model: &str,
    model_pins_at_spawn: bool,
) -> bool {
    if is_unspecified_model(Some(operator_pin)) {
        if !model_pins_at_spawn {
            return true;
        }
        return ajax_core::adapters::cursor_unspecified_spawn_satisfied(applied_model);
    }
    if let (Some(desired), Some(applied)) = (
        ajax_core::adapters::parse_cursor_model_intent(operator_pin),
        ajax_core::adapters::parse_cursor_model_intent(applied_model),
    ) {
        if model_pins_at_spawn {
            return ajax_core::adapters::cursor_model_intents_match(&desired, &applied);
        }
    }
    operator_pin.trim() == applied_model.trim()
}

/// Read the model id a harness advertises as currently applied on the handshake.
pub fn read_applied_model(
    session_result: &Value,
    config_options: Option<&[SessionConfigOption]>,
) -> String {
    read_model_applied(config_options).unwrap_or_else(|| {
        parse_session_new_catalog(session_result)
            .default_model
            .unwrap_or_default()
    })
}

async fn set_config_option(
    connection: &ConnectionTo<Agent>,
    session_id: &str,
    step: &ConfigApplyStep,
) -> Result<SetSessionConfigOptionResponse, String> {
    let config_id = step.config_id.clone();
    let request = build_set_config_request(session_id, &config_id, step.value.clone());
    tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        connection.send_request(request).block_task(),
    )
    .await
    .map_err(|_| format!("session/set_config_option {config_id} timed out"))?
    .map_err(|error| format!("session/set_config_option {config_id} failed: {error}"))
}

async fn apply_in_band(
    connection: &ConnectionTo<Agent>,
    session_id: &str,
    steps: &[ConfigApplyStep],
    config_options: Option<Vec<SessionConfigOption>>,
) -> Result<(String, Vec<SessionConfigOption>), String> {
    let mut latest = config_options.unwrap_or_default();
    // ponytail: bounded rounds — model set_config can reset sibling options so we
    // re-filter after each response instead of trusting the pre-apply skip list.
    const MAX_ROUNDS: usize = 8;
    for _ in 0..MAX_ROUNDS {
        let pending = apply_steps_needing_send(&latest, steps);
        if pending.is_empty() {
            break;
        }
        for step in &pending {
            let response = set_config_option(connection, session_id, step).await?;
            latest = response.config_options;
        }
    }
    let applied = read_model_applied(Some(&latest)).unwrap_or_default();
    Ok((applied, latest))
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
    let mut stored = config_options.map(|options| options.to_vec());
    let mut applied = read_applied_model(session_result, stored.as_deref());

    if is_unspecified_model(desired_model) {
        if model_pins_at_spawn && !pin_satisfied(stored.as_deref(), "", model_pins_at_spawn) {
            if model_config_advertised(stored.as_deref()) {
                match map_pin_to_apply_steps(
                    stored.as_deref().unwrap_or(&[]),
                    "",
                    model_pins_at_spawn,
                ) {
                    Ok(steps) => {
                        if !apply_steps_needing_send(stored.as_deref().unwrap_or(&[]), &steps)
                            .is_empty()
                        {
                            match apply_in_band(connection, session_id, &steps, stored.clone())
                                .await
                            {
                                Ok((next, options)) => {
                                    applied = next;
                                    replace_config_options(&mut stored, options);
                                }
                                Err(error) => {
                                    return ApplyModelOutcome {
                                        applied_model: applied.clone(),
                                        config_options: stored,
                                        error: Some(format!(
                                            "session model defaulted to {applied} — could not clear Fast: {error}"
                                        )),
                                    };
                                }
                            }
                        }
                    }
                    Err(error) => {
                        return ApplyModelOutcome {
                            applied_model: applied.clone(),
                            config_options: stored,
                            error: Some(error),
                        };
                    }
                }
            }
            if !pin_satisfied(stored.as_deref(), "", model_pins_at_spawn) {
                return ApplyModelOutcome {
                    applied_model: applied.clone(),
                    config_options: stored,
                    error: Some(format!(
                        "session model defaulted to {applied} — Ajax expects non-Fast default"
                    )),
                };
            }
        }
        return ApplyModelOutcome {
            applied_model: applied,
            config_options: stored,
            error: None,
        };
    }

    let raw = desired_model.unwrap_or_default().trim();
    if pin_satisfied(stored.as_deref(), raw, model_pins_at_spawn) {
        return ApplyModelOutcome {
            applied_model: applied,
            config_options: stored,
            error: None,
        };
    }

    if !model_config_advertised(stored.as_deref()) {
        return ApplyModelOutcome {
            applied_model: applied.clone(),
            config_options: stored,
            error: Some(format!(
                "session model {raw} could not be verified — harness did not advertise model controls"
            )),
        };
    }

    let options = stored.as_deref().unwrap_or(&[]);
    let steps = match map_pin_to_apply_steps(options, raw, model_pins_at_spawn) {
        Ok(steps) => steps,
        Err(error) => {
            return ApplyModelOutcome {
                applied_model: applied.clone(),
                config_options: stored,
                error: Some(if error.contains("invalid") {
                    error
                } else {
                    format!("session model {raw} was refused — {error}")
                }),
            };
        }
    };

    if apply_steps_needing_send(options, &steps).is_empty() {
        return ApplyModelOutcome {
            applied_model: applied,
            config_options: stored,
            error: None,
        };
    }

    match apply_in_band(connection, session_id, &steps, stored.clone()).await {
        Ok((next, options)) => {
            applied = next.clone();
            replace_config_options(&mut stored, options);
        }
        Err(error) => {
            return ApplyModelOutcome {
                applied_model: applied.clone(),
                config_options: stored,
                error: Some(format!("session model {raw} was refused — {error}")),
            };
        }
    }

    if pin_satisfied(stored.as_deref(), raw, model_pins_at_spawn) {
        ApplyModelOutcome {
            applied_model: applied,
            config_options: stored,
            error: None,
        }
    } else {
        ApplyModelOutcome {
            applied_model: applied.clone(),
            config_options: stored,
            error: Some(if applied.is_empty() {
                format!("session model {raw} could not be verified — harness did not report an applied model")
            } else {
                format!("session model {raw} was refused — harness is running {applied}")
            }),
        }
    }
}

pub(super) fn sync_live_model_config(
    session_result: &mut Value,
    config_options: &mut [SessionConfigOption],
    applied_model: &str,
) {
    if applied_model.is_empty() {
        return;
    }
    if let Some(option) = config_options
        .iter_mut()
        .find(|option| super::config_options::model_option(std::slice::from_ref(option)).is_some())
    {
        if let agent_client_protocol::schema::v1::SessionConfigKind::Select(select) =
            &mut option.kind
        {
            select.current_value = agent_client_protocol::schema::v1::SessionConfigValueId::from(
                applied_model.to_string(),
            );
        }
    } else if let Some(option) = config_options
        .iter_mut()
        .find(|option| option.id.0.as_ref() == "model")
    {
        if let agent_client_protocol::schema::v1::SessionConfigKind::Select(select) =
            &mut option.kind
        {
            select.current_value = agent_client_protocol::schema::v1::SessionConfigValueId::from(
                applied_model.to_string(),
            );
        }
    }
    sync_session_result_config_options(session_result, config_options);
}
