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
    assert_eq!(branch_lists, 1, "repo branch observation should be reused");
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
