
#[test]
fn smoke_destructive_commands_require_confirmation() {
    let sandbox = SmokeSandbox::new("destructive-safety");
    sandbox.create_repo("web");
    sandbox.write_config(&["web"]);
    complete_web_task_to_reviewable(&sandbox);

    let merge_without_yes = sandbox.ajax(["ship", "web/fix-login", "--execute"]);
    assert_success(
        &merge_without_yes,
        "safe ajax ship --execute without explicit --yes",
    );

    let clean_without_yes = sandbox.ajax(["drop", "web/fix-login", "--execute"]);
    assert_success(
        &clean_without_yes,
        "safe ajax drop --execute without explicit --yes",
    );

    create_active_web_task(&sandbox);
    let before_remove = sandbox.command_log();
    let remove_without_yes = sandbox.ajax(["drop", "web/fix-login", "--execute"]);
    assert!(
        !remove_without_yes.status.success(),
        "remove --execute should require explicit --yes"
    );
    assert!(
        stderr(&remove_without_yes).contains("confirmation required; pass --yes"),
        "remove failure should explain confirmation:\n{}",
        stderr(&remove_without_yes)
    );
    assert_eq!(
        before_remove,
        sandbox.command_log(),
        "remove without --yes must not run external commands"
    );

    let remove = sandbox.ajax(["drop", "web/fix-login", "--execute", "--yes"]);
    assert_success(&remove, "ajax drop --execute --yes");
    let tasks = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    assert_eq!(tasks["tasks"], Value::Array(vec![]));
}
