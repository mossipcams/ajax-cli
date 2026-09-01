//! Wave 2: live-apply contract — ban new `Task.agent_status` writers outside the
//! Wave 0 inventory. Wave 3 routes `clear_stale_agent_running` through
//! `live::retract_stale_agent_running_at`.

use std::path::{Path, PathBuf};

const CRATES: &[&str] = &["ajax-core", "ajax-web", "ajax-cli", "ajax-supervisor"];

/// Production files that may assign `agent_status` today (append only with plan update).
const ALLOWLIST_SUFFIXES: &[&str] = &[
    "live_application.rs",
    "registry/sqlite/row_codec.rs",
    "models/task.rs",
    "commands/teardown/drop_observation.rs",
];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn production_rust_files(crate_root: &Path) -> Vec<PathBuf> {
    let src = crate_root.join("src");
    if !src.is_dir() {
        return Vec::new();
    }
    let mut files = Vec::new();
    collect_production_rust_files(&src, &mut files);
    files
}

fn collect_production_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "tests") {
                continue;
            }
            collect_production_rust_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") && !is_test_source_path(&path) {
            files.push(path);
        }
    }
}

fn is_test_source_path(path: &Path) -> bool {
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with("_tests.rs") || name == "test_support.rs" || name == "tests.rs"
        })
    {
        return true;
    }
    path.components()
        .any(|component| component.as_os_str() == "tests")
}

fn production_source(source: &str) -> &str {
    const MARKERS: &[&str] = &["#[cfg(test)]\nmod tests", "#[cfg(test)]\npub mod tests"];
    for marker in MARKERS {
        if let Some(idx) = source.rfind(marker) {
            return &source[..idx];
        }
    }
    source
}

fn is_agent_status_assignment(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with("//") {
        return false;
    }
    if trimmed.contains("==") {
        return false;
    }
    if trimmed.contains("excluded.agent_status") {
        return false;
    }
    trimmed.contains(".agent_status =") || trimmed.starts_with("agent_status =")
}

fn path_allowed(path: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/");
    ALLOWLIST_SUFFIXES
        .iter()
        .any(|suffix| normalized.ends_with(suffix))
}

fn relative_path(path: &Path, workspace: &Path) -> String {
    path.strip_prefix(workspace)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn wave2_agent_status_assignments_are_allowlisted() {
    let workspace = workspace_root();
    let mut violations = Vec::new();

    for crate_name in CRATES {
        let crate_root = workspace.join("crates").join(crate_name);
        for path in production_rust_files(&crate_root) {
            if path_allowed(&path) {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            for (line_no, line) in production_source(&source).lines().enumerate() {
                if is_agent_status_assignment(line) {
                    violations.push(format!(
                        "{}:{}: {}",
                        relative_path(&path, &workspace),
                        line_no + 1,
                        line.trim()
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Task.agent_status assignments outside Wave 2 allowlist:\n{}",
        violations.join("\n")
    );
}

#[test]
fn wave2_allowlist_covers_known_inventory_writers() {
    let workspace = workspace_root();
    let core_src = workspace.join("crates/ajax-core/src");

    let expected = [
        (
            "live_application.rs",
            "task.agent_status = AgentRuntimeStatus::Running",
        ),
        (
            "live_application.rs",
            "task.agent_status = AgentRuntimeStatus::Unknown",
        ),
        (
            "registry/sqlite/row_codec.rs",
            "task.agent_status = agent_status",
        ),
        (
            "models/task.rs",
            "self.agent_status = AgentRuntimeStatus::Dead",
        ),
        (
            "commands/teardown/drop_observation.rs",
            "task.agent_status = AgentRuntimeStatus::Dead",
        ),
    ];

    for (file, needle) in expected {
        let path = core_src.join(file);
        let file_source = std::fs::read_to_string(&path).unwrap();
        let source = production_source(&file_source);
        assert!(
            source.contains(needle),
            "inventory writer missing from {file}: expected `{needle}`"
        );
    }
}
