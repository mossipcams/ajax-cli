# Graphify Task Creation Plan

## Goal

Generate Graphify output inside each newly created task worktree and treat
`graphify-out/` as generated, gitignored data rather than committed repository
content.

The existing `graphify_update` repo configuration remains optional and detached
so repositories without Graphify keep working and task startup does not wait for
the graph build.

## Task 1: Run Graphify in the new task worktree

### Failing behavior test

- Update the focused behavior tests in
  `crates/ajax-core/src/commands/new_task.rs`.
- Replace the repo-root expectation with an assertion that the configured
  detached `graphify_update` command:
  - appears after `git worktree add`;
  - uses the new task worktree as its cwd;
  - appears before the task session and agent launch commands.
- Run the focused Graphify planning tests and confirm they fail against the
  current repo-root command ordering.

### Code to implement

- Move the optional Graphify command after the worktree-add command.
- Set its cwd to the newly created task worktree.
- Preserve the existing configured command, detached execution, and public
  configuration shape.

### Verification

```sh
rtk cargo nextest run -p ajax-core graphify
rtk cargo nextest run -p ajax-core new_task_plan
```

## Task 2: Require generated Graphify output to be gitignored

### Failing behavior test

- Update the focused doctor behavior test in
  `crates/ajax-core/src/commands.rs`.
- Assert that `ajax doctor` passes when `graphify-out/` is gitignored and warns
  when it is not gitignored.
- Run the focused doctor test and confirm it fails against the current inverted
  policy.

### Code to implement

- Invert the Graphify doctor check so ignored output is healthy.
- Update the doctor messages and environment test-fixture naming to describe
  the generated-output policy clearly.

### Verification

```sh
rtk cargo nextest run -p ajax-core doctor
```

## Task 3: Encode and document the generated-output policy

### Documentation and configuration to update

- Add `graphify-out/` to the repository `.gitignore`.
- Update `README.md` to explain that `graphify_update` runs from each new task
  worktree and that generated Graphify output should be ignored.
- Update `architecture.md` so the documented start flow and doctor policy match
  the implementation.

### Verification

```sh
rtk git check-ignore -q graphify-out/graph.json
rg -n "graphify_update|graphify-out" README.md architecture.md .gitignore
```

## Final Validation

```sh
rtk cargo fmt --check
rtk cargo check --all-targets --all-features
rtk cargo clippy --all-targets --all-features -- -D warnings
rtk cargo nextest run --all-features
```
