use super::{
    acp_drain::{parse_json_rpc_id, permission_response},
    task_session::TaskSessionState,
    SessionError, SessionServerEvent,
};

pub(super) fn answer_elicitation(
    state: &mut TaskSessionState,
    request_id: &str,
    action: &str,
    content: Option<&serde_json::Value>,
) -> Result<(), SessionError> {
    let Some(client) = state.acp.client.as_mut() else {
        return Err(SessionError::protocol("session slot missing"));
    };
    let result = client.respond_elicitation(request_id, action, content);
    if result.is_ok()
        || result
            .as_ref()
            .err()
            .is_some_and(|message| message == "ACP elicitation request is no longer pending")
    {
        state.append_to_log(vec![SessionServerEvent::ElicitationResolved {
            request_id: request_id.to_string(),
            action: action.to_string(),
        }])?;
    }
    match result {
        Ok(()) => Ok(()),
        Err(message) if message == "ACP elicitation request is no longer pending" => Ok(()),
        Err(message) => Err(SessionError::protocol(message)),
    }
}

pub(super) fn answer_permission(
    state: &mut TaskSessionState,
    request_id: &str,
    approved: bool,
    reason: Option<&str>,
) -> Result<(), SessionError> {
    let Some(client) = state.acp.client.as_mut() else {
        return Err(SessionError::protocol("session slot missing"));
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
        }])?;
    }
    match result {
        Ok(()) => Ok(()),
        Err(message) if message == "ACP permission request is no longer pending" => Ok(()),
        Err(message) => Err(SessionError::protocol(message)),
    }
}
