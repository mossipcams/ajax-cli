# Plan: web new-task terminal blank space + empty-registry Drop save

## Scope

Two independent Web Cockpit defects reported 2026-07-13:

1. **New-task terminal blank band** — after agent-sized scale-to-fit (#440/#442),
   the phone terminal opens with a large empty region (CSS scale shrinks the
   canvas but row count still matches the unscaled host height).
2. **Drop save refusal** — `ajax web` / context save fails with
   `refusing to save empty registry over non-empty loaded state; authorize
   delete-all before saving` because the web bridge never sets
   `allow_empty_registry_once` on Drop (CLI and native Cockpit already do).

Non-goals: wterm migration, registry semantics beyond authorizing intentional
last-task Drop wipe, unrelated terminal chrome.

## Root causes (inspected)

### 1 — scale without row compensation

`fitNow` sets `cols = logicalCols(hostFit)` (≥80) then `rows = logicalRows(proposed.rows)`
(host-fit row count), then `applyTerminalScale()` with `scale = hostWidth / (cols * cellWidth) < 1`.

Visual height ≈ `rows * cellHeight * scale` ≈ `hostHeight * scale` → blank band
of height `hostHeight * (1 - scale)` under the canvas. Most obvious on fresh
new-task mounts. Prior PRs fixed column soft-wrap and expand hit-targets, not
vertical fill.

### 2 — web Drop missing empty-wipe authorization

`cockpit_backend` and CLI `drop --execute` call `allow_empty_registry_once()`
before save. `CliRuntimeBridge::persist_operate` / `persist_changed_state` never
do, so dropping the last persistable task (or any path that leaves the
in-memory registry empty after a non-empty load) fails the empty-overwrite
guard.

## Delegation decision

`Delegation decision: delegated via model-router` — two sequential bounded
behavior changes, each with its own packet + review gate.

Order: **terminal blank band first** (user-visible on every new task), then
**web Drop empty-registry authorize**.

## Approval

User reported both defects and asked for fixes — authorized to implement.

## Task checklist

### T1 — scale-compensated logical rows

- [x] Packet: `.planning/packets/web-scale-row-compensation.md`
- [x] Critique PASS
- [x] Test: geometry helper + TerminalRawView narrow-host resize rows increase
- [x] Impl: `scaledLogicalRows` (or equivalent) in `terminalGeometry.ts`; use in `fitNow`
- [x] Verify focused web tests + web:check (parent re-ran: 180 pass, 0 svelte errors)
- [x] Optionally size `.terminal-scale-layer` to 100% so FitAddon measures the host

### T2 — web Drop authorizes empty wipe

- [x] Packet: `.planning/packets/web-drop-empty-registry.md`
- [x] Critique PASS
- [x] Test: web bridge Drop of last task persists empty registry (prior round); this round replaced `OkRunner` with `AbsentDropRunner` (substrate absent), proved RED on `refusing to save empty registry`, then GREEN after fix
- [x] Impl: in `CliRuntimeBridge::execute_operate`, when `request.action == OperatorAction::Drop.as_str()` and the operate result will persist (`Ok(outcome) if outcome.state_changed` or `Err(OperateError::Command(_, true))`), call `self.save_state.allow_empty_registry_once()` before `persist_operate`
- [x] Verify focused ajax-cli web_backend tests

## Validation

```bash
# T1
rtk npm run web:test -- --run src/terminalGeometry.test.ts src/components/TerminalRawView.test.ts
rtk npm run web:check

# T2
rtk cargo nextest run -p ajax-cli web_bridge_drop_of_last_task_persists_empty_registry
rtk cargo nextest run -p ajax-cli web_bridge_rejects_empty_save
rtk cargo check -p ajax-cli
```

## Deviations

- Prior round's `AbsentDropRunner` requirement: pre-existing test used `OkRunner`, which always reported the task worktree/branch present, so Drop ended in `TeardownIncomplete` and never reached the empty-registry persist path. Replaced with a test-only `AbsentDropRunner` that reports the `/repo/web`-only worktree, `main`-only branch list, and empty tmux sessions, so Drop completes to `Removed` and hits the empty-registry save guard.

## Validation results

- T1 focused vitest (parent): 180 passed
- T1 web:check (parent): 0 errors / 0 warnings
- T2 RED (delegate, AbsentDropRunner, pre-fix): `refusing to save empty registry…`
- T2 GREEN (parent re-ran):
  - `web_bridge_drop_of_last_task_persists_empty_registry` — pass
  - `web_bridge_rejects_empty_save` — pass
  - `cargo check -p ajax-cli` — pass
- Checklist: all T1/T2 items complete
