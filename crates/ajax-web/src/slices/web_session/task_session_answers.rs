use super::{
    acp_drain::{parse_json_rpc_id, permission_response},
    task_session::TaskSessionState,
    SessionServerEvent,
};

pub(super) fn answer_elicitation(
    state: &mut TaskSessionState,
    request_id: &str,
    action: &str,
    content: Option<&serde_json::Value>,
) -> Result<(), String> {
    use crate::adapters::web_session_acp::sdk_elicitation::{
        accept_action, wire_content_from_json,
    };
    use agent_client_protocol::schema::v1::ElicitationAction;

    let Some(client) = state.client.as_mut() else {
        return Err("session slot missing".to_string());
    };
    let acp_action = match action {
        "accept" => {
            let payload = content.ok_or_else(|| {
                "elicitation accept requires content matching the requested schema".to_string()
            })?;
            accept_action(wire_content_from_json(payload)?)
        }
        "decline" => ElicitationAction::Decline,
        "cancel" => ElicitationAction::Cancel,
        other => return Err(format!("unsupported elicitation action: {other}")),
    };
    let result = client.respond_elicitation(request_id, acp_action);
    if result.is_ok()
        || result
            .as_ref()
            .err()
            .is_some_and(|message| message == "ACP elicitation request is no longer pending")
    {
        state.append_to_log(vec![SessionServerEvent::ElicitationResolved {
            request_id: request_id.to_string(),
            action: action.to_string(),
        }]);
    }
    match result {
        Ok(()) => Ok(()),
        Err(message) if message == "ACP elicitation request is no longer pending" => Ok(()),
        Err(message) => Err(message),
    }
}

pub(super) fn answer_permission(
    state: &mut TaskSessionState,
    request_id: &str,
    approved: bool,
    reason: Option<&str>,
) -> Result<(), String> {
    let Some(client) = state.client.as_mut() else {
        return Err("session slot missing".to_string());
    };
    let id = parse_json_rpc_id(request_id);
    let result = client.respond_client_request(&id, permission_response(approved, reason));
    if result.is_ok()
        || result
            .as_ref()
            .err()
            .is_some_and(|message| message == "ACP permission request is no longer pending")
    {
        state.append_to_log(vec![SessionServerEvent::PermissionResolved {
            request_id: request_id.to_string(),
            approved,
        }]);
    }
    match result {
        Ok(()) => Ok(()),
        Err(message) if message == "ACP permission request is no longer pending" => Ok(()),
        Err(message) => Err(message),
    }
}
