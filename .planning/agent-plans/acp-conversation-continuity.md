# Plan: ACP conversation continuity (#1179)

## Problem

Ajax Chat has had the same destructive failure several times (#1061, #1099
reverted, #1149, #1151, #1179): the JSONL transcript stays, the model context
is thrown away. Each patch closed one gate. Reconnect and internet drop-off
still treat **model-string inequality** as “this is a different conversation.”

A live healthy ACP child is supposed to be a **lease**. Restore exists only
when that child is gone. Model is desired config, applied in-band. Drop,
Switch, `/clear`, and explicit Start fresh are the only terminal ends.

## Decision

A stored ACP session id, or a live child for that handle, **is the same
conversation** until Drop, cross-harness Switch, `/clear`, or Start fresh.

- WebSocket drop / reconnect / internet blip: reattach the live child. Do not
  replace, resume, `session/close`, or `session/new` because `want_model` differs
  from the slot pin.
- Child gone (idle eviction, `ajax-web` restart, unexpected death): restore the
  stored id (`session/resume` / `session/load`). Apply the operator pin in-band
  on the restored session. Never silent `session/new`.
- Model pin spelling is not conversation identity. It must not appear in
  `slot_must_replace`.

This resolves the `web-cockpit.md` #979 “dead child → `session/new` (no
resume)” sentence in favor of the #1151 restore contract already in
`web-session-behavior.md`.

## Scope

- `docs/architecture/web-session-behavior.md`
- `docs/architecture/web-cockpit.md`
- `crates/ajax-web/src/slices/web_session/transcript.rs` (`slot_must_replace`)
- call sites in `task_session_spawn.rs`
- focused tests (`transcript_tests.rs`, `task_session_restore_tests.rs`)
- keep the existing #1179 `replace_resume_id` restore-on-mismatch change

## Non-goals

- New continuity enum, extra modules, or a rewrite of acquire/respawn.
- Changing Drop, Switch, `/clear`, Start fresh, idle grace duration, or
  fail-closed restore timeouts.
- Browser WebSocket cursor/replay behavior.
- Task lifecycle, registry truth, or ACP permission mode.

## Approval

User requested immediate implementation (2026-09-17): “Address the issue
architecturally, we should not be having consistent destructive defects.”

## Tasks

- [x] T1 — State the continuity invariant in `web-session-behavior.md`
      (lease vs restore vs explicit fresh). Stop describing “same-model slot
      replacement” as the close/resume discriminator.
- [x] T2 — Align `web-cockpit.md`: restore never falls through to silent
      `session/new`; dead-child pin recovery does not `session/new` when a
      stored id exists; close policy matches T1.
- [x] T3 — `slot_must_replace` is only child-health (`!acp_alive ||
      host_exited`). Pin mismatch does not replace a live child.
- [x] T4 — Regression: live healthy child, reconnect acquire with a different
      `want_model` → lease (same session id, `session/new` count stays 1, no
      context-reset note). Name #1179.
- [x] T5 — Keep/confirm dead-child + pin mismatch still restores (existing
      `issue_1179_reconnect_model_mismatch_still_restores_without_context_reset_note`).
- [x] T6 — Switch / Drop / `/clear` still start fresh. `session_close` tests
      stay green.

## Validation

```bash
rtk cargo test -p ajax-web --lib -- --test-threads=1 slot_must_replace issue_1179 session_close task_session_restore
rtk cargo test -p ajax-web --lib -- --test-threads=1 web_session::transcript
```

## Results

- `slot_must_replace(acp_alive, host_exited)` — model params removed; pin
  mismatch no longer replaces a live child.
- `replace_resume_id` unchanged: stored id restores on slot replacement.
- New test `issue_1179_live_child_model_mismatch_leases_without_context_reset_note`.
- Docs: continuity invariant section in `web-session-behavior.md`; dead-child
  restore and close policy aligned in both architecture docs.
- Docs revision (parent review): `web-cockpit.md` no longer says restore falls
  through to `session/new`; both architecture docs now state stored id always
  means restore and missing model control is a typed pin-apply error, not a
  `session/new` gate. Dropped duplicate assert in
  `slot_must_replace_only_when_child_is_unhealthy`.

## Deviations

- Parent review caught stale silent-reset wording in `web-cockpit.md` (restore
  “then creates a new session”) and a `#979`/`#1151` regression sentence tying
  `session/new` to “no model control” when a stored id exists. Corrected in
  docs only; T3/T4 code unchanged.
