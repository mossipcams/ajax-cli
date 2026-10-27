# Plan: open-defects-remediation

## Scope

Fix every open defect on `mossipcams/ajax-cli` as of 2026-10-26 (main
`3d5f367b`): 47 issues. They are grouped into PR-sized batches where defects
share a root cause, and ordered by operator impact.

Each defect follows `docs/defect-process.md`. The issue already exists. Write
a focused regression test that fails on main, fix the root cause, run the
verify gate, and reference the issue in the PR.

## Non-goals

- Watcher defects #1197, #1198 and #1200. They were closed because they live in
  unmerged PR #1196 and are fixed on that branch.
- #1428 is owned by open PR #1431 (`fix/web/background-level-continuous`).
  Close it when that PR merges, and don't duplicate the work here.
- Redesigning the tmux, chat or lifecycle model beyond what each defect needs.
- Dependency Dashboard #1159 (not a defect).
- #1413 (session cookie minting) is deprioritized by operator decision: the
  public surface is behind Cloudflare Access. Known residual risk: a sandboxed
  agent on loopback, or a LAN host reaching `*:8787` directly, can still mint
  a session. That is accepted under the accident-prevention sandbox threat
  model (sandbox review 2026-10-22, phase 2 shelved).

## Context that changes severity

Web task creation does not use tmux by default. The browser preference
`ajax.web.session.orchestrationChat` is on unless set to `"false"`. The web
then sends `orchestration_chat: true`, which sets `skip_interactive_agent`,
and `new_task_plan_with_observation` skips `new_detached_task_session` and
`send_agent_command` (`crates/ajax-core/src/commands/new_task.rs:165-181`).

tmux still appears in three places:

- Every task gets a derived `tmux_session` name (`new_task.rs`,
  `task_from_new_request`).
- Opening the terminal creates the session lazily
  (`crates/ajax-core/src/commands/open.rs:72-84`).
- CLI-created tasks, and web tasks created with chat off, still run the agent
  in tmux.

So:

- **#1459 is P0.** Refresh writes `tmux_status.exists = false` for every chat
  task (`runtime_refresh.rs:233-240`), and `task_is_known_invalid`
  (`recommended.rs:232`) counts that as invalid. Every default web task can
  collapse to Drop-only.
- **#1450 and #1452 are P1, not P0.** For chat tasks a wrong-session kill hits
  a shell, not the agent. They still kill live agents for tmux tasks.
- **#1457 and #1451 mostly affect tmux tasks.** #1456 affects all tasks: its
  gate is lifecycle-based, not tmux-based.

## Approval

**Status: approved 2026-10-27 (Matt): C, E #1458, F, P and Q with the recommended directions.**

Per `docs/agent/plans.md`, these batches change architecture, security,
lifecycle or task-truth semantics. Each one needs explicit approval of the
chosen direction before implementation:

| Batch | Decision needed |
|---|---|
| F (#1460) | Branch policy. Options: (a) create task branches in a gh-pr-create-compatible shape, e.g. `fix/<repo>/<handle>`; (b) treat "worktree on another named branch, expected branch gone" as an adoptable checkout mismatch and expose branch adoption in Web Cockpit. Recommendation: (b), plus #1461's observed-branch PR lookup. |
| C (#1436) | Move chat-run cleanup into the core drop path, or key runs to task generation (`created_at`/id). Recommendation: core drop path, plus the web host evicting actors for unowned tasks on registry reload. |
| E (#1458) | Add a `Mergeable → Reviewable` edge to `generic_transition_allowed` (lifecycle table change). |
| P (#1327) | Execution owner fencing on `ExecutionCommit` (durable-execution contract). |
| Q (#1233) | Persist request_id outcomes (new storage). |

All other batches are localized defect fixes and need no approval beyond this
plan.

## Working rules (every batch)

- Delegate implementation via `model-router` (workspace AGENTS.md). Ban
  `git stash`, `checkout` and `reset` in every delegate prompt.
- Test first: the regression test must fail on main before the fix.
- Commit each round as soon as it passes the gate.
- Size limits: at most 600 changed lines per file and 1500 per PR.
- PR titles use `fix(<scope>): …` so each one cuts a release. Open PRs with
  `scripts/gh-pr-create` from a `fix/<scope>/<desc>` branch.
- Run the same commands as the hook and CI (see Validation).
- For UI batches, run `pkill -f vite; CI=1 npm run web:smoke`. Inside nono
  web:smoke can't run, so cite the CI Web E2E job instead.

## Task checklist

### Wave 0: decisions and closures

- [x] Close fixed or stale issues: #1267, #1299, #1309, #1310, #1392, #1197,
  #1198, #1200.
- [x] #1278 confirmed real; evidence comment added.
- [x] Approve directions for batches C, E, F, P, Q.
- [x] Reproduce #1459 against a copy of the dev DB (`--state` on a DB copy,
  real chat tasks) before fixing it. Not possible inside nono (no access to ~/.ajax-dev or loopback 8787/8788); the issue's scratch-test repro stands, and batch A's regression test failed on main with available=[Drop].

### Wave 1: P0

**A. Chat tasks forced to Drop-only (#1459)**

- [x] Fix: apply the `skip_interactive_agent` tmux and task-window exemption
  inside `task_is_known_invalid` (`crates/ajax-core/src/recommended.rs:232`),
  matching `has_missing_substrate` (`models/task.rs:190-200`).
- [x] Test: Reviewable chat task with `tmux_status.exists = false` and the
  window missing. Assert Ship is in `available_operator_actions` and the
  primary action is not Drop.

**C. Chat run state survives a non-web drop (#1436; needs approval)**

- [x] Fix: delete chat runs (transcript, ACP session id, prompt ledger) in the
  core drop path. The web host stops any live actor whose task is no longer
  owned on its next registry reload.
- [x] Test: drop via the CLI path, recreate the same handle, and assert an
  empty transcript, no stored `native_session_id`, and no re-queued prompts.

**D. Start fresh after a cold restore failure overwrites history (#1432)**

- [x] Fix: in `clear_session_context` (`task_session_spawn.rs:300-363`), load
  the stored transcript, normalizer seed and prompt ledger when the actor has
  no loaded log (share the first-acquire load path), and take a holder for the
  attaching socket.
- [x] Test: cold actor + `--load-fail` + start_fresh. Assert the stored rows
  before the note are unchanged and the history stays visible.

### Wave 2: P1 core

**E. Refresh and lifecycle gating (#1456, #1457, #1458, #1451)**

- [x] #1456: run `ci_monitor::refresh_ci_monitor` on `RefreshTier::Full`
  independently of the probe gate and the tmux early returns
  (`runtime_refresh.rs:69-73, 131-155, 551`). Test: the only task is
  Mergeable and its PR becomes MERGED; a Full refresh makes it Merged.
- [x] #1457: add `LifecycleStatus::Mergeable` to `should_probe_live_substrate`
  (`runtime_refresh.rs:617`). Test: a Mergeable task with `TurnStarted`
  becomes Running and Active after refresh.
- [x] #1458 (approval): add `Mergeable → Reviewable` and demote in
  `apply_failed` (`ci_monitor.rs:282-304`). Test: green then red CI for the
  same PR returns the task to Reviewable, and Ship is no longer offered.
- [x] #1451: sweep reuses `drop_cleanup_resource_is_already_missing`
  (`task_operations/sweep_cleanup.rs:59-72`). Test: a "can't find session"
  kill-session still removes the task and processes the next candidate.

**F. Branch rename and path identity (#1460, #1461, #1462; #1460 needs approval)**

- [x] #1460: the approved branch direction. Under option (b), a missing
  expected branch while the worktree is on another named branch projects as
  an adoptable checkout mismatch (`recommended.rs:199, 232`), and the web
  allows Repair with `branch_adoption` (`slices/operate/mod.rs:113`,
  `slices/actions.rs`). Tests: the switch and rename scenarios are not
  Drop-only, and the web offers adoption.
- [x] #1461: discover PRs by the observed `git_status.current_branch` when it
  differs from `task.branch` (`ci_monitor.rs:142`, `diff_review.rs:477`).
  Test: assert the PR-list command uses the observed branch.
- [x] #1462: compare canonicalised paths in `path_worktree`
  (`commands.rs`), falling back to the raw comparison. Test: a symlinked
  `worktree_path` with a canonical listing yields `worktree_exists`.

**G. tmux identity (#1450, #1452)**

- [ ] #1450: use exact targets (`-t =<session>`, `=<session>:<window>`) for
  every session-targeting command in `adapters/tmux.rs`. Tests: adapter unit
  tests per command, plus a sweep test with a stale cached `exists = true`
  that issues no prefix kill.
- [x] #1452: block the plan when a non-removed task already owns the derived
  `tmux_session` (`new_task.rs`). Test: the colliding repo/handle pair returns
  `PlanBlocked`.

**H. Web Ship needs a confirmation it never offers (#1453)**

- [x] Fix: the web DTO sets `confirmation_required` from
  `merge_task_plan(...).requires_confirmation` (`slices/actions.rs:18-28`).
  Fix the "tap again" copy and `errorRecovery.ts`.
- [x] Tests: a Reviewable task with Unpushed projects
  `confirmation_required: true`, and a confirmed Ship executes end to end.

**I. Diff uses the stale local base (#1454)**

- [x] Fix: diff against `origin/{base_branch}`, falling back to the local
  branch, in `diff_task_plan` (`commands/diff.rs:22`) and
  `GithubChecksAdapter::local_diff` (`adapters/github.rs:128`).
- [x] Test: a temp repo where local main lags origin/main shows only the
  task's own file.

**J. Supervisor findings, unreproduced (#1236, #1237, #1238)**

- [x] Write failing tests in `crates/ajax-supervisor` first:
  - #1236: Completed then late stdout stays Completed.
  - #1237: cancel emits a terminal lifecycle event.
  - #1238: output without progress is classified as hung after the threshold.
- [x] Fix the ones that fail. Close the ones that pass, with the test as
  evidence.

### Wave 3: chat durability and status

**K. Prompt acknowledgement (#1434, #1239)**

- [x] Fix: append `PromptAccepted`, or a terminal ack the outbox honours, for
  `Interrupted` entries and on begin failure (`task_session_exit.rs:369-373,
  500-512`). The browser marks optimistic prompts as pending until accepted
  and keeps unaccepted ones for retry (`webSessionTransport.ts:41-49, 113`).
- [x] Tests: an interrupted prompt drains from the outbox once, and an
  unaccepted prompt renders as pending.

**L. Transcript backlog (#1435, #1407)**

- [ ] #1407 first: decide whether it's a test race or a product ordering bug.
  Don't lengthen the sleep. Must run unsandboxed or in CI (nono blocks
  start-time reads).
- [ ] #1435: retry a non-empty `unpersisted` backlog on the poll tick
  (`task_session.rs:316-321`), and attempt a final commit before actor exit
  (`task_session.rs:448-466`). Test: a failed append followed by restored
  writes, with no new events, commits on tick and at shutdown.

**M. transcriptError invisible (#1437)**

- [x] Fix: restore the client parsing and the composer gate and notice from
  the #1118 revert diff.
- [x] Test: a snapshot with `transcriptError` disables Send and shows the
  notice.

**N. Chat status (#1438, #1439, #1433; after #1431 merges)**

- [ ] #1438: `derive_task_status` keeps the known running summary
  (`ui_state.rs:161-164, 260-266`). Core test from observation to
  `status_explanation == "Background work running"`.
- [ ] #1439: report a settled activity on actor shutdown or eviction and on
  startup for runs this boot doesn't own, and report the current projection on
  first acquire. Test: an actor ends while running and the status clears.
- [ ] #1433: refetch the open task's detail on the cockpit poll cadence, and
  stop the converge interval on `waiting` or `error`
  (`useTaskDetailResource.ts`, `TaskWorkspace.tsx:83-91, 205-209`). Test: a
  server status change while the stream is idle reaches the head.

**O. Process and restore robustness (#1278, #1365)**

- [ ] #1278: on reader-thread spawn failure, kill the child's process group
  and wait before returning (`jsonl_process.rs:76-95`). Test: an injected
  spawn failure reaps the child.
- [ ] #1365: investigate with the live parity test. Is the previous codex
  process still holding the session, or did config switches break restore?
  Then fix (e.g. wait for the old child to exit before resume) with a
  regression test.

**P. Execution ownership (#1327; needs approval)**

- [x] `owner_fence` on `ExecutionCommit`, checked inside the IMMEDIATE
  transaction (`Fenced` error).
- [x] Orphan sweep polls for exit, escalates to SIGKILL, and clears identity
  only once the process is gone.
- [x] `record_child_identity` returns a Result, and attach fails on error.
- [x] Tests for each of the three, plus an update to durable-execution.md.

**Q. Idempotency after restart (#1233; needs approval)**

- [x] Persist completed request_id outcomes (registry table with a TTL), and
  have `OperationCoordinator` consult it.
- [x] Test: complete a request, restart the coordinator, and a retry returns
  the original outcome without re-executing.

### Wave 4: P2 Web Cockpit

**R. Latches and undo state (#1440, #1441, #1442)**

- [x] #1440: keep undo state per drop (`taskMutations.ts:98-125`). Test: two
  arms 1 s apart and both drops post.
- [x] #1441: persist `clientMessageId` in the composer queue
  (`draftStorage.ts:12-75`). Test: after a remount, removeQueued calls
  `withdrawQueuedPrompt(id)`.
- [x] #1442: release the Retry latch after `onRetry` settles
  (`ConnectionStatus.tsx:22-27`), as #978 did for Reload. Test: two clicks in
  the same state call `onRetry` twice.

**S. Chat surface (#1446, #1455, #1447)**

- [x] #1446: key the scroller and history window on the handle, not
  `detailStatus` (`ChatSurface.tsx:241`). Test: ready → stale → ready leaves
  scrollTop and the visible item count unchanged.
- [x] #1455: call `autoGrow` in `setDraftWithPersist` (`useComposer.tsx`).
  Test: multi-line `insertDelta` calls autoGrow.
- [x] #1447: always render a disabled `Select…` placeholder for enums without
  a default (`ElicitationPanel.tsx:19-31`). Test: picking the only option of
  a single-option enum enables Accept.

**T. Markdown (#1463)**

- [x] Fix in `Markdown.tsx`: autolink bare `https?://` through `safeLink`,
  `<ol start>`, fences `/^\s{0,3}```(\S*)/`, and `*` / `_` emphasis. Never
  render raw HTML.
- [x] Tests: one per case in `Markdown.test.tsx`.

**U. Controls (#1386, #1135)**

- [x] #1386: disable Confirm while its action is active, plus a ref guard in
  `execute()` (`TrainingModal.tsx`). Test: a double-click makes one API call.
- [ ] #1135: the expand-corner overlay must not intercept the Details header
  button. Use a surface-containment check, not a pixel snapshot. Test: an e2e
  click on `task-details` while expanded succeeds.

**V. Cloudflare expiry in the Home Screen app (#1378)**

- [ ] Reproduce on the device (iOS sim or Home Screen app). Probable fix: when
  a fetch hits the Access redirect or a 401, do a top-level navigation to the
  sign-in URL. Add a regression test once the trigger is known.

### Wave 5: P3 polish (one web PR)

**W.**

- [x] #1443: `close()` before `onOpenDiff()` (`TaskDetailsSheet.tsx:124-127`).
- [x] #1444: `MetaCopyCell` reports through `onResult`
  (`TaskMetaDetails.tsx:127`).
- [x] #1445: "1 needs you" singular (`App.tsx:203`).
- [x] #1448: Switch is disabled while `agent === currentAgent`
  (`HarnessSwap.tsx:52-55`).
- [x] #1449: a unique result id as the `ResultPanel` key
  (`ResultPanel.tsx:56-72`).
- [ ] One regression test per item, as described in each issue.

### Wave 6: design question

- [ ] #1370: web coding tasks can bypass the router. Decide whether to
  enforce (a launch-path preamble or hook that records the router decision)
  or accept instruction-only, then fix or close with the rationale.

## Execution order

0 → A → C → D → E → G → F → H → I → J → K → L → M → N → O → P → Q → R →
S → T → U → V → W → 6.

A has no dependencies and goes first. E goes before G because sweep relies on
the refreshed tmux evidence. F needs its approval. N waits for #1431 to merge.

## Deviations

- 2026-10-26: severity re-ranked after confirming that web tasks skip tmux at
  creation (see "Context that changes severity").
- 2026-10-26: #1278 stays open. The bug was confirmed in code, so it was not
  closed for its empty body.
- 2026-10-26: #1428 is owned by PR #1431 and excluded here.
- 2026-10-26: batch B (#1413) removed. The operator considers it covered by
  Cloudflare Access (see Non-goals for the residual risk).
- 2026-10-26: #1459 live DB repro skipped (nono blocks the dev DB and loopback web ports); the failing regression test is the repro.
- 2026-10-26: J discovery found all three supervisor defects real. #1237's fix (observer sends Exited on cancel; runtime forwarder drains for up to 1s) is ready but contradicts the existing test `runtime_cancel_closes_monitor_receiver`, which asserts the receiver closes with no event after cancel. #1236 contradicts `status_machine_yields_done_to_late_busy_output` and `..._blocked_...`, which suggests late output after Completed is intended for interactive agents. #1238 needs a definition of "progress" and contradicts `hang_detector_tracks_quiet_processes`. All three are parked for an operator decision; the work is saved as a patch outside the repo, and the tree was restored.
- 2026-10-26: #1450 and #1454 are held for approval because each fix rewrites existing test expectations that encode the defect (about 118 raw `-t <session>` assertions for #1450; 6 `main...HEAD` assertions for #1454). #1454 skips the plan's fallback to the local branch: new_task always fetches origin and branches from origin/<default>.
- 2026-10-26: Batches D, K, L, O and P live in ajax-web web_session, whose tests spawn ACP children that nono blocks ("child identity could not be recorded"), so they cannot be verified locally.
- 2026-10-26: #1456 backed out (commit 388856fd reverted by a forward commit). An always-on Full-tier CI monitor contradicts 6 existing ajax-cli tests: cockpit_backend::tests::{cockpit_snapshot_reuses_cache_when_visible_tasks_are_unchanged, live_refresh_does_not_probe_generic_error_task_without_live_attention, live_refresh_skips_window_and_pane_probes_for_non_live_tasks} and tests::{live_refresh_records_nonzero_session_listing_as_probe_failure, live_refresh_skips_cleanable_tasks_without_tmux_probe, tmux_probe_failure_renders_unavailable_without_marking_session_missing}. The all-Mergeable scenario is fixed by #1457; the remaining tmux-failure case needs an operator decision. Lesson: the E rounds were gated with -p ajax-core only; workspace nextest caught it.

- 2026-10-27: Q persists request_id outcomes to a bounded JSON file in the web state_dir instead of a registry table, so the shared registry schema stays at v10 (a v11 migration would lock out installed binaries and requires editing the user_version==10 test literals).
- 2026-10-27: P (#1327) needed no code. #1329 (9fd73b2b) already added owner_fence on ExecutionCommit checked in the IMMEDIATE transaction, the TERM -> poll -> KILL orphan sweep that clears identity only once gone, and a fallible record_child_identity that fails attach. Regression tests: ajax-core registry/sqlite/tests/execution_recovery.rs owner_fence_* (pass), plus ajax-web orphan_sweep_tests / task_session_phase2_tests *_issue_1327 (process-identity tests; blocked under nono before #1429).
- 2026-10-27: #1238 progress definition (operator delegated the choice): only stdout lines that parse into an AgentEvent reset the hang timer; raw stdout and stderr do not.
- 2026-10-27: the branch was rebased onto origin/main 8beeca0e (#1429) with operator approval, which made the web_session tests runnable in nono.
- 2026-10-27: the operator approved updating the existing tests that pinned old behaviour for #1450, #1454, #1456, #1237, #1448 and #1460; #1236 was closed as by-design; #1327 (fixed by #1329) and #1386 (fixed by #1388) were closed.
- 2026-10-27: PRs were split by file overlap into six code PRs (#1464-#1469) plus this docs PR, each cut from origin/main with the plan and generated dist excluded and dist rebuilt per web PR.
- 2026-10-27: every PR got an independent review (Codex for #1464-#1466; Cursor/Opus for #1467-#1469 after Codex hit its usage limit); all requested changes were made; verified findings were fixed as follow-up commits and pushed, and a combined re-review approved the #1464-#1467 follow-ups.
- 2026-10-27: a security review flagged #1461's observed-branch PR lookup when a worktree is on its base branch; fixed by excluding the base branch (cross-repo PR filtering left as a follow-up).
- 2026-10-27: the disk filled twice during multi-branch builds; CARGO_INCREMENTAL=0 and cargo clean were used.
- 2026-10-27: the ajax-web test expected_completion_does_not_surface_sidecar_exit_error is load-sensitive (passes alone) and was excluded from local PR gates; CI still runs it.

## Validation

Per batch (the hook and CI commands):

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
cargo test --doc
npm run web:check && npm run web:lint && npm run web:sg && npm run web:test -- --run
npm run ci:verify
# UI batches (outside nono):
pkill -f vite; CI=1 npm run web:smoke
```

The full gate is `npm run verify`.

Results (record each batch here):
- A (#1459): cargo fmt --check pass; workspace clippy --all-targets --all-features -D warnings pass; cargo nextest -p ajax-core 1099/1099 pass (RUST_LOG unset). Workspace nextest 3018/3168 under nono: the 150 failures are ps/start-time-token and ~/.ajax-dev sandbox denials in ajax-web, ajax-supervisor and ajax-cli, unrelated to the change; CI is authoritative. PR: #1464.
- E #1457: regression test failed before the fix (agent_status NotStarted vs Running); ajax-core nextest 1100/1100, fmt and clippy -p ajax-core pass. PR: #1464.
- E #1456: 2 regression tests failed before the fix (tmux list-sessions failure left the task Mergeable; a probe-ineligible Merged task kept its CI episode), plus a Live-tier guard; ajax-core nextest 1103/1103, fmt and clippy -p ajax-core pass. The plan's Mergeable-only repro no longer applies after #1457, so the test forces the tmux failure path instead. PR: #1464.
- E #1451: regression test failed before the fix (NonZeroExit from tmux kill-session "can't find session"); ajax-core nextest 1104/1104, fmt and clippy -p ajax-core pass. PR: #1464. #1458 still awaits approval.
- G #1452: regression test failed before the fix (planner returned Ok with a colliding `new-session -s ajax-ajax-cli-fix`); ajax-core nextest 1105/1105, fmt and clippy -p ajax-core pass. PR: #1464.
- G #1450: held for approval. Exact `=session` targets change about 118 raw `-t <session>` expectations across about 25 existing test files in ajax-core, ajax-cli and ajax-web, plus tmux builders outside TmuxAdapter.
- H #1453: actions-level regression test failed before the fix (Ship confirmation_required false for an Unpushed task); ajax-web slices::actions + slices::operate 30/30, fmt and clippy -p ajax-web pass. errorRecovery.ts left unchanged: its existing test pins confirmation_required to hint "retry", and the DTO fix alone opens the Confirm panel. PR: #1467.
- M #1437: 3 parser/reducer/composer tests plus 3 useSessionConnection tests all failed before the fix; web:check, web:lint, web:sg pass; vitest 1763 passed / 9 skipped. Restores the pre-#1118 behaviour exactly; the transcript notice has no recovery button, as before, so on mobile /clear depends on the keyboard's Return submitting (follow-up). PR: #1469.
- R #1440/#1441/#1442: 6 new tests failed before the fixes; web:check, web:lint, web:sg pass; vitest 1770 passed / 9 skipped. Follow-ups: DropUndoHandles.dropResolvedRef and clearDropTimer are now dead; a second drop's banner replaces the first's Undo button (both drops still post). PR: #1469.
- S #1446/#1455/#1447: each regression test failed before its fix; vitest 1774 passed / 9 skipped; web:check, web:lint, web:sg pass. #1446 keys on handle + (detailStatus === "loading") because a handle-only key breaks the existing ChatScroller re-pin test. The #1455 speech-only grow and the iOS keyboard path are unverified here (web:smoke blocked). PR: #1469.
- T #1463: 6 case tests failed before the fix; a revision round fixed the opener so an inline "```x```" span no longer opens a fence; Markdown 21/21, vitest 1783 passed / 9 skipped; web:check, web:lint, web:sg pass; inert javascript: and <img onerror> guard test included. PR: #1469.
- U #1386: already fixed on main by #1388 (248c7860: in-flight ref guard plus disabled Confirm; Stop-path test exists). Added a train-path and re-enable test, which fails when the guard is removed (mutation check); vitest 1784 passed. Close #1386 citing 248c7860. #1135 still needs Playwright (blocked under nono).
- W #1443/#1444/#1445/#1449: each regression test failed before its fix; web gate green. #1448 held: disabling Switch contradicts the existing HarnessSwap test "disables the current harness and refuses a same-harness apply". PR: #1469.
- E #1456: backed out; see Deviations. Re-applied with operator approval in PR #1464.
- E #1458: regression test failed before the fix (task stayed Mergeable after red CI), passes after (ajax-core 1101/1101; ajax-core+ajax-cli 1487/1488, the one failure is the sandbox binary_prints_cli_errors_with_display_formatting); fmt and clippy -D warnings pass. PR: #1464.
- F #1462: symlinked-path refresh test failed before the fix (worktree_exists false, WorktreeMissing set), passes after; ajax-core 1108/1108 and ajax-core+ajax-cli 1490/1491 (the one failure is the sandbox binary_prints_cli_errors_with_display_formatting); fmt and clippy -D warnings pass. PR: #1464.
- F #1461: 2 regression tests failed before the fix (gh pr list --head ajax/fix-login instead of fix/web/login, in CI monitor and observe_task_pull_requests); 2 guard tests (current branch equal to task branch, or unknown) pass before and after. ajax-core+ajax-cli nextest 1494/1495 (the one failure is the sandbox binary_prints_cli_errors_with_display_formatting); fmt and clippy -D warnings pass. PR: #1464.
- F #1460 (core): 1 regression test (rename: expected branch gone, worktree on fix/web/login) failed before the fix and passes after; the switch case (expected branch still present) and the detached guard already passed before and still pass; ajax-core 1115/1115, ajax-cli 382/383 (the one failure is the sandbox binary_prints_cli_errors_with_display_formatting), ajax-web slices::cockpit/actions/operate 49/49; fmt --check and clippy -D warnings pass. Web Cockpit adoption pending: existing test ajax-web cockpit::tests::browser_cockpit_and_detail_pass_through_checkout_mismatch pins actions [resume, drop] without repair. PR: #1464.
- Q #1233: restart replay test failed before the fix ("a retry after restart must replay the original outcome"), passes after; runtime::tests 112/115 (the same 3 pre-existing suite_4 harness-swap failures as before the change, 109/112); ajax-web slices::cockpit/actions/operate 49/49; fmt --check and clippy -D warnings pass. PR: #1468.
- C #1436 (core): regression test (CLI drop, recreate same handle) failed before the fix (recreated task inherited run-a with native_session_id acp-a) and passes after; ajax-core+ajax-cli nextest 1498/1499 (the one failure is the sandbox binary_prints_cli_errors_with_display_formatting); ajax-web slices::cockpit/operate and adapters::web_session_store 100/100; fmt --check and clippy -D warnings pass. Web host eviction of live actors for unowned tasks pending (web_session tests cannot run under nono before #1429). PR: #1465.
- P #1327: already fixed by #1329 (9fd73b2b); close the issue with that evidence.
- J #1237: 2 regression tests (process_observer launch_tests::issue_1237_cancel_ends_event_stream_with_terminal_event, runtime runtime_cancel_ends_event_stream_with_exited_event) and the updated runtime_cancel_closes_monitor_receiver; ajax-supervisor nextest 135/135 (RUST_LOG unset); fmt --check and clippy --all-targets -D warnings pass; #1236 closed as by-design (status_machine_yields_*_to_late_busy_output pin it). PR: #1466.
- I #1454: regression test (local main lags origin/main; task diff and Diff Review must list only task.txt) failed before the fix (listed task.txt and upstream.txt), passes after; ajax-core+ajax-cli nextest 1499/1500 (the one failure is the sandbox binary_prints_cli_errors_with_display_formatting); fmt --check and clippy -D warnings pass; 6 range assertions updated with operator approval. PR: #1464.
- E #1456 (re-applied with approval): 4 ajax-cli refresh tests changed: 2 had their assertions replaced (cockpit_backend::tests::live_refresh_does_not_probe_generic_error_task_without_live_attention, tests::live_refresh_skips_cleanable_tasks_without_tmux_probe); 2 round-1 edits (tests::live_refresh_records_nonzero_session_listing_as_probe_failure, tests::tmux_probe_failure_renders_unavailable_without_marking_session_missing via FailingTmuxRunner). 2 ajax-core suite_4 #1456 tests un-ignored and pass; new ci_monitor retire no-op test. PR: #1464.
- F #1460 (web): before, the web hid Repair, so a renamed or switched branch could only be dropped (the renamed-branch projection test fails on that code by construction); after, Repair projects as a confirmed "Adopt branch" when core plans adoption, and operate runs it. New tests: browser_actions renamed-branch projection and operate_slice_runs_confirmed_repair_branch_adoption_for_renamed_branch; ajax-cli web_bridge_persists_confirmed_mismatch_branch_adoption flipped back to success (fix/pane-stuck persisted). ajax-web+ajax-cli nextest 1741/1742 (the one failure is the sandbox binary_prints_cli_errors_with_display_formatting); stt_provider test passes alone; web:test 1788 passed / 9 skipped; fmt --check and clippy -D warnings pass; cockpit checkout-mismatch test updated with operator approval. PR: #1467.
- W #1448: Switch is disabled while the selected harness equals the current one, with the note kept as helper text; HarnessSwap same-harness test updated with operator approval. PR: #1469.
- J #1238: 2 regression tests (process_observer launch_tests::unparsed_stdout_does_not_reset_hang_detection, stderr_does_not_reset_hang_detection) failed before the fix (no Hung event; quiet-stream threshold 200 ms) and pass after; parsed-event guard (parsed_agent_events_keep_hang_detection_quiet) passed before and after; ajax-supervisor+ajax-cli nextest 520/521 (the one failure is the sandbox binary_prints_cli_errors_with_display_formatting); fmt --check and clippy --all-targets -D warnings pass. PR: #1466.
- C #1436 (web): before, a registry reload that dropped a task left its live chat actor in TaskSessionDirectory (ACP child running in the deleted worktree); the new test shuts_down_live_sessions_for_unowned_handles failed on the no-op stub (actor still live). After, cleanup_unowned shuts down and deletes unowned sessions and the owned-handles guard test passes; ajax-web session_cleanup 7/7 (2 new), ajax-web nextest 1361/1361 (1 skipped); fmt --check and clippy -D warnings pass. PR: #1465.
- D #1432: regression test start_fresh_after_cold_restore_failure_keeps_stored_history_issue_1432 failed before the fix (attach snapshot 2 rows, expected 5: stored history missing) and passes after; ajax-web nextest 1362/1362 (1 skipped; web_session 974/974 before the test was counted); fmt --check and clippy -D warnings pass. PR: #1468.
- K #1434: before, the interrupted-prompt test (prompt_ack_tests::interrupted_prompt_is_acknowledged_once_and_not_again_on_reconnect) failed (0 PromptAccepted acks, expected 1) and the 3 begin-failure ack tests passed; after, all 4 pass and ajax-web nextest is 1366/1366 (1 skipped); fmt --check and clippy -D warnings run per the gate. PR: #1468.
- K #1239: optimistic prompts show Sending… until prompt_accepted; outbox kept on socket-open failure; web:test 1796 passed / 9 skipped, web:check, web:lint, web:sg pass. PR: #1469.

## Remaining work (2026-10-27)

- L #1435/#1407 and O #1278: work orders written, not yet run.
- #1450: tmux exact targets, approved; three rounds written, not yet run.
- #1365: parked; needs the real Codex harness and a live parity test.
- N #1438/#1439/#1433: waiting on PR #1431.
- V #1378: device repro.
- U #1135: needs Playwright; cannot run under nono.
- Wave 6 #1370: design decision.
- Review follow-ups noted as acceptable:
  - a short recreate+acquire window in drop_session (#1465);
  - Exited is emitted before termination is confirmed (#1466);
  - a consumer stalled before cancel can still block a normal send (pre-existing);
  - outcome writes from back-to-back finishes may land out of order (#1468).
