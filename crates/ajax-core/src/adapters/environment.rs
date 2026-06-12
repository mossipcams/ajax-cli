use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime},
};

pub const REQUIRED_DOCTOR_TOOLS: [&str; 3] = ["git", "tmux", "codex"];

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DoctorEnvironment {
    available_tools: BTreeSet<String>,
    existing_paths: Option<BTreeSet<PathBuf>>,
    graphify_out_gitignored: Option<BTreeSet<PathBuf>>,
}

impl DoctorEnvironment {
    pub fn from_available_tools<I, T>(tools: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        Self {
            available_tools: tools.into_iter().map(Into::into).collect(),
            existing_paths: None,
            graphify_out_gitignored: None,
        }
    }

    pub fn from_path() -> Self {
        let Some(path) = std::env::var_os("PATH") else {
            return Self::default();
        };
        let available_tools = REQUIRED_DOCTOR_TOOLS
            .iter()
            .copied()
            .filter(|tool| {
                std::env::split_paths(&path).any(|directory| directory.join(tool).is_file())
            })
            .map(str::to_string)
            .collect();

        Self {
            available_tools,
            existing_paths: None,
            graphify_out_gitignored: None,
        }
    }

    pub fn with_graphify_out_gitignored<I, T>(mut self, repo_paths: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<PathBuf>,
    {
        self.graphify_out_gitignored = Some(repo_paths.into_iter().map(Into::into).collect());
        self
    }

    pub fn with_existing_paths<I, T>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<PathBuf>,
    {
        self.existing_paths = Some(paths.into_iter().map(Into::into).collect());
        self
    }

    pub(crate) fn has_tool(&self, tool: &str) -> bool {
        self.available_tools.contains(tool)
    }

    pub(crate) fn path_exists(&self, path: &Path) -> bool {
        self.existing_paths
            .as_ref()
            .map_or_else(|| path.exists(), |paths| paths.contains(path))
    }

    pub(crate) fn graphify_out_gitignored(&self, repo_path: &Path) -> bool {
        if let Some(repo_paths) = &self.graphify_out_gitignored {
            return repo_paths.contains(repo_path);
        }

        Command::new("git")
            .args([
                "-C",
                repo_path.to_str().unwrap_or_default(),
                "check-ignore",
                "-q",
                "graphify-out",
            ])
            .output()
            .is_ok_and(|output| output.status.success())
    }
}

pub fn origin_fetch_age(repo_path: impl AsRef<Path>) -> Option<Duration> {
    let fetch_head = repo_path.as_ref().join(".git/FETCH_HEAD");
    let metadata = std::fs::metadata(fetch_head).ok()?;
    let modified = metadata.modified().ok()?;
    SystemTime::now().duration_since(modified).ok()
}

#[cfg(test)]
mod tests {
    use super::origin_fetch_age;
    use std::{
        fs,
        io::Write,
        time::{Duration, SystemTime},
    };

    #[test]
    fn origin_fetch_age_reads_fetch_head_mtime() {
        let root = std::env::temp_dir().join(format!(
            "ajax-origin-fetch-age-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join(".git")).unwrap();
        let mut file = fs::File::create(root.join(".git/FETCH_HEAD")).unwrap();
        writeln!(file, "ref: origin/main").unwrap();

        let age = origin_fetch_age(&root).expect("expected fetch head age");

        assert!(age < Duration::from_secs(5), "unexpected age: {age:?}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn origin_fetch_age_is_none_without_fetch_head() {
        let root = std::env::temp_dir().join(format!(
            "ajax-origin-fetch-age-missing-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join(".git")).unwrap();

        assert_eq!(origin_fetch_age(&root), None);

        let _ = fs::remove_dir_all(root);
    }
}
