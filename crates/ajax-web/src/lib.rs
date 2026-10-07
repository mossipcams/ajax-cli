#![deny(unsafe_op_in_unsafe_fn)]

pub mod adapters;
pub mod runtime;
pub mod slices;

#[cfg(test)]
mod architecture;

#[cfg(test)]
mod architecture_web_session;

#[cfg(test)]
pub(crate) mod test_support {
    use ajax_core::{
        commands::CommandContext,
        config::{Config, ManagedRepo},
        models::{AgentClient, Task, TaskId},
        registry::{InMemoryRegistry, Registry as _},
    };

    pub(crate) fn fix_login_task() -> Task {
        task_in("web", "fix-login", "Fix login")
    }

    pub(crate) fn task_in(repo: &str, handle: &str, title: &str) -> Task {
        Task::new(
            TaskId::new(format!("{repo}/{handle}")),
            repo,
            handle,
            title,
            format!("ajax/{handle}"),
            "main",
            format!("/repo/{repo}__worktrees/ajax-{handle}"),
            format!("ajax-{repo}-{handle}"),
            "task",
            AgentClient::Codex,
        )
    }

    pub(crate) fn config_with(repos: &[&str]) -> Config {
        Config {
            repos: repos
                .iter()
                .map(|repo| ManagedRepo::new(*repo, format!("/repo/{repo}"), "main"))
                .collect(),
            ..Config::default()
        }
    }

    pub(crate) fn context_with_tasks(
        repos: &[&str],
        tasks: Vec<Task>,
    ) -> CommandContext<InMemoryRegistry> {
        let mut registry = InMemoryRegistry::default();
        for task in tasks {
            registry.create_task(task).unwrap();
        }
        CommandContext::new(config_with(repos), registry)
    }

    pub(crate) fn context_with_fix_login_task() -> CommandContext<InMemoryRegistry> {
        context_with_tasks(&["web"], vec![fix_login_task()])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebError {
    CommandFailed(String),
    JsonSerialization(String),
}

impl std::fmt::Display for WebError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CommandFailed(message) => write!(formatter, "{message}"),
            Self::JsonSerialization(message) => {
                write!(formatter, "json serialization failed: {message}")
            }
        }
    }
}

impl std::error::Error for WebError {}
