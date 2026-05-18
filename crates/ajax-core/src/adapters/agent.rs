use super::command::CommandSpec;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentLaunch {
    pub worktree_path: String,
    pub prompt: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentAdapter {
    program: String,
}

impl AgentAdapter {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }

    pub fn launch(&self, launch: &AgentLaunch) -> CommandSpec {
        let mut args = vec!["--cd".to_string(), launch.worktree_path.clone()];
        if !launch.prompt.is_empty() {
            args.push(launch.prompt.clone());
        }
        CommandSpec {
            program: self.program.clone(),
            args,
            cwd: None,
            mode: super::command::CommandMode::Capture,
        }
    }
}
