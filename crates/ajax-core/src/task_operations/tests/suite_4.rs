use super::*;

#[test]
fn sweep_cleanup_batches_repo_observations_across_candidates() {
    let mut context = context_with_two_cleanable_tasks();
    let candidates = crate::commands::sweep_cleanup_candidates(&context);
    assert_eq!(candidates.len(), 2);
    let mut runner = RecordingQueuedRunner::new(sweep_success_runner_outputs(&context));

    execute_sweep_cleanup_operation(&mut context, true, &mut runner, None).unwrap();

    let list_sessions = runner
        .commands
        .iter()
        .filter(|command| command.args.first().map(String::as_str) == Some("list-sessions"))
        .count();
    let worktree_lists = runner
        .commands
        .iter()
        .filter(|command| {
            command.program == "git"
                && command.args.iter().any(|arg| arg == "worktree")
                && command.args.iter().any(|arg| arg == "list")
        })
        .count();
    let branch_lists = runner
        .commands
        .iter()
        .filter(|command| {
            command.program == "git"
                && command
                    .args
                    .iter()
                    .any(|arg| arg.contains("--format=%(refname:short)"))
        })
        .count();

    assert_eq!(list_sessions, 1, "shared tmux listing should run once");
    assert_eq!(
        worktree_lists, 1,
        "repo worktree observation should be reused"
    );
    assert_eq!(
        branch_lists, 2,
        "local and remote branch observations should be reused once each"
    );
}

#[test]
fn sweep_cleanup_operation_executes_candidates_and_reports_partial_failure_state() {
    let mut context = context_with_two_cleanable_tasks();
    let candidates = crate::commands::sweep_cleanup_candidates(&context);
    let trash_sweeps = crate::commands::sweep_trash_commands(&context);
    let total_plan_commands: usize = candidates
        .iter()
        .map(|candidate| {
            crate::commands::clean_task_plan(&context, candidate)
                .unwrap()
                .commands
                .len()
        })
        .sum();
    let mut runner = RecordingQueuedRunner::new(sweep_success_runner_outputs(&context));

    let (outputs, state_changed) =
        execute_sweep_cleanup_operation(&mut context, true, &mut runner, None).unwrap();

    assert_eq!(outputs.len(), total_plan_commands + trash_sweeps.len());
    assert!(state_changed);
    assert!(context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .is_none());
    assert!(context
        .registry
        .get_task(&TaskId::new("web/fix-sidebar"))
        .is_none());

    let mut context = context_with_two_cleanable_tasks();
    let candidates = crate::commands::sweep_cleanup_candidates(&context);
    let trash_sweeps = crate::commands::sweep_trash_commands(&context);
    let first_candidate_command_count = crate::commands::clean_task_plan(&context, &candidates[0])
        .unwrap()
        .commands
        .len();
    let mut outputs: Vec<CommandOutput> = trash_sweeps.iter().map(|_| output(0, "", "")).collect();
    outputs.push(output(0, "ajax-web-fix-login\n", ""));
    outputs.extend((0..first_candidate_command_count.saturating_sub(1)).map(|_| output(0, "", "")));
    outputs.push(output(2, "", "branch delete failed"));
    let mut runner = RecordingQueuedRunner::new(outputs);

    let (error, state_changed) =
        execute_sweep_cleanup_operation(&mut context, true, &mut runner, None).unwrap_err();

    assert!(state_changed);
    assert!(matches!(
        error,
        CommandError::CommandRun(crate::adapters::CommandRunError::NonZeroExit {
            status_code: 2,
            ..
        })
    ));
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .expect("first candidate should remain after partial failure")
            .lifecycle_status,
        LifecycleStatus::Cleanable
    );
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("web/fix-sidebar"))
            .expect("second candidate should remain untouched")
            .lifecycle_status,
        LifecycleStatus::Cleanable
    );
}

#[test]
fn sweep_cleanup_treats_missing_tmux_session_as_already_removed_and_continues() {
    let mut context = context_with_two_cleanable_tasks();
    for handle in ["web/fix-login", "web/fix-sidebar"] {
        let task = context
            .registry
            .get_task_mut(&TaskId::new(handle))
            .expect("candidate task");
        task.lifecycle_status = LifecycleStatus::Merged;
    }
    context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .expect("first candidate")
        .tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    let trash_sweeps = crate::commands::sweep_trash_commands(&context);
    let first_plan = crate::commands::clean_task_plan(&context, "web/fix-login").unwrap();
    let kill_session_index = first_plan
        .commands
        .iter()
        .position(|command| {
            command.program == "tmux"
                && command.args.first().map(String::as_str) == Some("kill-session")
        })
        .expect("first candidate should plan a kill-session command");
    let mut outputs = sweep_success_runner_outputs(&context);
    outputs[trash_sweeps.len() + 1 + kill_session_index] =
        output(1, "", "can't find session: ajax-web-fix-login");
    let mut runner = RecordingQueuedRunner::new(outputs);

    let (_outputs, state_changed) =
        execute_sweep_cleanup_operation(&mut context, true, &mut runner, None).unwrap();

    assert!(state_changed);
    assert!(context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .is_none());
    assert!(context
        .registry
        .get_task(&TaskId::new("web/fix-sidebar"))
        .is_none());
}

#[test]
fn sweep_cleanup_kill_session_targets_exact_session_for_merged_task() {
    let mut context = context_with_two_cleanable_tasks();
    for handle in ["web/fix-login", "web/fix-sidebar"] {
        let task = context
            .registry
            .get_task_mut(&TaskId::new(handle))
            .expect("candidate task");
        task.lifecycle_status = LifecycleStatus::Merged;
    }
    context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .expect("first candidate")
        .tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    let mut runner = RecordingQueuedRunner::new(sweep_success_runner_outputs(&context));

    execute_sweep_cleanup_operation(&mut context, true, &mut runner, None).unwrap();

    let kill_targets: Vec<&str> = runner
        .commands
        .iter()
        .filter(|command| {
            command.program == "tmux"
                && command.args.first().map(String::as_str) == Some("kill-session")
        })
        .filter_map(|command| command.args.get(2).map(String::as_str))
        .collect();
    assert_eq!(kill_targets, vec!["=ajax-web-fix-login"]);
}

struct TmuxListSessionsRunner {
    list_sessions_outputs: VecDeque<CommandOutput>,
    commands: Vec<CommandSpec>,
}

impl CommandRunner for TmuxListSessionsRunner {
    fn run(
        &mut self,
        command: &CommandSpec,
    ) -> Result<CommandOutput, crate::adapters::CommandRunError> {
        self.commands.push(command.clone());
        if command.args.first().map(String::as_str) == Some("list-sessions") {
            return Ok(self
                .list_sessions_outputs
                .pop_front()
                .unwrap_or_else(|| output(0, "", "")));
        }
        Ok(output(0, "", ""))
    }
}

#[test]
fn drop_treats_no_tmux_server_after_kill_session_as_session_absent_1507() {
    let mut context = context_with_cleanable_task();
    let task_id = TaskId::new("web/fix-login");
    let mut runner = TmuxListSessionsRunner {
        list_sessions_outputs: VecDeque::from([
            output(0, "ajax-web-fix-login\n", ""),
            output(1, "", "no server running on /tmp/tmux-501/default"),
        ]),
        commands: Vec::new(),
    };
    let operation = plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

    let (_outputs, completion) =
        execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
            .unwrap();

    assert_eq!(completion, DropTaskCompletion::Removed);
    assert!(context.registry.get_task(&task_id).is_none());
}

#[test]
fn drop_keeps_unexpected_tmux_listing_failure_incomplete_1507() {
    let mut context = context_with_cleanable_task();
    let task_id = TaskId::new("web/fix-login");
    let mut runner = TmuxListSessionsRunner {
        list_sessions_outputs: VecDeque::from([
            output(0, "ajax-web-fix-login\n", ""),
            output(1, "", "permission denied"),
        ]),
        commands: Vec::new(),
    };
    let operation = plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

    let (_outputs, completion) =
        execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
            .unwrap();

    assert!(matches!(
        completion,
        DropTaskCompletion::TeardownIncomplete {
            failed_step: DropOp::EnsureTmuxSessionAbsent,
            ..
        }
    ));
    assert_eq!(
        context
            .registry
            .get_task(&task_id)
            .expect("task should remain after unexpected tmux failure")
            .lifecycle_status,
        LifecycleStatus::TeardownIncomplete
    );
}
