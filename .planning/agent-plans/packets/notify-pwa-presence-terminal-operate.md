PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Refresh Web Cockpit "browser connected" presence when the operator is actively
using the PWA via terminal WebSocket or operate/actions — not only via
`GET /api/cockpit` — so the background notify tick stays suppressed while they
work in the terminal.

## Allowed files

- `crates/ajax-web/src/runtime.rs`
- `architecture.md`

## Forbidden changes

- Do not change `BROWSER_CONNECTED_TTL` (90s) or notify tick period logic
  beyond calling the existing `mark_browser_cockpit_seen`.
- Do not change attention/live classifiers (Round A already landed).
- Do not suppress CLI/TUI notify paths.
- No frontend SPA changes; no renames or formatting sweeps.

## Context evidence

- Desired: while using the PWA terminal or submitting operate actions, webhook
  notify tick must see `browser_connected() == true`.
- Today only `axum_cockpit` marks presence (`runtime.rs:837`). Tick skips when
  connected (`runtime.rs:477-479`). Architecture (`architecture.md:682-684`)
  documents cockpit-only presence.
- Gap: `axum_task_terminal` (`runtime.rs:947`) and `axum_action`
  (`runtime.rs:1121`) never mark. Cockpit polls can stall while the terminal
  WS stays alive → TTL expires → tick delivers.
- Pattern: `state.mark_browser_cockpit_seen()` already used by cockpit; tests
  `axum_cockpit_marks_browser_connected_even_on_cache_hit` and
  `browser_connected_is_false_until_marked_and_expires_after_ttl`.

## Code anchors

- `crates/ajax-web/src/runtime.rs:180` — `mark_browser_cockpit_seen`
- `crates/ajax-web/src/runtime.rs:837` — cockpit mark site
- `crates/ajax-web/src/runtime.rs:947` — `axum_task_terminal`
- `crates/ajax-web/src/runtime.rs:1000` area — origin check then upgrade
- `crates/ajax-web/src/runtime.rs:1121` — `axum_action`
- `crates/ajax-web/src/runtime.rs:2234` — existing cockpit mark test
- `crates/ajax-web/src/runtime.rs:3167` — terminal non-upgrade test helper
- `architecture.md:682` — presence documentation

## Test-first instructions

Add near the existing browser_connected / terminal tests in `runtime.rs`:

1. `axum_operations_marks_browser_connected`
   - `app_with(context_with_task(), …)`
   - `assert!(!state.browser_connected())` before
   - `post_json` a minimal valid operate/resume-or-review payload that the
     existing tests already use (reuse a working operation body from nearby
     tests; status may be 200/4xx — presence must mark on authenticated parse
     path regardless of operate outcome, as long as the handler runs past JSON
     parse). Prefer a known-good operation from `axum_operations_are_idempotent_by_request_id`.
   - `assert!(state.browser_connected())`.

2. `axum_task_terminal_marks_browser_connected_after_origin_ok`
   - Authenticated request that reaches `axum_task_terminal` with allowed origin
     (same helpers as `axum_task_terminal_rejects_non_upgrade_requests` /
     `websocket_get` with same-origin). Even if response is 400 upgrade
     required, after a same-origin authenticated hit that passed the origin
     gate, `browser_connected()` must be true.
   - Cross-site forbidden path must **not** be required to mark (do not assert
     mark on the evil-origin test).

Red:

```bash
cargo nextest run -p ajax-web axum_operations_marks_browser_connected axum_task_terminal_marks_browser_connected_after_origin_ok
```

## Edit instructions

1. `axum_action`: call `state.mark_browser_cockpit_seen()` once JSON parses
   successfully (before operate gate / work). Invalid JSON → no mark.
2. `axum_task_terminal`: call `state.mark_browser_cockpit_seen()` only after
   `websocket_origin_allowed` passes (before upgrade / plan failures are OK).
3. `architecture.md`: extend the notify-tick presence sentence so presence
   includes recent cockpit poll **or** terminal/operate browser activity
   (still 90s TTL).

Optional: also mark `axum_start_task` the same way as action if trivial and
already in this file — only if tests stay focused; otherwise skip.

## Verification commands

```bash
cargo nextest run -p ajax-web axum_operations_marks_browser_connected axum_task_terminal_marks_browser_connected_after_origin_ok
cargo nextest run -p ajax-web browser_connected axum_cockpit_marks_browser axum_task_terminal
cargo fmt --check
cargo clippy -p ajax-web --all-targets --all-features -- -D warnings
```

## Acceptance criteria

- Operate/actions refresh presence after valid JSON.
- Same-origin terminal handler refreshes presence; evil origin still forbidden.
- Cockpit-only behavior unchanged.
- architecture.md presence sentence updated.
- Diff only Allowed files.

## Stop conditions

- Need frontend poll changes to make presence work.
- Need to change TTL or tick skip logic beyond mark calls.
- Diff spreads outside Allowed files or exceeds ~120 lines.
