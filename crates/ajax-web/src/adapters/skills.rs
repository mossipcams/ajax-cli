//! Resolve Cursor/Codex skill paths on the companion host.

use std::path::{Path, PathBuf};

pub fn resolve_skill_path(skill_name: &str) -> Option<PathBuf> {
    let home = home_dir()?;
    for base in skill_search_roots(&home) {
        let path = base.join(skill_name).join("SKILL.md");
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub(crate) fn skill_search_roots(home: &Path) -> Vec<PathBuf> {
    let mut roots = vec![
        home.join(".codex").join("skills"),
        home.join(".cursor").join("skills-cursor"),
    ];
    if let Ok(extra) = std::env::var("AJAX_SKILL_ROOT") {
        if !extra.is_empty() {
            roots.insert(0, PathBuf::from(extra));
        }
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::skill_search_roots;
    use std::path::Path;

    #[test]
    fn skill_search_roots_include_codex_and_cursor_defaults() {
        let home = Path::new("/home/operator");
        let roots = skill_search_roots(home);

        assert!(roots
            .iter()
            .any(|root| { root.ends_with(".codex/skills") || root.ends_with("skills") }));
    }
}
