use crate::{
    adapters::{CommandRunError, CommandRunner, CommandSpec, GithubChecksAdapter},
    commands::{CommandContext, CommandError},
    diff_review::{remember_pull_requests, stored_pull_requests, PullRequestRef, PullRequestState},
    registry::Registry,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergePrPlan {
    pub pull_request: PullRequestRef,
    pub command: CommandSpec,
}

pub fn plan_merge_pr_operation<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<MergePrPlan, CommandError> {
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or_else(|| CommandError::TaskNotFound(qualified_handle.to_string()))?;
    let pull_request = stored_pull_requests(task)
        .into_iter()
        .filter(|pr| pr.state == PullRequestState::Open)
        .max_by_key(|pr| pr.number)
        .ok_or_else(|| {
            CommandError::PlanBlocked(vec![format!("no open pull request for {qualified_handle}")])
        })?;
    let worktree = task.worktree_path.display().to_string();
    let command = GithubChecksAdapter::new("gh").pr_merge(&worktree, pull_request.number);
    Ok(MergePrPlan {
        pull_request,
        command,
    })
}

pub fn execute_merge_pr_operation<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    plan: &MergePrPlan,
    runner: &mut impl CommandRunner,
) -> Result<(), CommandError> {
    let output = runner
        .run(&plan.command)
        .map_err(CommandError::CommandRun)?;
    if output.status_code != 0 {
        return Err(CommandError::CommandRun(CommandRunError::NonZeroExit {
            program: plan.command.program.clone(),
            status_code: output.status_code,
            stderr: output.stderr,
            cwd: plan.command.cwd.clone(),
        }));
    }
    record_pull_request_merged(context, qualified_handle, &plan.pull_request)
}

fn record_pull_request_merged<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    pull_request: &PullRequestRef,
) -> Result<(), CommandError> {
    let task_id = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .map(|task| task.id.clone())
        .ok_or_else(|| CommandError::TaskNotFound(qualified_handle.to_string()))?;
    let task = context
        .registry
        .get_task_mut(&task_id)
        .ok_or_else(|| CommandError::TaskNotFound(qualified_handle.to_string()))?;
    let merged = PullRequestRef {
        state: PullRequestState::Merged,
        ..pull_request.clone()
    };
    remember_pull_requests(task, std::slice::from_ref(&merged));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{execute_merge_pr_operation, plan_merge_pr_operation};
    use crate::{
        adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
        commands::{CommandContext, CommandError},
        config::{Config, ManagedRepo},
        diff_review::{
            remember_pull_requests, stored_pull_requests, PullRequestRef, PullRequestState,
        },
        models::{AgentClient, Task, TaskId},
        registry::{InMemoryRegistry, Registry},
    };

    #[derive(Default)]
    struct FakeRunner {
        result: Option<Result<CommandOutput, CommandRunError>>,
        commands: Vec<CommandSpec>,
    }

    impl FakeRunner {
        fn answering(result: Result<CommandOutput, CommandRunError>) -> Self {
            Self {
                result: Some(result),
                commands: Vec::new(),
            }
        }
    }

    impl CommandRunner for FakeRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            self.result.take().unwrap_or(Ok(CommandOutput {
                status_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            }))
        }
    }

    fn pull_request(number: u64, state: PullRequestState) -> PullRequestRef {
        PullRequestRef {
            number,
            title: "Fix login".to_string(),
            url: format!("https://example.com/pull/{number}"),
            state,
            head_ref: "ajax/fix-login".to_string(),
            head_sha: Some("abc".to_string()),
        }
    }

    fn context_with_pull_requests(prs: &[PullRequestRef]) -> CommandContext<InMemoryRegistry> {
        let mut context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let mut task = Task::new(
            TaskId::new("web/fix-login"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/repo/web__worktrees/ajax-fix-login",
            "ajax-web-fix-login",
            "task",
            AgentClient::Codex,
        );
        remember_pull_requests(&mut task, prs);
        context.registry.create_task(task).unwrap();
        context
    }

    fn stored(context: &CommandContext<InMemoryRegistry>) -> Vec<PullRequestRef> {
        let task = context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap();
        stored_pull_requests(task)
    }

    #[test]
    fn merge_without_open_pull_request_is_refused() {
        let context = context_with_pull_requests(&[pull_request(10, PullRequestState::Merged)]);

        let error = plan_merge_pr_operation(&context, "web/fix-login").unwrap_err();

        match error {
            CommandError::PlanBlocked(reasons) => assert_eq!(
                reasons,
                vec!["no open pull request for web/fix-login".to_string()]
            ),
            other => panic!("expected PlanBlocked, got {other:?}"),
        }
    }

    #[test]
    fn merge_with_open_pull_request_runs_squash_merge_and_marks_merged() {
        let mut context = context_with_pull_requests(&[pull_request(12, PullRequestState::Open)]);
        let plan = plan_merge_pr_operation(&context, "web/fix-login").unwrap();
        let mut runner = FakeRunner::answering(Ok(CommandOutput {
            status_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        }));

        execute_merge_pr_operation(&mut context, "web/fix-login", &plan, &mut runner).unwrap();

        assert_eq!(runner.commands.len(), 1);
        assert_eq!(runner.commands[0].program, "gh");
        assert_eq!(
            runner.commands[0].args,
            ["pr", "merge", "12", "--squash"].map(String::from)
        );
        assert_eq!(
            runner.commands[0].cwd.as_deref(),
            Some(std::path::Path::new("/repo/web__worktrees/ajax-fix-login"))
        );
        assert_eq!(
            stored(&context),
            vec![pull_request(12, PullRequestState::Merged)]
        );
    }

    #[test]
    fn gh_failure_is_an_error_and_leaves_pull_request_open() {
        let mut context = context_with_pull_requests(&[pull_request(12, PullRequestState::Open)]);
        let plan = plan_merge_pr_operation(&context, "web/fix-login").unwrap();
        let mut runner = FakeRunner::answering(Ok(CommandOutput {
            status_code: 1,
            stdout: String::new(),
            stderr: "Pull request is not mergeable".to_string(),
        }));

        let error = execute_merge_pr_operation(&mut context, "web/fix-login", &plan, &mut runner)
            .unwrap_err();

        assert!(matches!(
            error,
            CommandError::CommandRun(CommandRunError::NonZeroExit { status_code: 1, .. })
        ));
        assert_eq!(
            stored(&context),
            vec![pull_request(12, PullRequestState::Open)]
        );
    }
}
