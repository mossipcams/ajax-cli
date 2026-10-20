# Native harness live parity

Status: **implemented** 2026-10-19 (Matt: fix 1–5; Claude plan mode is a non-goal). All scope items done and committed; not pushed, no PR.
Branch: `fix/web/native-live-parity` (from origin/main 36412e05).

The previous parity round (`native-harness-parity.md`) was verified only against
fake fixtures: under `cfg(test)` a native transport is selected only while a
fixture override is installed, so no test ever ran the real Claude SDK, Pi or
Cursor. A live probe against the real harnesses on 2026-10-19 found the defects
below. Core turns, tools, diffs, cancel, config changes and resume all work.

## Scope

- [x] **#1357 Claude todo/plan panel.** Sidecar sets `CLAUDE_CODE_ENABLE_TODO_TOOLS=1`
      in the query env; `claude_sdk_map.rs` maps main-loop `TaskCreate`/`TaskUpdate`
      to `Plan` (the `TodoWrite` path is dead with the current SDK).
- [x] **#1358 Claude AskUserQuestion.** `canUseTool` for `AskUserQuestion` forwards
      the questions as an `elicitation_request` and returns the operator's answers
      in `updatedInput.answers`; cancel/decline denies the tool.
- [x] **#1359 Claude slash-command output.** Text blocks of `assistant` messages
      with model `<synthetic>` map to agent text.
- [x] **#1360 Pi built-in slash commands.** `/compact [instructions]` and the other
      RPC-backed built-ins run their RPC command instead of being prompted.
- [x] **#1361 Pi extension notify.** `extension_ui_request` `notify` surfaces as
      visible output instead of being dropped.
- [x] Commit the live probe as an `#[ignore]` test so future parity audits run
      against the real harnesses.

## Deviations

- #1358 uses a `PreToolUse` hook, not `canUseTool`: the SDK never consults
  `canUseTool` under `bypassPermissions` (verified live).
- #1360 covers `/compact` and `/name` only; `/new` would replace the session id
  and `/session` has no chat surface. Added a follow-up so a failed manual
  compaction shows one error and a successful one shows a confirmation.
- #1361's change removed `notify` from the existing "fire-and-forget maps to
  nothing" test, which encoded the defect; the test now also gives the other
  methods a message.
- Three Qwen rounds timed out after finishing the edits; the parent ran the
  remaining verification (and `cargo fmt` once). The live test and one test
  revision were routed to Haiku.
- The live test found an unrelated Codex ACP flake: resume right after
  shutdown times out 2 of 3 runs (#1365, not fixed here).

## Non-goals

- Claude plan mode / permission-mode selector (bypass stays, #1297).
- Cursor: the SDK credential on this machine has expired; re-login is an
  environment step, not a code change.
- Codex ACP path.

## Delivery

One commit per issue on this branch, each with a regression test, routed
through the model router. Verification per commit: focused nextest for the
touched module, then `npm run verify` before any PR. Live re-probe of the real
harness after each Claude/Pi fix.

## Validation log

| Date | Command | Result |
|---|---|---|
| 2026-10-19 | live probe (real Claude SDK 0.3.302, Pi, Codex ACP) | defects above; Cursor blocked by expired auth |
| 2026-10-19 | real sidecar + Claude SDK: AskUserQuestion, TaskCreate/Update, /context | answer round-trips, ajax_plan updates, /context text shown |
| 2026-10-19 | live probe Pi: /extensions, /name, /compact | output shown, name set, compact runs (too-small session error) |
| 2026-10-19 | `AJAX_LIVE_HARNESS=claude` / `=pi` live_harness_parity | pass / pass |
| 2026-10-19 | `AJAX_LIVE_HARNESS=codex` live_harness_parity | 1 of 3 pass (#1365) |
