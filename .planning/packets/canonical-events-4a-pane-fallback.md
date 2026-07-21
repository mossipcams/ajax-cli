# Packet: capability-gated pane wait fallback (Phase 4a)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Reintroduce a **narrow** structural pane fallback that can only emit
wait/ask hints when the client’s capability profile marks
`question_wait` / `permission_wait` as `Unavailable` or `Unverified`. Never
override native Lifecycle evidence. Do **not** restore Busy chrome
classification (that caused sticky false Running). Port only permission +
idle prompt chrome recognition from the deleted `live_recognize.rs` at
parent of `3199734`.

## Allowed files

- `crates/ajax-core/src/pane_fallback.rs` (new)
- `crates/ajax-core/src/lib.rs`
- `crates/ajax-core/src/live.rs` (wire into `select_status_observation` or a
  clear helper called from runtime_refresh)
- `crates/ajax-core/src/runtime_refresh.rs` (capture visible pane only when
  profile allows fallback AND no fresh lifecycle wait/working applied)
- `crates/ajax-core/src/agent_capability.rs` (only if a tiny helper needed)

## Forbidden changes

- Do not restore Busy footer → AgentRunning.
- Do not restore GenericPane/StructuredPane ObservationSource variants unless
  necessary — prefer ProviderHook or a single low-confidence path that
  applies WaitingForInput/Approval via existing apply_observation with short
  TTL (60s) and only when decision.applied is false after lifecycle/hook
  scan.
- Do not classify scrollback; visible pane only.
- No commits. No web UI edits.

## Context evidence

- Deleted recognizer: `git show '3199734^:crates/ajax-core/src/live_recognize.rs'`
  — use `recognize_claude_prompt` / `claude_permission_menu` /
  `recognize_codex_prompt` for IdlePrompt vs ApprovalPrompt only.
- Capabilities: `agent_capability.rs` `allows_pane_fallback`.
- Cursor/Pi: permission_wait + question_wait Unavailable → fallback allowed.
- Claude: Native waits → `allows_pane_fallback` false → never pane-wait.
- Refresh loop after status decision: `runtime_refresh.rs` ~342–420; pane
  capture historically nearby (search `capture-pane` / list panes).

## Code anchors

1. `pane_fallback.rs`:
   - `PaneWaitHint { WaitingQuestion, WaitingPermission }`
   - `recognize_wait_hint(agent: AgentClient, visible_pane: &str) -> Option<PaneWaitHint>`
     — port chrome-only permission/idle detection; no Busy; no stream-json
     Busy.
   - Map ApprovalPrompt→WaitingPermission, IdlePrompt→WaitingQuestion.
2. Gate: `profile_for_agent_client(agent).allows_pane_fallback(...)` —
   permission hint requires permission_wait fallback allowed; question hint
   requires question_wait fallback allowed.
3. `runtime_refresh`: if `!decision.applied` (and not acknowledged_hold), and
   profile allows, capture visible pane text (reuse existing tmux capture
   helpers if present — search `capture_pane` / `CommandSpec`), call
   `recognize_wait_hint`, apply `LiveObservation` WaitingForInput or
   WaitingForApproval with summary via `apply_observation` (not authoritative
   lifecycle), observed_at=now.
4. Tests in pane_fallback.rs from old claude_permission_menu /
   idle prompt cases; plus gate test that Claude agent returns None from
   gated helper even when pane shows permission chrome.

## Test-first instructions

Red: `cargo test -p ajax-core pane_fallback -- --nocapture`

1. `claude_permission_chrome_is_waiting_permission`
2. `cursor_other_idle_prompt_is_waiting_question` (AgentClient::Other)
3. `gated_fallback_skips_when_claude_has_native_wait` — public
   `maybe_pane_wait(agent, pane)` returns None for Claude even with
   permission chrome.
4. If runtime_refresh wired: one unit/integration test with fake pane capture
   is ideal but not required if wiring is thin and pane_fallback is fully
   tested — prefer at least one refresh test if anchors allow without huge
   fixtures.

## Edit instructions

Copy the smallest chrome helpers from the deleted file; delete Busy and
stream-json busy paths. Wire refresh conservatively.

## Verification commands

```bash
cargo test -p ajax-core pane_fallback
cargo test -p ajax-core agent_capability
cargo test -p ajax-core live
cargo clippy -p ajax-core --all-targets -- -D warnings
cargo fmt -p ajax-core -- --check
```

## Acceptance criteria

- Pane can produce wait/ask only when capability allows.
- Claude native profile never uses pane wait.
- No Busy→Running from pane.

## Stop conditions

- Cannot find tmux capture without large refresh rewrite — ship
  `pane_fallback` module + gated API + tests, wire a clearly marked TODO
  call site only if refresh integration exceeds scope; prefer full wire.
- Patch > ~400 lines.
