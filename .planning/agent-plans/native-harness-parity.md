# Native harness parity and defects

Status: **approved** 2026-10-18. P0, P1, P2, P3, P4 done (PRs #1343-#1349, stacked on #1337); P5 deferred (Codex still uses ACP).

The web chat moved from ACP (claude-agent-acp, pi-acp, cursor ACP) to native
transports: Claude Agent SDK sidecar, Pi RPC, Cursor SDK sidecar. Codex stays on
ACP. This plan covers what is still missing or broken after that move.

## Done

- [x] Audit of ACP vs native event coverage.
- [x] #1333 Claude empty system prompt, #1334 Claude context meter, #1335 Pi
      silent provider errors, #1336 "ACP process exited" wording, shipped as
      PR #1337 (`fix/web/native-harness-defects`, CI green, mergeable).
- [x] #1338 Claude TodoWrite → plan panel and #1339 Claude/Pi tool titles and
      kinds. Implemented and verified (`npm run verify`: 3007 nextest + vitest +
      lint, all pass). **Uncommitted** in this worktree on top of #1337.

## Scope

### P0. Ship group 1
- [x] (PR #1343) Rename and commit onto a new branch `feat/web/native-tool-titles-and-plan`
      from the current HEAD, then open the PR with
      `BASE_BRANCH=fix/web/native-harness-defects scripts/gh-pr-create`. The PR is
      stacked on #1337 because both touch `claude_sdk_map.rs`. Body: `Closes #1338`,
      `Closes #1339`.

### P1. Triage the older native-harness defect issues — DONE
Outcome: #1272, #1273, #1298, #1306 already fixed (closed, tests exist). #1274 PR #1344, #1275 PR #1345, #1276 PR #1346, #1279 PR #1348 fixed real defects. #1277 passes on current code (test-only PR #1347).
All have a title and an empty body, and none is linked from a commit. For each
one: the delegate writes a focused regression test for the symptom in the title,
run against current code.
- Test passes: the defect is already fixed. Close the issue, naming the fixing
  commit, and keep the test.
- Test fails: fix it (one packet per issue, `fix/web/...` PR, `Fixes #N`).

| Issue | Symptom | First evidence |
|---|---|---|
| #1272 | clear-context/harness-switch fails on Claude/Pi (idle cancel fatal) | Likely fixed: `SessionClient::cancel` treats an idle RPC host as a no-op ("Releasing an idle live RPC host needs no abort"), probably via #1328 |
| #1273 | Successful teardown renders as an error containing the session id | Unknown |
| #1274 | Claude sidecar pump failure leaves the session busy and healthy-looking | Unknown |
| #1275 | Claude error results reported as successful prompts | Partly addressed by #1287 (error text shown); need to check the prompt outcome is Failed |
| #1276 | Pi startup reports a dead session healthy when the child exits after `get_state` | Unknown |
| #1277 | Pi handshake deadline not enforced while records keep arriving | Unknown |
| #1279 | Pi model selection loses provider identity when providers share model ids | Unknown |
| #1298 | Harness shutdown leaves grandchild processes | Likely fixed by 69f455d5 (kill process groups) |
| #1306 | Pi turn hangs on extension dialogs | Likely fixed by PR #1318 (`dialog_request` maps `extension_ui_request`) |

Routing: Qwen, one issue per packet. A packet that has to touch more than 4
files goes to Haiku.

### P2. #1342 Claude sub-agent (Task) activity — DONE (PR #1349; sub-agent TodoWrite keeps its tool row but never emits a Plan)
- [ ] In `claude_sdk_map.rs`, stop dropping messages with a non-null
      `parent_tool_use_id` wholesale. Map sub-agent `assistant` tool_use blocks
      and `user` tool_result blocks to normal tool rows (tool ids are unique, so
      the rows sit flat under the parent Task row). Keep dropping sub-agent
      `stream_event` text and thinking, which would otherwise interleave into the
      parent's message.
- [ ] Tests: a sub-agent tool call opens, updates and completes; sub-agent text
      produces no message events; the main-loop mapping is unchanged.
- Files: `claude_sdk_map.rs`, `claude_sdk_map_tests.rs`. Docs bullet in
  `web-session-behavior.md`. Delivered as a `feat/web/...` PR.

### P3. Close #1340 and #1341 with evidence (no code) — DONE, both closed not planned
- [ ] #1340 (session title, live status, per-turn usage): none of these is
      visible today, even for Codex on ACP.
  - `sessionTitle` reaches `useChatSession` but no component renders it.
  - The `status` wire event is stored in `view.status.acpState` and never read.
  - The head label comes from `turn.busy`, decisions/elicitations and task
    attention (`headView.ts` `headState`).
  - `formatTurnUsage` has no caller.

  Wiring these for native harnesses would change nothing on screen, so close as
  not planned and point to P5.
- [ ] #1341 (harness-initiated config changes, Cursor context meter):
  - Cursor SDK 1.0.37's public typings expose no context usage; it exists only
    in internal protobufs. Cursor never reported context under ACP either (see
    the docs), so this is not a regression.
  - Harness-initiated model/effort changes have no realistic trigger. Changes
    come from the cockpit sheet, which works. Verify that Claude
    `supportedCommands` and Pi `get_commands` do not include a built-in `/model`.
    If neither does, close as not planned; otherwise, rescope to that command.

### P4. Docs correction — DONE (in PR #1349)
- [ ] `web-session-behavior.md` says ACP `status.state` drives the live-head
      label (Working / Needs you / Ready). The code derives it from turn and
      attention state instead. Correct the bullet. This can ship with P2.

### P5. Optional: delete the dead browser plumbing (needs a separate yes)
Re-checked against the code on 2026-10-18; the original "end to end" scope was wrong for `status`.

| Item | Browser | Server | Verdict |
|---|---|---|---|
| `status` event | stored in `view.status.acpState`, never read (`statusProjection.ts`, `model.ts`, `projectWireInput.ts`, `turnProjection.ts`) | **used**: `acp_execution_map.rs` maps it to execution state and `session_activity.rs` reports it | delete the browser store only; keep the Rust event |
| `sessionTitle` | parsed into `useChatSession`, never rendered (`parse.ts`, `contracts.ts`, `model.ts`, `useChatSession.ts`, `useSessionConnection.ts`) | produced only by ACP `SessionInfoUpdated` (`sdk_connection.rs`), so Codex only; carried in the snapshot (`protocol.rs`, `task_session_outbound.rs`, `acp_slot.rs`, `acp_drain.rs`) | browser delete is safe; Rust delete touches the Codex ACP path |
| `turn_usage` | `usageProjection.ts`, `UsageIndicators.tsx` `formatTurnUsage` has no caller | produced by `acp_usage.rs` (Codex ACP), ignored by `transcript.rs`/`normalize.rs` | browser delete is safe; Rust delete touches the Codex ACP path |

- [ ] **P5a (browser only, no wire change):** delete the unused `acpState`,
      `sessionTitle` and per-turn usage parsing/projection/formatting and their
      tests in `crates/ajax-web/web/src`. The server keeps sending the fields;
      the browser already ignores unknown ones, so old and new bundles both work.
      Rebuild the vendored `web/dist/app.js` (easy to forget) and run
      `npm run verify`. Web smoke is not needed: nothing rendered changes.
- [ ] **P5b (Rust, changes the Codex ACP path and the wire):** remove
      `turn_usage` and `sessionTitle` production and the snapshot field.
      Skipped unless you want it: the browser deletion already removes the
      "dead code that looks live" cost, and P5b only saves server lines.

## Non-goals
- Codex ACP path changes.
- Nested sub-agent UI. Rows stay flat; there is no parent field on the wire.
- Rendering a session title or turn usage anywhere.
- Cursor context meter, since there is no public SDK data.

## Order and delivery
P0, then P1 (one PR per real defect; closures need no PR), then P2 + P4 in one
PR, then P3 (issue comments only), then P5 only if approved.

Each PR goes through the model router with sequential delegates, parent review of
the diff, `npm run verify` before `scripts/gh-pr-create`, and the PR body links
its issue. Web smoke tests are not needed: no layout changes.

## Risks
- P1 issue bodies are empty, so the delegate infers the expected behaviour from
  the title. Ambiguous ones get a written assumption in the issue before a fix.
- Stacked PRs: if #1337 changes before merge, the group-1 PR needs a rebase.
  Rebasing needs explicit approval.
- P2: sub-agent tool calls can run in parallel. The browser merges rows by
  call id, which is expected to be safe; the test covers interleaving.

## Validation log
| Date | Command | Result |
|---|---|---|
| 2026-10-18 | `npm run verify` (after #1333–#1336) | pass (3002 nextest, 1647 vitest) |
| 2026-10-18 | `npm run verify` (after #1338/#1339) | pass (3007 nextest) |
| 2026-10-18 | PR #1337 CI | all checks pass |
