//! Deferred ACP form elicitation handling for the Web Session host.

use super::client::AcpClientEvent;
use agent_client_protocol::{
    schema::v1::{
        CreateElicitationRequest, CreateElicitationResponse, ElicitationAcceptAction,
        ElicitationAction, ElicitationContentValue, ElicitationFormCapabilities,
        ElicitationFormMode, ElicitationMode, ElicitationPropertySchema, ElicitationSchema,
    },
    Error, Responder,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{mpsc::Sender, Arc, Mutex},
};

pub(super) struct PendingElicitation {
    responder: Responder<CreateElicitationResponse>,
}

pub(super) type PendingElicitations = Arc<Mutex<HashMap<String, PendingElicitation>>>;

pub(super) fn advertised_elicitation_capabilities(
) -> agent_client_protocol::schema::v1::ElicitationCapabilities {
    agent_client_protocol::schema::v1::ElicitationCapabilities::new()
        .form(ElicitationFormCapabilities::new())
}

pub(super) fn handle_create_elicitation(
    request: CreateElicitationRequest,
    responder: Responder<CreateElicitationResponse>,
    pending: &PendingElicitations,
    events: &Sender<AcpClientEvent>,
) -> Result<(), Error> {
    let request_id = responder.id().to_string();
    let mode = request.mode.clone();
    match mode {
        ElicitationMode::Form(form) => accept_form_elicitation(
            request.message,
            form,
            request_id,
            responder,
            pending,
            events,
        ),
        ElicitationMode::Url(_) => {
            Err(Error::invalid_params().data("URL elicitation is not advertised"))
        }
        ElicitationMode::Other(other) => {
            Err(Error::invalid_params()
                .data(format!("unsupported elicitation mode: {}", other.mode)))
        }
        _ => Err(Error::invalid_params().data("unsupported elicitation mode")),
    }
}

fn accept_form_elicitation(
    message: String,
    form: ElicitationFormMode,
    request_id: String,
    responder: Responder<CreateElicitationResponse>,
    pending: &PendingElicitations,
    events: &Sender<AcpClientEvent>,
) -> Result<(), Error> {
    if schema_collects_secrets(&form.requested_schema) {
        return Err(Error::invalid_params()
            .data("form elicitation must not collect secrets, passwords, or tokens"));
    }
    let schema = serde_json::to_value(&form.requested_schema)
        .map_err(|error| Error::internal_error().data(error.to_string()))?;
    pending
        .lock()
        .unwrap()
        .insert(request_id.clone(), PendingElicitation { responder });
    let _ = events.send(AcpClientEvent::ElicitationRequest {
        request_id,
        message,
        schema,
    });
    Ok(())
}

fn schema_collects_secrets(schema: &ElicitationSchema) -> bool {
    schema
        .properties
        .iter()
        .any(|(name, property)| forbidden_field_name(name) || property_collects_secrets(property))
}

fn property_collects_secrets(property: &ElicitationPropertySchema) -> bool {
    match property {
        ElicitationPropertySchema::String(_) => false,
        ElicitationPropertySchema::Number(_) => false,
        ElicitationPropertySchema::Integer(_) => false,
        ElicitationPropertySchema::Boolean(_) => false,
        ElicitationPropertySchema::Array(_) => false,
        ElicitationPropertySchema::Other(other) => other.type_.contains("password"),
        _ => true,
    }
}

fn forbidden_field_name(name: &str) -> bool {
    let normalized = name.trim().to_ascii_lowercase().replace(['-', '_'], "");
    ["password", "secret", "token", "apikey", "authcode", "otp"]
        .iter()
        .any(|needle| normalized.contains(needle))
}

pub(super) fn respond_elicitation(
    pending: &PendingElicitations,
    request_id: &str,
    action: ElicitationAction,
) -> Result<(), String> {
    let item = pending
        .lock()
        .unwrap()
        .remove(request_id)
        .ok_or_else(|| "ACP elicitation request is no longer pending".to_string())?;
    item.responder
        .respond(CreateElicitationResponse::new(action))
        .map_err(|error| error.to_string())
}

pub(super) fn cancel_elicitations(pending: &PendingElicitations) -> Vec<String> {
    let drained: Vec<_> = pending.lock().unwrap().drain().collect();
    drained
        .into_iter()
        .map(|(request_id, item)| {
            let _ = item
                .responder
                .respond(CreateElicitationResponse::new(ElicitationAction::Cancel));
            request_id
        })
        .collect()
}

pub(crate) fn wire_content_from_json(
    content: &Value,
) -> Result<BTreeMap<String, ElicitationContentValue>, String> {
    let Some(object) = content.as_object() else {
        return Err("elicitation content must be an object".to_string());
    };
    let mut mapped = BTreeMap::new();
    for (key, value) in object {
        mapped.insert(key.clone(), json_to_content_value(value)?);
    }
    Ok(mapped)
}

pub(crate) fn accept_action(
    content: BTreeMap<String, ElicitationContentValue>,
) -> ElicitationAction {
    ElicitationAction::Accept(ElicitationAcceptAction::new().content(content))
}

fn json_to_content_value(value: &Value) -> Result<ElicitationContentValue, String> {
    match value {
        Value::String(text) => Ok(ElicitationContentValue::String(text.clone())),
        Value::Bool(flag) => Ok(ElicitationContentValue::Boolean(*flag)),
        Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                Ok(ElicitationContentValue::Integer(integer))
            } else if let Some(float) = number.as_f64() {
                Ok(ElicitationContentValue::Number(float))
            } else {
                Err("unsupported numeric elicitation value".to_string())
            }
        }
        Value::Array(items) => {
            let strings = items
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| "elicitation array values must be strings".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ElicitationContentValue::StringArray(strings))
        }
        _ => Err("unsupported elicitation content value".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::ElicitationSessionScope;

    #[test]
    fn rejects_secret_field_names_in_form_schema() {
        let schema = ElicitationSchema::new().string("api_token", true);
        assert!(schema_collects_secrets(&schema));
    }

    #[test]
    fn accepts_plain_form_schema() {
        let schema = ElicitationSchema::new()
            .string("name", true)
            .boolean("confirmed", false);
        assert!(!schema_collects_secrets(&schema));
    }

    #[test]
    fn url_mode_is_not_advertised_in_capabilities() {
        let caps = advertised_elicitation_capabilities();
        assert!(caps.form.is_some());
        assert!(caps.url.is_none());
    }

    #[test]
    fn wire_content_round_trips_json_object() {
        let content = serde_json::json!({
            "name": "Ada",
            "rating": 4.5,
            "confirmed": true,
            "tags": ["a", "b"]
        });
        let mapped = wire_content_from_json(&content).expect("content");
        assert_eq!(
            mapped.get("name"),
            Some(&ElicitationContentValue::String("Ada".into()))
        );
        assert!(matches!(
            mapped.get("rating"),
            Some(ElicitationContentValue::Number(_))
        ));
        assert_eq!(
            mapped.get("confirmed"),
            Some(&ElicitationContentValue::Boolean(true))
        );
        assert!(matches!(
            mapped.get("tags"),
            Some(ElicitationContentValue::StringArray(_))
        ));
    }

    #[test]
    fn form_mode_schema_serializes_for_browser_wire() {
        let form = ElicitationFormMode::new(
            ElicitationSessionScope::new("sess-1"),
            ElicitationSchema::new()
                .string("choice", true)
                .number("count", 1.0, 10.0, false),
        );
        let value = serde_json::to_value(&form.requested_schema).expect("schema");
        assert!(value.get("properties").is_some());
    }
}
