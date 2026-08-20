//! Per-harness model catalog for task creation and orchestration chat.
//!
//! Cursor lists its models through the CLI (`agent models`). The other
//! harnesses advertise theirs on the ACP `session/new` handshake, which costs a
//! short-lived bridge process — too slow to repeat on every page.
//!
//! A catalog only changes when the harness itself changes, so the cache is keyed
//! by the harness CLI version rather than by a clock: read the version (cheap),
//! reuse the stored catalog when it matches, and re-read the catalog only after
//! the harness has been updated.

use ajax_core::{
    adapters::{parse_cursor_model_intent, CURSOR_DEFAULT_MODEL},
    models::AgentClient,
};
use serde::Serialize;
use std::{collections::HashMap, sync::Mutex};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionModelOption {
    pub id: String,
    pub label: String,
    /// Reasoning levels advertised for this Cursor base (slim catalog only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub efforts: Option<Vec<String>>,
    /// True when a Fast sibling exists for this Cursor base (slim catalog only).
    #[serde(rename = "hasFast", skip_serializing_if = "Option::is_none")]
    pub has_fast: Option<bool>,
}

/// A second axis beside the model list — the reasoning level, which Cursor
/// bakes into its model ids and the bridges expose as their own option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionModelGroup {
    /// Config id the harness answers to, e.g. `effort`.
    pub id: String,
    pub label: String,
    pub options: Vec<SessionModelOption>,
    pub default: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionModelsResponse {
    pub models: Vec<SessionModelOption>,
    /// Model the harness runs when the operator picks nothing.
    pub default: String,
    /// Agent this catalog belongs to, echoed so the browser can cache per agent.
    pub agent: String,
    /// Reasoning level, when this harness exposes one separately.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<SessionModelGroup>,
    /// Why the catalog is empty, when the harness could not be read at all.
    /// An empty list with no error means the harness offers no choice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Harness version this catalog was read from.
    pub harness_version: String,
}

struct Cache {
    harness_version: String,
    response: SessionModelsResponse,
}

static CACHE: Mutex<Option<HashMap<String, Cache>>> = Mutex::new(None);

/// Version string for the harness CLI, used as the cache key. Empty when the
/// CLI cannot be asked, which keeps the catalog uncached rather than stale.
pub fn harness_version(agent: AgentClient) -> String {
    let program = match agent {
        AgentClient::Cursor => "agent",
        AgentClient::Codex => "codex",
        AgentClient::Claude => "claude",
        AgentClient::Pi => "pi",
        AgentClient::Other => return String::new(),
    };
    let Some(mut command) = crate::adapters::program::harness_command(program) else {
        return String::new();
    };
    command
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Agent name as the browser sends it, mapped to a client Ajax can start.
pub fn agent_client_from_name(agent: &str) -> AgentClient {
    match agent.trim().to_ascii_lowercase().as_str() {
        "codex" => AgentClient::Codex,
        "claude" => AgentClient::Claude,
        "pi" => AgentClient::Pi,
        "cursor" | "" => AgentClient::Cursor,
        _ => AgentClient::Other,
    }
}

/// List the models `agent` can run. Soft-fails to the harness default alone.
pub fn list_session_models(agent: &str) -> SessionModelsResponse {
    let key = agent.trim().to_ascii_lowercase();
    let key = if key.is_empty() {
        "cursor".to_string()
    } else {
        key
    };

    let version = harness_version(agent_client_from_name(&key));

    if let Ok(guard) = CACHE.lock() {
        if let Some(cache) = guard.as_ref().and_then(|entries| entries.get(&key)) {
            // An unreadable version can't prove the cache is current.
            if !version.is_empty() && cache.harness_version == version {
                return cache.response.clone();
            }
        }
    }

    let mut response = fetch_catalog(&key);
    response.harness_version = version.clone();

    // Never cache a failure: a harness that was briefly unreachable would then
    // stay "not installed" until its version changed, which may be never.
    if response.error.is_none() && !version.is_empty() {
        if let Ok(mut guard) = CACHE.lock() {
            guard.get_or_insert_with(HashMap::new).insert(
                key,
                Cache {
                    harness_version: version,
                    response: response.clone(),
                },
            );
        }
    }

    response
}

#[cfg(test)]
pub(crate) fn cached_versions() -> Vec<(String, String)> {
    let guard = CACHE.lock().unwrap();
    guard
        .as_ref()
        .map(|entries| {
            entries
                .iter()
                .map(|(agent, cache)| (agent.clone(), cache.harness_version.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn fetch_catalog(agent: &str) -> SessionModelsResponse {
    let client = agent_client_from_name(agent);
    if client == AgentClient::Cursor {
        let raw = fetch_models_from_agent().unwrap_or_else(|| {
            vec![SessionModelOption {
                id: "auto".to_string(),
                label: "Auto".to_string(),
                efforts: None,
                has_fast: None,
            }]
        });
        return SessionModelsResponse {
            models: raw,
            default: CURSOR_DEFAULT_MODEL.to_string(),
            agent: agent.to_string(),
            // Cursor carries its reasoning level inside the model id.
            reasoning: None,
            error: None,
            harness_version: String::new(),
        };
    }

    // A harness Ajax cannot start at all is an install or PATH problem, not a
    // harness with nothing to offer. Say which, rather than showing a bare list.
    if let Some(missing) = missing_acp_program(client) {
        return SessionModelsResponse {
            models: Vec::new(),
            default: String::new(),
            agent: agent.to_string(),
            reasoning: None,
            error: Some(missing),
            harness_version: String::new(),
        };
    }

    // The bridges only advertise their catalog inside a session, so this costs
    // one short-lived ACP process per cache miss.
    let catalog =
        crate::adapters::web_session_acp::read_agent_model_catalog(client, &std::env::temp_dir());
    let models = catalog
        .models
        .into_iter()
        .map(|(id, label)| SessionModelOption {
            id,
            label,
            efforts: None,
            has_fast: None,
        })
        .collect::<Vec<_>>();
    let default = catalog
        .default_model
        .or_else(|| models.first().map(|model| model.id.clone()))
        .unwrap_or_default();
    SessionModelsResponse {
        error: models
            .is_empty()
            .then(|| format!("{agent} started but listed no models")),
        models,
        default,
        agent: agent.to_string(),
        reasoning: catalog.reasoning.map(|group| SessionModelGroup {
            id: group.id,
            label: group.label,
            default: group
                .current
                .clone()
                .or_else(|| group.options.first().map(|(id, _)| id.clone()))
                .unwrap_or_default(),
            options: group
                .options
                .into_iter()
                .map(|(id, label)| SessionModelOption {
                    id,
                    label,
                    efforts: None,
                    has_fast: None,
                })
                .collect(),
        }),
        harness_version: String::new(),
    }
}

/// Install hint when none of a harness's ACP programs can be found.
fn missing_acp_program(client: AgentClient) -> Option<String> {
    let launch = ajax_core::adapters::acp_launch_for_agent(client)?;
    let found = launch
        .candidates
        .iter()
        .any(|(program, _)| crate::adapters::program::resolve_program(program).is_some());
    (!found).then(|| {
        format!(
            "{} is not installed — {}",
            launch.candidates[0].0, launch.install_hint
        )
    })
}

fn fetch_models_from_agent() -> Option<Vec<SessionModelOption>> {
    let output = crate::adapters::program::harness_command("agent")?
        .arg("models")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let models = parse_agent_models_output(&stdout);
    if models.is_empty() {
        None
    } else {
        Some(models)
    }
}

/// Parse `agent models` text: lines like `id - Label` after an optional header.
pub fn parse_agent_models_output(stdout: &str) -> Vec<SessionModelOption> {
    let mut models = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.eq_ignore_ascii_case("Available models") {
            continue;
        }
        let Some((id, label)) = line.split_once(" - ") else {
            continue;
        };
        let id = id.trim();
        let label = label.trim();
        if id.is_empty() || label.is_empty() {
            continue;
        }
        if id.chars().any(|c| c.is_whitespace() || c.is_control()) {
            continue;
        }
        models.push(SessionModelOption {
            id: id.to_string(),
            label: label.to_string(),
            efforts: None,
            has_fast: None,
        });
    }
    if !models.iter().any(|m| m.id == "auto") {
        models.insert(
            0,
            SessionModelOption {
                id: "auto".to_string(),
                label: "Auto".to_string(),
                efforts: None,
                has_fast: None,
            },
        );
    }
    models
}

const CURSOR_EFFORT_RANK: [&str; 6] = ["xhigh", "high", "medium", "low", "none", "max"];

fn strip_fast_label(label: &str) -> String {
    label
        .trim_end_matches(" Fast")
        .trim_end_matches(" fast")
        .trim()
        .to_string()
}

fn effort_rank(effort: &str) -> usize {
    CURSOR_EFFORT_RANK
        .iter()
        .position(|candidate| *candidate == effort)
        .unwrap_or(CURSOR_EFFORT_RANK.len())
}

/// Collapse exploded Cursor `agent models` ids into unique bases with axis metadata.
pub fn collapse_cursor_catalog(models: Vec<SessionModelOption>) -> Vec<SessionModelOption> {
    let mut auto = Vec::new();
    let mut order: Vec<String> = Vec::new();
    let mut grouped: HashMap<String, (String, Vec<String>, bool)> = HashMap::new();

    for model in models {
        if model.id == "auto" {
            auto.push(model);
            continue;
        }
        let Some(intent) = parse_cursor_model_intent(&model.id) else {
            if !grouped.contains_key(&model.id) {
                order.push(model.id.clone());
                grouped.insert(model.id.clone(), (model.label.clone(), Vec::new(), false));
            }
            continue;
        };
        if !grouped.contains_key(&intent.base) {
            order.push(intent.base.clone());
            grouped.insert(
                intent.base.clone(),
                (strip_fast_label(&model.label), Vec::new(), false),
            );
        }
        let entry = grouped.get_mut(&intent.base).expect("base inserted above");
        if !intent.fast.unwrap_or(false) {
            entry.0 = strip_fast_label(&model.label);
        }
        if let Some(effort) = intent.effort {
            if !entry.1.iter().any(|existing| existing == &effort) {
                entry.1.push(effort);
            }
        }
        if intent.fast.unwrap_or(false) {
            entry.2 = true;
        }
    }

    let mut collapsed = auto;
    for base in order {
        let (label, mut efforts, has_fast) = grouped.remove(&base).expect("base tracked in order");
        efforts.sort_by_key(|effort| effort_rank(effort));
        collapsed.push(SessionModelOption {
            id: base,
            label,
            efforts: (!efforts.is_empty()).then_some(efforts),
            has_fast: has_fast.then_some(true),
        });
    }
    collapsed
}

#[cfg(test)]
mod tests {
    use super::*;

    // The catalog is re-read when the harness changes, not on a timer: a second
    // call at the same version must not pay for another handshake.
    #[test]
    fn catalog_is_cached_against_the_harness_version() {
        let first = list_session_models("cursor");
        let version = first.harness_version.clone();
        let second = list_session_models("cursor");

        assert_eq!(second.harness_version, version);
        assert_eq!(second.models, first.models);
        if !version.is_empty() {
            assert!(
                cached_versions()
                    .iter()
                    .any(|(agent, cached)| agent == "cursor" && cached == &version),
                "the cursor catalog should be stored under its harness version"
            );
        }
    }

    // A harness that cannot be reached must be retried, not remembered: the
    // fix is usually an install, and the version it would key on never changes.
    #[test]
    fn a_failed_catalog_read_is_not_cached() {
        let failed = list_session_models("other");
        assert!(failed.error.is_some() || failed.models.is_empty());
        assert!(
            !cached_versions().iter().any(|(agent, _)| agent == "other"),
            "an unreachable harness must not be cached"
        );
    }

    #[test]
    fn harness_version_is_empty_for_an_agent_ajax_cannot_ask() {
        assert!(harness_version(AgentClient::Other).is_empty());
    }

    #[test]
    fn parse_agent_models_output_reads_id_label_lines() {
        let models = parse_agent_models_output(
            "Available models\n\nauto - Auto (default)\ncomposer-2.5 - Composer 2.5\n",
        );
        assert_eq!(
            models,
            vec![
                SessionModelOption {
                    id: "auto".to_string(),
                    label: "Auto (default)".to_string(),
                    efforts: None,
                    has_fast: None,
                },
                SessionModelOption {
                    id: "composer-2.5".to_string(),
                    label: "Composer 2.5".to_string(),
                    efforts: None,
                    has_fast: None,
                },
            ]
        );
    }

    #[test]
    fn collapse_cursor_catalog_emits_unique_bases_with_axes() {
        let collapsed = collapse_cursor_catalog(vec![
            SessionModelOption {
                id: "auto".to_string(),
                label: "Auto".to_string(),
                efforts: None,
                has_fast: None,
            },
            SessionModelOption {
                id: "composer-2.5".to_string(),
                label: "Composer 2.5".to_string(),
                efforts: None,
                has_fast: None,
            },
            SessionModelOption {
                id: "composer-2.5-fast".to_string(),
                label: "Composer 2.5 Fast".to_string(),
                efforts: None,
                has_fast: None,
            },
            SessionModelOption {
                id: "cursor-grok-4.6-high".to_string(),
                label: "Grok 4.6".to_string(),
                efforts: None,
                has_fast: None,
            },
            SessionModelOption {
                id: "cursor-grok-4.6-high-fast".to_string(),
                label: "Grok 4.6 Fast".to_string(),
                efforts: None,
                has_fast: None,
            },
            SessionModelOption {
                id: "gpt-5.6-sol-medium".to_string(),
                label: "GPT 5.6".to_string(),
                efforts: None,
                has_fast: None,
            },
            SessionModelOption {
                id: "gpt-5.6-sol-high".to_string(),
                label: "GPT 5.6 High".to_string(),
                efforts: None,
                has_fast: None,
            },
        ]);
        assert_eq!(collapsed.len(), 4);
        assert_eq!(collapsed[0].id, "auto");
        let composer = collapsed.iter().find(|m| m.id == "composer-2.5").unwrap();
        assert_eq!(composer.has_fast, Some(true));
        assert_eq!(composer.efforts, None);
        let grok = collapsed.iter().find(|m| m.id == "grok-4.6").unwrap();
        assert_eq!(
            grok.efforts.as_deref(),
            Some(["high".to_string()].as_slice())
        );
        assert_eq!(grok.has_fast, Some(true));
        let sol = collapsed.iter().find(|m| m.id == "gpt-5.6-sol").unwrap();
        assert_eq!(
            sol.efforts.as_deref(),
            Some(["high".to_string(), "medium".to_string()].as_slice())
        );
    }

    #[test]
    fn collapse_cursor_catalog_collects_grok_effort_levels_issue_1004() {
        let collapsed = collapse_cursor_catalog(vec![
            SessionModelOption {
                id: "cursor-grok-4.6-low".to_string(),
                label: "Grok 4.6".to_string(),
                efforts: None,
                has_fast: None,
            },
            SessionModelOption {
                id: "cursor-grok-4.6-medium".to_string(),
                label: "Grok 4.6".to_string(),
                efforts: None,
                has_fast: None,
            },
            SessionModelOption {
                id: "cursor-grok-4.6-high".to_string(),
                label: "Grok 4.6".to_string(),
                efforts: None,
                has_fast: None,
            },
            SessionModelOption {
                id: "cursor-grok-4.6-xhigh".to_string(),
                label: "Grok 4.6".to_string(),
                efforts: None,
                has_fast: None,
            },
            SessionModelOption {
                id: "cursor-grok-4.6-high-fast".to_string(),
                label: "Grok 4.6 Fast".to_string(),
                efforts: None,
                has_fast: None,
            },
        ]);
        let grok = collapsed
            .iter()
            .find(|model| model.id == "grok-4.6")
            .unwrap();
        assert_eq!(
            grok.efforts.as_deref(),
            Some(
                ["xhigh", "high", "medium", "low"]
                    .map(String::from)
                    .as_slice()
            )
        );
        assert_eq!(grok.has_fast, Some(true));
    }

    #[test]
    fn collapse_cursor_catalog_keeps_thinking_bases_distinct_issue_1004() {
        let collapsed = collapse_cursor_catalog(vec![
            SessionModelOption {
                id: "claude-opus-5-high".to_string(),
                label: "Claude Opus 5 High".to_string(),
                efforts: None,
                has_fast: None,
            },
            SessionModelOption {
                id: "claude-opus-5-thinking-high".to_string(),
                label: "Claude Opus 5 Thinking High".to_string(),
                efforts: None,
                has_fast: None,
            },
        ]);
        let bases: Vec<_> = collapsed.iter().map(|model| model.id.as_str()).collect();
        assert_eq!(bases, ["claude-opus-5", "claude-opus-5-thinking"]);
    }

    #[test]
    fn collapse_cursor_catalog_preserves_first_seen_base_order() {
        let collapsed = collapse_cursor_catalog(vec![
            SessionModelOption {
                id: "auto".to_string(),
                label: "Auto".to_string(),
                efforts: None,
                has_fast: None,
            },
            SessionModelOption {
                id: "cursor-grok-4.6-high".to_string(),
                label: "Grok 4.6".to_string(),
                efforts: None,
                has_fast: None,
            },
            SessionModelOption {
                id: "gpt-5.6-sol-medium".to_string(),
                label: "GPT 5.6".to_string(),
                efforts: None,
                has_fast: None,
            },
        ]);
        let base_ids: Vec<_> = collapsed
            .iter()
            .filter(|model| model.id != "auto")
            .map(|model| model.id.as_str())
            .collect();
        assert_eq!(base_ids, ["grok-4.6", "gpt-5.6-sol"]);
    }

    #[test]
    fn cursor_catalog_keeps_exploded_agent_model_ids() {
        let raw = vec![
            SessionModelOption {
                id: "auto".to_string(),
                label: "Auto".to_string(),
                efforts: None,
                has_fast: None,
            },
            SessionModelOption {
                id: "cursor-grok-4.6-high".to_string(),
                label: "Grok 4.6".to_string(),
                efforts: None,
                has_fast: None,
            },
            SessionModelOption {
                id: "cursor-grok-4.6-high-fast".to_string(),
                label: "Grok 4.6 Fast".to_string(),
                efforts: None,
                has_fast: None,
            },
        ];
        let collapsed = collapse_cursor_catalog(raw.clone());
        assert_eq!(collapsed.len(), 2);
        assert_eq!(raw.len(), 3);
        assert!(raw
            .iter()
            .any(|model| model.id == "cursor-grok-4.6-high-fast"));
    }

    #[test]
    fn parse_agent_models_output_injects_auto_when_missing() {
        let models = parse_agent_models_output("composer-2.5 - Composer 2.5\n");
        assert_eq!(models[0].id, "auto");
        assert_eq!(models[1].id, "composer-2.5");
    }
}
