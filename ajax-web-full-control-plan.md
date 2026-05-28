# Ajax Web Full-Control Plan

Branch: `ajax/ajax-web-full-control`

## Goal

Eliminate every reason an operator currently opens SSH alongside the Web Cockpit. The PWA must close the loop for the two highest-frequency SSH drivers:

1. **Watching the agent work** — operator needs to see what Codex/Claude is currently doing without attaching to tmux.
2. **Typing back to the agent** — operator needs to approve commands, answer prompts, and send free-form input.

The framing is **cockpit, not terminal**. We do not render a scrolling monospace pane or pass through ANSI escapes. We surface structured operator affordances — status pills, command cards with Approve/Deny, prompt cards with focused input, and a clean activity log card.

Out of v1 scope (kept here so we don't lose track):

- Inline diff review (deferred per operator choice).
- Full xterm.js web terminal + raw PTY broker (deferred; only revisit if v1 doesn't eliminate SSH).
- Custom branch/base on new task, file browsing, ad-hoc shell.
- Supervisor `MonitorEvent` → web event bus.

## Why this scope works (and what was wrong with v0)

Holes poked during planning that reshaped the design:

| Original assumption | Reality found in the codebase | Adjustment |
|---|---|---|
| Live agent stream comes from the supervisor's `MonitorEvent` channel. | `spawn_monitor` only runs for the one-shot `ajax supervise` flow. Long-running tasks run inside tmux unsupervised. `cockpit_backend.rs` already uses `tmux capture-pane` to compute the live summary. | Source of "what's the agent doing" is `tmux capture-pane`, not the supervisor. No new event bus. |
| Need a heuristic to detect "needs input". | `AgentEvent::WaitingForInput { prompt }` and `WaitingForApproval { command }` already exist; `live::classify_pane` already maps pane text to `LiveStatusKind`. | Use what exists. |
| PTY input bridge framed as a "light terminal". | The realistic mechanism is `tmux send-keys`. Native Cockpit's `task_session.rs` uses `forkpty` + raw mode — too heavy to port to a browser in this phase. | Honest framing: a contextual input bar that sends keys to the tmux session. |
| Three-tab detail view (Activity / Diff / History). | Diff and History deferred per operator priority. | Single inline interact panel — no tabs. |
| Auto-promote `OperatorAction::Resume` to `supported` on web. | Core's `Resume` = `tmux attach-session`; web "supported" would lie about parity with native. | Keep `Resume` blocked in the operate slice. Pane view is reached by tapping the card (existing behavior). |
| Render pane content as a scrolling terminal. | Operator wants cockpit feel, not terminal. | Structured affordances; clean activity log; no ANSI rendering. |
| Frontend follows AGENTS.md TDD. | TDD in AGENTS.md is Rust-only. `app.js` has no cargo test rig. | Frontend verification is manual demo + screenshot review; backend keeps strict TDD. |

## Architecture fit

- `ajax-web::slices::pane` (new) — operator capability slice that exposes pane snapshot and tmux input.
- `ajax-core::slices::pane` (new, small) — builds `CommandSpec` for `tmux capture-pane` and `tmux send-keys`, parses output, holds the ANSI-strip + line-dedup policy. Mechanism only — no task-lifecycle authority.
- `ajax-web::adapters::tmux_input` (new) — thin command-builder using `CommandRunner`.
- `ajax-web::slices::operate` — unchanged. `Resume` continues to return `UnsupportedCapability`.
- `architecture.md` — one paragraph added under "Web Cockpit Architecture" describing pane/input slice + adapter.
- `crates/ajax-web/src/architecture.rs` — slice direction tests extended to permit the new slice and adapter.

## Endpoint contracts

### `GET /api/tasks/{handle}/pane?since={sequence}`

Returns a cleaned pane snapshot.

Response 200:

```json
{
  "sequence": 42,
  "lines": ["...", "...", "..."],
  "truncated": false,
  "tmux_exists": true,
  "state": {
    "kind": "WaitingForApproval",
    "summary": "approve 'cargo test'?",
    "command": "cargo test",
    "prompt": null
  }
}
```

- `sequence` is a monotonic counter per task on the server. Increments only when cleaned content changes.
- `lines` are ANSI-stripped, adjacent-duplicate-collapsed, last ~12 non-empty lines.
- `truncated` is true when more lines were captured than returned.
- `state` mirrors the live status with `command` (for `WaitingForApproval`) and `prompt` (for `WaitingForInput`) hoisted from the structured agent event when available, else parsed from pane text on a best-effort basis.

When `since` matches the current sequence, server returns 200 with `lines: []` and the same sequence — empty delta.

Response 409 when tmux session is missing:

```json
{ "tmux_exists": false, "sequence": 0, "lines": [], "state": null }
```

Response 404 when task not found.

### `POST /api/tasks/{handle}/input`

Sends keys to the task's tmux session.

Request:

```json
{ "keys": "...", "submit": true, "request_id": "uuid-v4" }
```

- `keys` — literal text, or one of the allow-listed tmux key tokens: `Enter`, `C-c`, `C-d`, `C-z`, `Up`, `Down`, `Left`, `Right`, `Tab`, `Escape`, `BSpace`.
- `submit: true` appends a trailing `Enter` after the literal text. `submit: false` sends the keys verbatim.
- `request_id` — client-generated UUID. Server de-duplicates by `(task_handle, request_id)` in a 30s in-memory LRU; a repeat returns the cached response without re-running send-keys.

Response 200:

```json
{ "sequence_hint": 43 }
```

`sequence_hint` is the server's current pane sequence at the moment the input was sent — the client polls until it observes a sequence ≥ this to know its keystroke landed.

Response 409 when tmux session is missing.

Response 429 when rate limit exceeded (max 10 inputs per task per 5s).

## Backend (Codex)

All work uses TDD per AGENTS.md. Each task ships as a single PR with a `feat(web):` title.

### B1 — `ajax-core::slices::pane`

- `pane::snapshot(session, since, limit)` → typed `PaneSnapshot` from `CommandRunner`.
- `pane::send_keys(session, keys, submit)` → typed `SendKeysOutcome`.
- ANSI strip and line-dedup live here (so policy is in core, not the web adapter).
- Tests: command building, ANSI strip cases, dedup cases, key allow-list rejection.

### B2 — `ajax-web::slices::pane`

- `GET /api/tasks/{handle}/pane` handler.
- `POST /api/tasks/{handle}/input` handler with rate limit + request-id de-dup.
- Tests for happy path, missing session, missing task, rate limit, de-dup.

### B3 — `ajax-web::adapters::tmux_input`

- Wraps `CommandRunner` for tmux `send-keys` calls.
- Tests verify constructed `CommandSpec` matches the documented vocabulary.

### B4 — Architecture test + docs

- Update `crates/ajax-web/src/architecture.rs` to permit the new slice + adapter.
- Add paragraph to `architecture.md` (Web Cockpit Architecture section).

### B5 — Required validation before merge

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo nextest run --all-features`

## Frontend (Claude)

All work in `crates/ajax-web/web/{index.html, app.css, app.js}`. No new dependencies. No IndexedDB. Mobile-first.

### F1 — Cockpit interact panel (new)

Added to the top of `#task-detail`, above the existing live-status grid. Renders entirely from `detail.live_status_kind` plus the new pane payload.

States and what they show:

- **`WaitingForApproval`** — yellow status pill `Needs your approval`, command card with the `command` text in a code block, Approve and Deny buttons. Approve posts `{"keys": "y", "submit": true}`. Deny posts `{"keys": "n", "submit": true}`.
- **`WaitingForInput`** — yellow status pill `Asking you`, prompt card with the `prompt` text, focused input bar with `enterkeyhint="send"`.
- **`AgentRunning` / `CommandRunning` / `TestsRunning` / `Thinking`** — blue status pill `Working`, no special CTA. Input bar still visible at the bottom of the panel for free-form interjection.
- **`CommandFailed` / `Blocked` / `AuthRequired` / `RateLimited`** — red status pill with the failure summary; input bar still visible.
- **`Done`** — green status pill `Idle`; input bar still visible (operator may want to resume).
- **`MergeConflict` / `CiFailed`** — red pill with remediation hint; remediation buttons already exist in the action drawer, panel doesn't duplicate them.
- **No tmux session** — empty state `Task tmux session is gone — sync to recover.` Input bar hidden.

### F2 — Cockpit activity log card

A compact card below the state row showing the last cleaned lines from `pane.lines`.

- Rendered as a vertical list with a subtle row separator. **Not** monospace; not styled as a terminal.
- Latest line at the bottom.
- Sticky-to-bottom autoscroll when the user is already at the bottom; "Pinned" pill when they scroll up.
- Empty state `Pane is quiet.` when `lines` is empty.
- Optimistic-echo entries: prefixed with a paper-plane glyph, faint color, `Sent` label. Cleared as soon as a new pane sequence arrives whose contents differ from before the send; after 5s with no change, annotated `unconfirmed`.

### F3 — Input bar

Anchored to the bottom of the interact panel.

- `<form class="interact-input">` with a text `<input>` and a Send button.
- Quick-action buttons next to Send: `Enter`, `Ctrl-C`. These are always present.
- Submit posts `POST /api/tasks/{handle}/input` with `{"keys": text, "submit": true, "request_id": uuid}`.
- Enter button posts `{"keys": "Enter", "submit": false, "request_id": uuid}`.
- Ctrl-C posts `{"keys": "C-c", "submit": false, "request_id": uuid}` and requires a tap-to-confirm following the existing `confirming` pattern.
- iOS: `inputmode="text"`, `enterkeyhint="send"`, autocomplete/autocorrect/spellcheck off.
- After successful send: input cleared, focus retained, optimistic echo appended to activity log.
- On 429: show "Slow down — too many inputs" in the result panel; do not retry automatically.
- On 409 (session gone): show "Task tmux session is gone" in result panel; hide input bar until next poll succeeds.

### F4 — Pane polling loop

- New `loadPane()` parallel to `loadDetail()`, owned by detail view.
- Cadence: 1000ms default; 250ms while input bar is focused; paused on `visibilitychange` to hidden.
- Always sends `?since={paneSequence}`; updates local sequence from response.
- Reconciles state into the interact panel without rebuilding the whole detail view.
- On network error: existing `setOnline(false)` path; pane content fades to dim, status pill becomes `Offline`.

### F5 — Detail view structural changes

- Live-status grid moves below the interact panel (it duplicates the pill).
- The `agent_activity` excerpt block is hidden when the pane is available; shown as fallback only when pane fetch fails.
- Recent attempts, branch, agent sections unchanged.

### F6 — Cards: surface "needs input"

- When card `live_status_kind` is `WaitingForApproval` or `WaitingForInput`, card body shows the prompt/command in the existing summary line (already in `live_status_summary`).
- Existing `is-attention` indicator applies; no new card affordance.
- Tapping the card already routes to detail — no change.

### F7 — Service worker

- `/api/tasks/{handle}/pane` and `/api/tasks/{handle}/input` are live data and must never be cached. Verify they fall under the existing `/api/*` cache bypass in `sw.js`. No change expected.

## Sequencing

1. **B1** (`ajax-core::slices::pane`) — Codex.
2. **B2** (`ajax-web::slices::pane`) + **B3** (`tmux_input` adapter) — Codex. After this, the contract is live and Claude can verify F4 in dev.
3. **F1–F7** — Claude. Can begin in parallel as soon as the contract is stable (endpoints can 404 during development; the frontend handles that path).
4. **B4** (arch test + docs) — Codex, alongside B2/B3.
5. **B5** (validation) — Codex, before each PR.

## Verification

### Backend

Each PR runs:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
```

### Frontend

Manual demo (the codebase has no JS test rig):

- iOS Safari standalone PWA + desktop Chrome.
- Start a Codex task → interact panel shows `Working` → activity log fills.
- Drive an approval flow → `Needs your approval` state appears → tap Approve → agent continues, activity log reflects within ~1s.
- Drive a `WaitingForInput` flow → prompt visible → type response → land on next poll.
- Kill the tmux session → empty state appears, input bar hides.
- Toggle to a Done task → green pill, input bar still usable.

Screenshots attached to the frontend PR.

## Open risks (post-decision)

- **Codex TUI pane parsing fidelity.** Codex draws an alternate-screen TUI. `tmux capture-pane -p -e` includes the alt-screen contents but interpretation varies between TUI redraws. Server-side dedup + last-N strategy will likely show the right thing for a "working" state, but the first PR for B1 should include a manual check against a real Codex session before locking the format.
- **Activity log feels stale during heavy redraw.** If Codex repaints frequently, the deduped lines may oscillate. Acceptable for v1 — the status pill carries the operator's most-important signal anyway. Revisit if the operator reports the log feels confusing.
- **Multi-tab two browsers.** Both polling and sending input is fine semantically (de-dup keys on request_id). Conflicting input is the operator's problem; acceptable for v1.
