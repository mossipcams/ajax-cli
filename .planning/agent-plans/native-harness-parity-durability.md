# Native harnesses: finish alignment, ACP parity, in-session durability

## Status

**APPROVED 2026-10-14** (Matt answered Q1–Q4, see Decisions). Written 2026-10-14 against `origin/main` @ `1ba0de1b`.
Planning only; nothing implemented. Supersedes the open checklist items in
`native-harness-handoff.md` (rev 2) and tasks 1–6 of `native-acp-parity.md`.
Durability stages 5 and 12 of `durable-execution-tmux-removal.md` are scheduled
here; stage 8 stays in that plan.

## Where main stands (verified in source)

- Merged: #1289 orphan sweep, #1290–#1293 Cursor SDK stack, #1294 Cursor flip,
  #1295 contract suite (P1), #1296 Claude usage fix.
- **#1294 merged out of order**: before P2 (dedupe), before P3 (process groups),
  and before the live spike. Its PR body still says "Draft until the live spike
  passes" and `npm run verify` is unticked.
- Rev-2 invariants 2 and 3 are not met: three `uses_*` selectors drive
  if-chains (`session_client.rs:181`, `catalog.rs:78`); every shared method still
  has four arms; commands one-shot copied ×3; `#![allow(dead_code)]` remains
  on `session_client.rs`; Pi/Claude test overrides are still `thread_local!` (T1).
- `harness_contract_tests.rs` has rows for ACP, Pi, Claude only, so there's no Cursor row.
- `read_cursor_catalog_if_sdk` (`session_models.rs:129`) survives the flip (L1).
- Docs still describe Cursor as ACP: `web-cockpit.md` 31/181/194/206/293,
  `web-session-behavior.md` 4/51; `durable-execution.md` §1 omits Cursor SDK.

### Correction to the 2026-10-14 review

The review said Cursor lost browser permission prompts. Wrong: the host
auto-approves every ACP `session/request_permission` and forces the full-access
mode (`web-session-behavior.md` "Permission persistence", line ~689). Policy
is "auto-approve" for every harness; Claude's sidecar `canUseTool` and the
prompt-free Cursor/Pi transports already match it. `@cursor/sdk` 1.0.37 has no
interactive approval callback anyway (`options.d.ts`: only `sandboxOptions`,
`autoReview`). **Permission parity is met; no work scheduled.**

## Scope

1. Finish the rev-2 architecture (P2, Cursor contract row, L1, docs).
2. Close the ACP parity gaps each harness *can* close.
3. Durability: recovery not gated on a browser attach, process-group cleanup,
   and a host kill/restart suite that proves it.

## Non-goals

- New traits, factories, registries, or `Capabilities` structs (rev 2 verdict stands).
- Changing the auto-approve permission policy.
- Terminal/tmux (durability stages 9–11 stay parked); status projection (stage 8).
- Unifying `AcpClientEvent` and `NativeEvent` vocabularies.
- Message ids / content blocks on native messages, session titles, Cursor
  context meter. Recorded below as "not scheduled"; add when the UI needs them.

## Parity matrix (target after this plan)

Codex ACP is the reference. ✅ parity, ➕ closed by this plan, ✖ harness cannot,
— not scheduled.

| Feature | Claude SDK | Pi RPC | Cursor SDK | Evidence |
| --- | --- | --- | --- | --- |
| Text, thinking, tools, cancel, fail-closed restore | ✅ | ✅ | ✅ | contract suite (Cursor row ➕ W1) |
| Permissions (auto-approve policy) | ✅ | ✅ | ✅ | see correction |
| Images | ➕ W3b | ➕ W3a | ✅ | Pi `prompt.images` (`docs/rpc-commands.md`); Claude SDK streaming input image blocks; `rpc_client::begin_prompt_with_images` exists |
| Diff tool content | ➕ W3c | ➕ W3c | ➕ W3c (after spike) | `native_map::native_tool_content` is text-only |
| Plan / todos | — dropped (Q2) | ✖ no todo tool | ✅ | `claude_sdk_map.rs` emits no `Plan` |
| Elicitation / dialogs | ➕ W3f | ➕ W3e | ✖ no SDK hook | Claude `onElicitation` declines; Pi `extension_ui_request` ignored |
| Usage meter | ✅ | ✅ | ✖ no context size (`TokenUsage` has tokens only) | `usage-types.d.ts` |
| Commands | ✅ (stale-on-swap ➕ W3g) | ✅ | ✅ | `acp_slot.rs:35` never resets |
| Modes | — dropped (Q2) | ✖ | ✅ agent/plan | |
| Title, message ids | — | — | — | |

## Workstreams and PRs

Each PR is ≤600 changed lines/file and ≤1500 lines total, and updates its owning doc. No PR adds lines to
`sdk_connection.rs` (938), `client.rs` (913) or `config_options.rs` (816).

### W0. Live spike on the Cursor SDK login (gate for W3c-Cursor; retroactive for #1294)

Live-host op on the ajax-web host user (already logged in). Record in
`cursor-native-sdk.md`: `tool_call.args` shape for edit/write tools (needed for
diffs), `agentId` stable across turns, `Agent.resume` round trip, `run.cancel()`,
`send("/cmd")` expansion of `.cursor/commands/*.md`, usage message presence.
Any failure → GitHub issue (it's production now) before W3c-Cursor.

### W1. Close-out PR: docs + Cursor contract row + plan hygiene

- Add the Cursor row to `contract_rows()` (one entry; fixture `fake_cursor_sdk.mjs`
  exists; file is under `src/`, not `tests/`).
- Rewrite the Cursor-as-ACP passages in `web-cockpit.md` and
  `web-session-behavior.md`; Cursor auth line (SDK login, 90-day key, compose
  mount); add Cursor SDK to `durable-execution.md` §1.
- Mark `native-harness-handoff.md` and `native-acp-parity.md` superseded by this file.
- Acceptance: contract suite green with 4 rows; rustdoc `-D warnings`.

### W2. P2 dedupe (no behaviour change), as specified in rev 2, split in two

- **W2a selection**: one `transport_for(agent) -> HarnessTransport` in
  `session_client.rs`; spawn and `read_agent_model_catalog` `match` on it
  exhaustively; one process-wide static test override keyed by
  `HarnessTransport` (fixes T1 for Pi/Claude); delete `uses_*`; fold
  `read_cursor_catalog_if_sdk` into the catalog `match` (L1).
- **W2b shared arms**: private `transport()` accessor so the 10 shared methods
  have two arms; commands one-shot into `RpcClientCore`; `sidecar_launch_args`
  + init parse into `rpc_handshake.rs`; `global_npm_root` into `program.rs`;
  drop `#![allow(dead_code)]` where clean.
- Acceptance: all existing tests green **unmodified**; zero `uses_*`; net lines
  negative; new guard assertions for invariants 2 and 3 in
  `architecture_web_session.rs`.

### W3. Parity packets (shared layer first, one packet each)

- **W3a Pi images**: Pi arm of `begin_prompt` uses `prompt_parts` +
  `begin_prompt_with_images`; Pi `SpawnReport.prompt_capabilities.image = true`.
  Test: fixture records the `images` field (fixture edit approved, Q4).
- **W3b Claude images**: sidecar sends image content blocks via the SDK's
  streaming input; Claude report `image = true`. Fixture edit approved (Q4).
- **W3c Diff content**: `NativeEvent::ToolCallUpdate.content` becomes
  `Vec<NativeToolContent>` (`Text`, `Diff { path, old_text, new_text }`);
  `native_map` maps it to `ToolContent::Diff`. Mappers: Claude
  `Edit`/`MultiEdit`/`Write` input; Pi `edit`/`write` args; Cursor after W0.
  One packet for the shared type, one per mapper.
- ~~W3d Claude plan~~: dropped (Q2).
- **W3e Pi dialogs**: first a regression test: a Pi fixture emitting a
  `confirm` `extension_ui_request` without `timeout`; does the turn hang?
  If yes: GitHub issue (defect), then route `select`/`confirm`/`input` onto the
  existing elicitation wire (`sdk_elicitation`) and answer with
  `extension_ui_response`; `respond_elicitation` gains a Pi arm.
  Fire-and-forget methods stay ignored.
- **W3f Claude elicitation**: sidecar `onElicitation` forwards to the host and
  waits; Claude arm of `respond_elicitation`. Cancel resolves pending as declined.
- **W3g Commands on swap** (known edge): reset
  `AcpSlot.session_available_commands` when the client is replaced. Defect →
  issue + regression test.
- Acceptance per packet: contract suite capability rows updated to the matrix
  above (this is the parity guard that replaces `native-acp-parity.md` task 5).

### W4. Durability

- **W4a Process groups (P3)**: regression test first (fixture child spawns a
  grandchild; `shutdown()` and a foreign-boot sweep). If the grandchild
  survives → issue, `process_group(0)` on `JsonlProcess` and the ACP spawner,
  group kill on drop, sweep signals the group when the leader token matches.
  If not → close, drop the `ponytail:` upgrade note in `orphan_sweep.rs:81`.
- **W4b Recovery at host start**: after the owner lease is claimed and the
  sweep runs, scan runs with unfinished prompt tasks and apply the existing
  restart rule (dispatching → `interrupted` + transcript note; queued kept)
  **without spawning an agent**. Today this only happens inside `acquire`
  (`task_session_spawn.rs:84`), so a restart leaves status and transcript
  stale until someone opens the chat. Reuse `recover_after_restart`; the
  `acquire` path stays idempotent (nothing left to interrupt).
- **W4c Headless dispatch of queued prompts** (decision Q1): on host start,
  spawn the harness (resume by stored id, fail closed) and dispatch queued
  prompts in order, concurrency 1. Changes runtime authority (agents start
  with no browser attached): **approved (Q1)**, lands only after W4d is green;
  update `durable-execution.md` §5/§6 and `web-session-behavior.md` with it.
- **W4d Crash suite (stage 12)**: host-level test: start the web session host
  on each fixture harness, admit two prompts, kill the host (SIGKILL) mid-turn,
  restart, assert: transcript intact to the last committed batch; dispatching
  prompt `interrupted`, never resent; queued prompt dispatched exactly once (on
  attach, or at start if W4c); native session resumed with the same id; old
  child (and group, after W4a) gone; lease taken over only after staleness.
- ~~W4e Pi reconcile~~: rejected (Q3). Dispatched prompts stay `unsafe` →
  `interrupted` for every harness.

## W0 results (2026-10-14, `@cursor/sdk` 1.0.37, SDK login, model `default`/Auto)

Run from a scratch dir outside the repo (`/tmp/w0-spike/spike.mjs`), one local
agent, 3 turns. Nothing in the repo changed.

| Question | Result |
| --- | --- |
| `agentId` stable across turns | Yes, same id for turns 1–2 |
| `Agent.resume(id, {local, model})` | Works, returns the same id |
| `run.cancel()` mid-turn | Run ends `status: "cancelled"` in ~4 s |
| Event kinds | `status`, `thinking`, `assistant`, `tool_call`, `usage` (one per turn at the end) |
| Usage shape | `{inputTokens, outputTokens, cacheReadTokens, cacheWriteTokens, totalTokens, reasoningTokens}`: no context size, confirms ✖ for the meter |
| Edit tool call | `name: "edit"`, `args: {path}` **only**; the change is in `result.value.diffString` (unified diff, `linesAdded`/`linesRemoved`) |
| Read tool call | `name: "read"`, `args: {path}`, `result.value.content` |
| Models | `Cursor.models.list()` returns `{id, displayName, aliases, parameters:[{id, values}]}` or `variants` |
| `.cursor/commands` expansion via `send("/cmd")` | **Not exercised**; still unknown |

Consequences:

- **W3c Cursor** maps the diff from `result.value.diffString`, not from `args`.
  `ToolContent::Diff` wants `oldText`/`newText`, so either parse the unified
  diff into hunks or carry the diff as `Text`; decide at W3c with the mapper
  tests. Claude and Pi carry real old/new strings in their edit inputs.
- **Account state, not a defect**: with `grok-4.7` the first run failed with
  "You're out of usage. Switch to Auto" (`status: "error"`, 7 s). The same
  agent on Auto worked. The Ajax default Cursor model is a Grok id, so chat
  on this account would show that error until usage resets or the model is
  switched; confirm the error text reaches the browser (add to the W4d/W3
  checks, no issue opened).

## Order

W1 → W0 (spike, parallel) → W2a → W2b → W4a → W4b → W4d → W3g → W3e → W3a →
W3c (shared, Claude, Pi; Cursor after W0) → W3b → W3f → W4c.

Why: docs and the 4th contract row first so every later PR is guarded; dedupe
before features so new arms land once, not four times; durability correctness
(W4a/b/d) before parity polish; defects (W3g, W3e) before features.

## Decisions (Matt, 2026-10-14)

- **Q1 yes**: a restarted host spawns agents and dispatches queued prompts with
  no browser attached (W4c), after W4d is green, concurrency 1.
- **Q2 no plan feature**: no Claude `plan` mode and no TodoWrite → plan
  mapping (W3d dropped). Cursor's existing agent/plan mode is left as is.
- **Q3 no**: no Pi reconcile (W4e dropped).
- **Q4 yes**: fixture edits under `tests/fixtures/` are approved for W3a, W3b,
  W3e, W3f, W4a, W4d. Existing assertions are still never weakened or deleted.

## Delegation

Per `AGENTS.md`: model-router, Qwen in small packets (one module per packet,
inline signatures, "do not explore", ban `git stash/checkout/reset/restore`).
Parent reviews every `session_client.rs` hunk (W2b) and every recovery hunk
(W4b–W4d). Line-multiset diff for any moved code in W2.

## Checklist

- [x] Approval of this plan and answers to Q1–Q4 (2026-10-14).
- [x] W0 spike recorded (2026-10-14, see W0 results). `/cmd` expansion not exercised.
- [x] W1 close-out PR (W1a contract row, W1b/W1c docs, W1d plan hygiene done on branch chore/web/native-harness-closeout; PR pending).
- [ ] W2a done (branch refactor/web/transport-selection), W2b done (branch refactor/web/shared-native-arms).
- [ ] W4a done (#1298, branch fix/web/harness-process-groups), W4b, W4d.
- [ ] W3g, W3e (repro → fix or close), W3a, W3c ×4, W3b, W3f.
- [ ] W4c (after W4d).

## Deviations

- 2026-10-14: permission gap withdrawn (see Correction).
- 2026-10-14: W3d and W4e dropped, W4c approved (Decisions).
- 2026-10-14: worktree branch ajax/new-adapters was stale (base #1291, 5 behind main, 2 commits duplicating #1295); work continues on stacked branches cut from origin/main starting at chore/web/native-harness-closeout.
- 2026-10-14: W2a deleted the uses_* selectors, so the 5 gating tests that called them were mechanically rewired from uses_x(a) to transport_for(a) == X (same assertion count and strength); the 'tests unmodified' acceptance could not hold literally.

## Validation

```
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run -p ajax-web -p ajax-core --no-fail-fast
pkill -f vite; CI=1 npm run web:smoke   # any PR touching chat UI wiring
npm run verify                          # PR gate
```

No results yet.
