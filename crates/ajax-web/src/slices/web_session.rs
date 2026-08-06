//! Ajax Web Session wire types, task worktree preparation, and symbol search.

use ajax_core::{commands::CommandContext, models::AgentClient, registry::Registry};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const WEB_SESSION_PROTOCOL_VERSION: u32 = 2;
pub const SYMBOL_SEARCH_MAX_RESULTS: usize = 30;
pub const WEB_SESSION_PREFERENCE_FILE: &str = "web_session_pref.json";

#[derive(Clone, Debug)]
pub struct WebSessionPreferenceStore {
    path: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
struct WebSessionPreference {
    enabled: bool,
}

impl WebSessionPreferenceStore {
    pub fn new(state_dir: PathBuf) -> Self {
        Self {
            path: state_dir.join(WEB_SESSION_PREFERENCE_FILE),
        }
    }

    pub fn enabled(&self) -> bool {
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|contents| serde_json::from_str::<WebSessionPreference>(&contents).ok())
            .is_some_and(|preference| preference.enabled)
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create web session state: {error}"))?;
        }
        let contents = serde_json::to_string(&WebSessionPreference { enabled })
            .map_err(|error| format!("encode web session preference: {error}"))?;
        fs::write(&self.path, contents)
            .map_err(|error| format!("write web session preference: {error}"))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebSessionPlan {
    pub qualified_handle: String,
    pub worktree_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebSessionRouteError {
    TaskNotFound,
    WorktreeMissing,
    AgentNotSupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebSessionStatus {
    Running,
    Waiting,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttentionKind {
    Permission,
    Question,
    Failed,
    Review,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AttentionResponse {
    #[serde(rename = "permission")]
    Permission {
        #[serde(rename = "outcome")]
        outcome: PermissionOutcome,
    },
    #[serde(rename = "question")]
    Question { text: String },
    #[serde(rename = "failed")]
    Failed { action: FailedAttentionAction },
    #[serde(rename = "review")]
    Review { action: ReviewAttentionAction },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionOutcome {
    AllowOnce,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FailedAttentionAction {
    Stop,
    Retry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewAttentionAction {
    Open,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WebSessionClientMessage {
    #[serde(rename = "session.prompt")]
    Prompt { version: u32, message: String },
    #[serde(rename = "session.abort")]
    Abort { version: u32 },
    #[serde(rename = "attention.respond")]
    AttentionRespond {
        version: u32,
        #[serde(rename = "targetHandle")]
        target_handle: String,
        #[serde(rename = "requestId")]
        request_id: String,
        response: AttentionResponse,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WebSessionServerEvent {
    #[serde(rename = "session.ready")]
    Ready {
        version: u32,
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    #[serde(rename = "session.status")]
    Status {
        version: u32,
        state: WebSessionStatus,
    },
    #[serde(rename = "session.assistant_delta")]
    AssistantDelta { version: u32, text: String },
    #[serde(rename = "session.progress")]
    Progress {
        version: u32,
        kind: String,
        #[serde(rename = "toolName", skip_serializing_if = "Option::is_none")]
        tool_name: Option<String>,
        status: String,
        summary: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
    #[serde(rename = "session.settled")]
    Settled { version: u32 },
    #[serde(rename = "session.error")]
    Error {
        version: u32,
        code: String,
        message: String,
    },
    #[serde(rename = "session.closed")]
    Closed { version: u32 },
    #[serde(rename = "attention.required")]
    AttentionRequired {
        version: u32,
        handle: String,
        #[serde(rename = "requestId")]
        request_id: String,
        kind: AttentionKind,
        title: String,
        summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        options: Option<Vec<String>>,
    },
    #[serde(rename = "attention.cleared")]
    AttentionCleared {
        version: u32,
        handle: String,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "attention.error")]
    AttentionError {
        version: u32,
        handle: String,
        #[serde(rename = "requestId")]
        request_id: String,
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebSymbolKind {
    Function,
    Method,
    Struct,
    Class,
    #[serde(rename = "type")]
    Type,
    Interface,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebSymbolHit {
    pub id: String,
    pub name: String,
    pub kind: WebSymbolKind,
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub preview: String,
    pub source: String,
}

pub fn prepare_web_session<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<WebSessionPlan, WebSessionRouteError> {
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or(WebSessionRouteError::TaskNotFound)?;

    if task.worktree_path.as_os_str().is_empty() || !task.worktree_path.is_dir() {
        return Err(WebSessionRouteError::WorktreeMissing);
    }

    if task.selected_agent != AgentClient::Cursor {
        return Err(WebSessionRouteError::AgentNotSupported);
    }

    Ok(WebSessionPlan {
        qualified_handle: qualified_handle.to_string(),
        worktree_path: task.worktree_path.clone(),
    })
}

pub fn search_worktree_symbols(worktree: &Path, query: &str) -> Vec<WebSymbolHit> {
    let needle = query.trim();
    if needle.is_empty() {
        return Vec::new();
    }

    let mut hits = if rg_available() {
        search_with_rg(worktree, needle)
    } else {
        search_with_walk(worktree, needle)
    };
    dedupe_and_cap(&mut hits);
    hits
}

fn rg_available() -> bool {
    Command::new("rg")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn search_with_rg(worktree: &Path, needle: &str) -> Vec<WebSymbolHit> {
    let output = Command::new("rg")
        .arg("-n")
        .arg("--no-heading")
        .arg("--max-count")
        .arg("120")
        .arg("--glob")
        .arg("!.git/**")
        .arg("--glob")
        .arg("!target/**")
        .arg("--glob")
        .arg("!node_modules/**")
        .arg("-i")
        .arg("-F")
        .arg(needle)
        .arg(worktree)
        .output();

    let Ok(output) = output else {
        return search_with_walk(worktree, needle);
    };
    if !output.status.success() && output.status.code() != Some(1) {
        return search_with_walk(worktree, needle);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut hits = Vec::new();
    for line in stdout.lines() {
        if let Some(hit) = parse_rg_line(worktree, line) {
            hits.push(hit);
        }
    }
    hits
}

fn parse_rg_line(worktree: &Path, line: &str) -> Option<WebSymbolHit> {
    let (path_part, rest) = line.split_once(':')?;
    let (line_no, content) = rest.split_once(':')?;
    let line_number: u32 = line_no.parse().ok()?;
    let absolute = PathBuf::from(path_part);
    symbol_hit_from_line(worktree, &absolute, line_number, content)
}

fn search_with_walk(worktree: &Path, needle: &str) -> Vec<WebSymbolHit> {
    let needle_lower = needle.to_ascii_lowercase();
    let mut hits = Vec::new();
    walk_for_symbols(worktree, worktree, &needle_lower, &mut hits);
    hits
}

fn walk_for_symbols(worktree: &Path, dir: &Path, needle_lower: &str, hits: &mut Vec<WebSymbolHit>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if matches!(name.as_ref(), "target" | "node_modules" | ".git") {
                continue;
            }
            walk_for_symbols(worktree, &path, needle_lower, hits);
            continue;
        }
        if !is_searchable_file(&path) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        if path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.to_ascii_lowercase().contains(needle_lower))
        {
            if let Some(hit) = file_symbol_hit(worktree, &path, &content) {
                hits.push(hit);
            }
        }
        for (index, line) in content.lines().enumerate() {
            if !line.to_ascii_lowercase().contains(needle_lower) {
                continue;
            }
            if let Some(hit) = symbol_hit_from_line(worktree, &path, (index as u32) + 1, line) {
                hits.push(hit);
            }
        }
    }
}

fn is_searchable_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java" | "css" | "md")
    )
}

fn file_symbol_hit(worktree: &Path, path: &Path, content: &str) -> Option<WebSymbolHit> {
    let relative = relative_worktree_path(worktree, path)?;
    let file_name = path.file_name()?.to_str()?.to_string();
    let line_count = content.lines().count().max(1) as u32;
    let source = truncate_source(content, 80);
    Some(WebSymbolHit {
        id: symbol_id(&relative, 1, &file_name),
        name: file_name.clone(),
        kind: WebSymbolKind::File,
        path: relative,
        start_line: 1,
        end_line: line_count,
        preview: format!("file {file_name}"),
        source,
    })
}

fn symbol_hit_from_line(
    worktree: &Path,
    path: &Path,
    line_number: u32,
    line: &str,
) -> Option<WebSymbolHit> {
    let relative = relative_worktree_path(worktree, path)?;
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (kind, name) = classify_symbol_line(trimmed)?;
    let (start_line, end_line, source) = extract_symbol_source(path, line_number, trimmed)?;
    let preview = trimmed.chars().take(120).collect::<String>();
    Some(WebSymbolHit {
        id: symbol_id(&relative, start_line, &name),
        name,
        kind,
        path: relative,
        start_line,
        end_line,
        preview,
        source,
    })
}

fn relative_worktree_path(worktree: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(worktree)
        .ok()
        .map(|value| value.to_string_lossy().replace('\\', "/"))
}

fn symbol_id(path: &str, line: u32, name: &str) -> String {
    format!("{path}:{line}:{name}")
}

fn classify_symbol_line(line: &str) -> Option<(WebSymbolKind, String)> {
    for (prefix, kind) in [
        ("pub async fn ", WebSymbolKind::Function),
        ("async fn ", WebSymbolKind::Function),
        ("pub fn ", WebSymbolKind::Function),
        ("fn ", WebSymbolKind::Function),
        ("pub struct ", WebSymbolKind::Struct),
        ("struct ", WebSymbolKind::Struct),
        ("pub enum ", WebSymbolKind::Type),
        ("enum ", WebSymbolKind::Type),
        ("pub trait ", WebSymbolKind::Interface),
        ("trait ", WebSymbolKind::Interface),
        ("export interface ", WebSymbolKind::Interface),
        ("interface ", WebSymbolKind::Interface),
        ("export type ", WebSymbolKind::Type),
        ("type ", WebSymbolKind::Type),
        ("export class ", WebSymbolKind::Class),
        ("class ", WebSymbolKind::Class),
        ("export function ", WebSymbolKind::Function),
        ("function ", WebSymbolKind::Function),
        ("def ", WebSymbolKind::Function),
    ] {
        if let Some(rest) = line.trim_start().strip_prefix(prefix) {
            let name = take_identifier(rest)?;
            let kind = if matches!(kind, WebSymbolKind::Function)
                && (line.contains("&self") || line.contains("self,") || line.contains("(self"))
            {
                WebSymbolKind::Method
            } else {
                kind
            };
            return Some((kind, name));
        }
    }
    None
}

fn take_identifier(input: &str) -> Option<String> {
    let trimmed = input.trim_start();
    let mut end = 0;
    for (index, ch) in trimmed.char_indices() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            end = index + ch.len_utf8();
            continue;
        }
        break;
    }
    if end == 0 {
        return None;
    }
    Some(trimmed[..end].to_string())
}

fn extract_symbol_source(
    path: &Path,
    line_number: u32,
    signature_line: &str,
) -> Option<(u32, u32, String)> {
    let content = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    if line_number == 0 || line_number as usize > lines.len() {
        return None;
    }
    let start_idx = (line_number - 1) as usize;
    let mut end_idx = start_idx;
    if signature_line.contains('{') {
        let mut depth = 0usize;
        for (offset, line) in lines.iter().enumerate().skip(start_idx).take(120) {
            for ch in line.chars() {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth = depth.saturating_sub(1);
                }
            }
            end_idx = offset;
            if depth == 0 && offset > start_idx {
                break;
            }
        }
    } else {
        end_idx = (start_idx + 40).min(lines.len().saturating_sub(1));
    }
    let source = lines[start_idx..=end_idx].join("\n");
    Some((
        start_idx as u32 + 1,
        end_idx as u32 + 1,
        truncate_source(&source, 80),
    ))
}

fn truncate_source(source: &str, max_lines: usize) -> String {
    let mut lines = source.lines();
    let truncated: Vec<&str> = lines.by_ref().take(max_lines).collect();
    let mut out = truncated.join("\n");
    if lines.next().is_some() {
        out.push_str("\n…");
    }
    out
}

fn dedupe_and_cap(hits: &mut Vec<WebSymbolHit>) {
    let mut seen = HashSet::new();
    hits.retain(|hit| seen.insert(hit.id.clone()));
    hits.truncate(SYMBOL_SEARCH_MAX_RESULTS);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;
    use std::fs;

    #[test]
    fn client_and_server_wire_types_round_trip() {
        let client = WebSessionClientMessage::Prompt {
            version: WEB_SESSION_PROTOCOL_VERSION,
            message: "hello".to_string(),
        };
        let client_json = serde_json::to_value(&client).expect("serialize client");
        assert_eq!(client_json["type"], "session.prompt");
        assert_eq!(client_json["message"], "hello");

        let events = [
            WebSessionServerEvent::Ready {
                version: WEB_SESSION_PROTOCOL_VERSION,
                session_id: "web/fix-login-1".to_string(),
            },
            WebSessionServerEvent::Status {
                version: WEB_SESSION_PROTOCOL_VERSION,
                state: WebSessionStatus::Running,
            },
            WebSessionServerEvent::AssistantDelta {
                version: WEB_SESSION_PROTOCOL_VERSION,
                text: "delta".to_string(),
            },
            WebSessionServerEvent::Settled {
                version: WEB_SESSION_PROTOCOL_VERSION,
            },
            WebSessionServerEvent::Error {
                version: WEB_SESSION_PROTOCOL_VERSION,
                code: "provider_error".to_string(),
                message: "boom".to_string(),
            },
            WebSessionServerEvent::Closed {
                version: WEB_SESSION_PROTOCOL_VERSION,
            },
            WebSessionServerEvent::AttentionRequired {
                version: WEB_SESSION_PROTOCOL_VERSION,
                handle: "web/other".to_string(),
                request_id: "7".to_string(),
                kind: AttentionKind::Permission,
                title: "Permission needed".to_string(),
                summary: "Permission: Run tests".to_string(),
                options: Some(vec!["allow-once".to_string(), "reject".to_string()]),
            },
            WebSessionServerEvent::AttentionCleared {
                version: WEB_SESSION_PROTOCOL_VERSION,
                handle: "web/other".to_string(),
                request_id: "7".to_string(),
            },
        ];
        for event in events {
            let encoded = serde_json::to_vec(&event).expect("serialize");
            let decoded: WebSessionServerEvent =
                serde_json::from_slice(&encoded).expect("deserialize");
            assert_eq!(decoded, event);
        }

        let respond = WebSessionClientMessage::AttentionRespond {
            version: WEB_SESSION_PROTOCOL_VERSION,
            target_handle: "web/other".to_string(),
            request_id: "7".to_string(),
            response: AttentionResponse::Permission {
                outcome: PermissionOutcome::AllowOnce,
            },
        };
        let respond_json = serde_json::to_value(&respond).expect("serialize respond");
        assert_eq!(respond_json["type"], "attention.respond");
        assert_eq!(respond_json["targetHandle"], "web/other");
    }

    #[test]
    fn prepare_web_session_returns_worktree_for_existing_task() {
        let temp =
            std::env::temp_dir().join(format!("ajax-web-session-worktree-{}", std::process::id()));
        fs::create_dir_all(&temp).expect("worktree dir");
        let mut task = test_support::fix_login_task();
        task.worktree_path = temp.clone();
        task.selected_agent = AgentClient::Cursor;
        let context = test_support::context_with_tasks(&["web"], vec![task]);

        let plan = prepare_web_session(&context, "web/fix-login").expect("plan");

        assert_eq!(plan.qualified_handle, "web/fix-login");
        assert_eq!(plan.worktree_path, temp);
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn prepare_web_session_returns_task_not_found() {
        let context = test_support::context_with_fix_login_task();
        assert_eq!(
            prepare_web_session(&context, "web/missing").unwrap_err(),
            WebSessionRouteError::TaskNotFound
        );
    }

    #[test]
    fn prepare_web_session_returns_worktree_missing_for_empty_or_absent_path() {
        let mut empty = test_support::fix_login_task();
        empty.worktree_path = PathBuf::new();
        let empty_context = test_support::context_with_tasks(&["web"], vec![empty]);
        assert_eq!(
            prepare_web_session(&empty_context, "web/fix-login").unwrap_err(),
            WebSessionRouteError::WorktreeMissing
        );

        let mut missing = test_support::fix_login_task();
        missing.worktree_path = PathBuf::from("/definitely/missing/ajax-worktree");
        let missing_context = test_support::context_with_tasks(&["web"], vec![missing]);
        assert_eq!(
            prepare_web_session(&missing_context, "web/fix-login").unwrap_err(),
            WebSessionRouteError::WorktreeMissing
        );
    }

    #[test]
    fn prepare_web_session_returns_agent_not_supported_for_non_cursor() {
        let temp = std::env::temp_dir().join(format!(
            "ajax-web-session-agent-gate-{}",
            std::process::id()
        ));
        fs::create_dir_all(&temp).expect("worktree dir");
        let mut task = test_support::fix_login_task();
        task.worktree_path = temp.clone();
        task.selected_agent = AgentClient::Codex;
        let context = test_support::context_with_tasks(&["web"], vec![task]);

        assert_eq!(
            prepare_web_session(&context, "web/fix-login").unwrap_err(),
            WebSessionRouteError::AgentNotSupported
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn web_session_preference_store_round_trips_under_state_dir() {
        let temp =
            std::env::temp_dir().join(format!("ajax-web-session-pref-{}", std::process::id()));
        let store = WebSessionPreferenceStore::new(temp.clone());

        assert!(!store.enabled());
        store.set_enabled(true).expect("write preference");
        assert!(store.enabled());
        assert_eq!(
            fs::read_to_string(temp.join(WEB_SESSION_PREFERENCE_FILE)).expect("preference file"),
            "{\"enabled\":true}"
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn search_worktree_symbols_finds_declarations_without_rg() {
        let temp =
            std::env::temp_dir().join(format!("ajax-web-session-symbols-{}", std::process::id()));
        fs::create_dir_all(&temp).expect("worktree dir");
        fs::write(
            temp.join("session.rs"),
            "pub struct SessionManager {\n    id: String,\n}\n\nimpl SessionManager {\n    pub fn start_session(&self) -> bool {\n        true\n    }\n}\n",
        )
        .expect("write fixture");

        let hits = search_with_walk(&temp, "start_session");
        assert!(
            hits.iter()
                .any(|hit| hit.name == "start_session" && hit.kind == WebSymbolKind::Method),
            "expected method hit, got {hits:?}"
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn classify_symbol_line_detects_common_declarations() {
        assert_eq!(
            classify_symbol_line("pub fn prepare_web_session() {"),
            Some((WebSymbolKind::Function, "prepare_web_session".to_string()))
        );
        assert_eq!(
            classify_symbol_line("export interface WebSymbolHit {"),
            Some((WebSymbolKind::Interface, "WebSymbolHit".to_string()))
        );
    }

    #[test]
    fn search_worktree_symbols_returns_empty_for_blank_query() {
        let temp = std::env::temp_dir().join(format!(
            "ajax-web-session-empty-query-{}",
            std::process::id()
        ));
        fs::create_dir_all(&temp).expect("worktree dir");
        assert!(search_worktree_symbols(&temp, "   ").is_empty());
        let _ = fs::remove_dir_all(temp);
    }
}
