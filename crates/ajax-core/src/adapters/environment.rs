use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub const REQUIRED_DOCTOR_TOOLS: [&str; 3] = ["git", "tmux", "codex"];

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DoctorEnvironment {
    available_tools: BTreeSet<String>,
    existing_paths: Option<BTreeSet<PathBuf>>,
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
        }
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
}
