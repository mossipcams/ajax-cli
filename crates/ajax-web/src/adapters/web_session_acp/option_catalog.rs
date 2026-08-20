//! Last-advertised harness `configOptions` cache for New Task pickers.

use super::config_option_descriptors::{config_option_descriptors, ConfigOptionDescriptor};
use agent_client_protocol::schema::v1::SessionConfigOption;
use ajax_core::models::AgentClient;
use std::{collections::HashMap, sync::Mutex};

struct CacheEntry {
    harness_version: String,
    options: Vec<ConfigOptionDescriptor>,
}

static CACHE: Mutex<Option<HashMap<String, CacheEntry>>> = Mutex::new(None);

fn agent_cache_key(agent: AgentClient) -> String {
    match agent {
        AgentClient::Cursor => "cursor".to_string(),
        AgentClient::Codex => "codex".to_string(),
        AgentClient::Claude => "claude".to_string(),
        AgentClient::Pi => "pi".to_string(),
        AgentClient::Other => "other".to_string(),
    }
}

fn harness_version(agent: AgentClient) -> String {
    crate::slices::session_models::harness_version(agent)
}

/// Store the latest advertised options for `agent` when a session handshakes.
pub fn remember_harness_config_options(agent: AgentClient, options: &[SessionConfigOption]) {
    if options.is_empty() {
        return;
    }
    let key = agent_cache_key(agent);
    let version = harness_version(agent);
    let descriptors = config_option_descriptors(options);
    if let Ok(mut guard) = CACHE.lock() {
        guard.get_or_insert_with(HashMap::new).insert(
            key,
            CacheEntry {
                harness_version: version,
                options: descriptors,
            },
        );
    }
}

/// Cached descriptors for `agent`, when the harness version still matches.
pub fn cached_harness_config_options(agent: AgentClient) -> Option<Vec<ConfigOptionDescriptor>> {
    let key = agent_cache_key(agent);
    let version = harness_version(agent);
    // An unreadable version can't prove the cache is current (session_models.rs).
    if version.is_empty() {
        return None;
    }
    let guard = CACHE.lock().ok()?;
    let entry = guard.as_ref()?.get(&key)?;
    if entry.harness_version != version {
        return None;
    }
    Some(entry.options.clone())
}

#[cfg(test)]
pub(crate) fn clear_option_catalog_cache() {
    if let Ok(mut guard) = CACHE.lock() {
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{SessionConfigOption, SessionConfigSelectOption};
    use ajax_core::models::AgentClient;

    #[test]
    fn cached_harness_config_options_misses_when_harness_version_is_empty() {
        clear_option_catalog_cache();
        let options = vec![SessionConfigOption::select(
            "model",
            "Model",
            "m1",
            vec![SessionConfigSelectOption::new("m1", "M1")],
        )];
        remember_harness_config_options(AgentClient::Other, &options);
        assert!(cached_harness_config_options(AgentClient::Other).is_none());
    }
}
