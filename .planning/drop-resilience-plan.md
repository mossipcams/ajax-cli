# Drop Resilience Plan

## Goal

Keep partially dropped tasks truthful and actionable: runtime refresh must not
mask teardown failures as ordinary missing-tmux failures, and retries must
continue safely from fresh substrate evidence until teardown can be confirmed.

## Task 1: Preserve teardown-incomplete status during runtime refresh

### Failing behavior test

- Add a focused unit test in `crates/ajax-core/src/runtime_refresh.rs`.
- Create a `TeardownIncomplete` task whose recorded drop failure identifies a
  remaining Git resource and whose tmux session is absent.
- Refresh runtime substrate and assert:
  - tmux absence is still recorded in substrate/runtime evidence;
  - lifecycle remains `TeardownIncomplete`;
  - the operator-facing live status/reason remains teardown-related instead of
    becoming the misleading primary `TmuxMissing` status.

### Code to implement

- Update runtime refresh/live-status application so expected tmux absence during
  `Removing` or `TeardownIncomplete` does not replace the recorded drop failure.
- Preserve tmux absence as substrate evidence for retry planning.
- Prefer the recorded `drop_failed_step` / `drop_failed_detail` when projecting
  the operator-facing blocker for incomplete teardown.

### Verification

```sh
rtk cargo nextest run -p ajax-core runtime_refresh
rtk cargo nextest run -p ajax-core recommended
```

## Task 2: Harden partial-drop retry after inconclusive final observation

### Failing behavior test

- Add a focused operation test in `crates/ajax-core/src/task_operations.rs`.
- Simulate a drop where teardown commands succeed but final observation is
  inconclusive, leaving the task `TeardownIncomplete`.
- Retry with fresh evidence showing resources absent and assert the task is
  hard-deleted without repeating already-satisfied destructive work.

### Code to implement

- Ensure incomplete-drop metadata and receipts retain enough evidence for an
  idempotent retry while fresh substrate observation remains authoritative.
- Make the retry complete removal when fresh final evidence proves all
  resources absent.
- Keep the task and explicit failed-step detail when observation remains
  unavailable; never report successful removal without proof.

### Verification

```sh
rtk cargo nextest run -p ajax-core task_operations
rtk cargo nextest run -p ajax-cli drop_execute_second_run_after_partial_failure_resumes_and_removes_task
```

## Final Validation

```sh
rtk cargo fmt --check
rtk cargo check --all-targets --all-features
rtk cargo clippy --all-targets --all-features -- -D warnings
rtk cargo nextest run --all-features
```
