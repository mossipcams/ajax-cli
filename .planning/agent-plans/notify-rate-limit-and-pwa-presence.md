# Notify: rate-limit false positives + PWA presence

## Scope

Two spam sources reported together:

1. **Rate limited is wrong / spammy.** Pane matcher treats `"try again later"` as
   `RateLimited` (OR with `"rate limit"` / `"too many requests"`). Agents say
   that phrase constantly → false Waiting status → webhook spam. Also treat
   genuine Rate limited as non-actionable for phone pings (wait-it-out, not
   operator input).
2. **PWA active still gets pings.** Presence today is only `GET /api/cockpit`.
   Terminal WebSocket / operate traffic does not refresh the 90s TTL, so the
   background notify tick can fire while the operator is working in the
   terminal (especially if cockpit polls stall).

## Non-goals

- Visibility-aware SPA signaling (hidden vs focused) beyond existing poll skip
- Suppressing CLI/TUI notify paths based on PWA presence
- Changing Waiting confirmation dwell or episode-clear constants
- New config knobs

## Delegation decision

`Delegation decision: delegated via model-router` — two sequential rounds
(pi / GLM). Parent Review Gate ACCEPT on both after independent validation.

## Tasks

### Round A — rate limit (ajax-core)

- [x] Test: pane text with only `"try again later"` does **not** classify as
      `RateLimited`
- [x] Test: `"rate limit exceeded"` / `"too many requests"` still does
- [x] Test: `take_attention_transition` does not fire for Waiting `"Rate limited"`
- [x] Implement: tighten `pane_evidence` rate-limit phrases; exclude
      `"Rate limited"` in `is_actionable_attention`
- [x] Docs: architecture.md notify bullet — drop rate limited from phone-ping list
- [x] Validate focused nextest + fmt/clippy

### Round B — PWA presence (ajax-web)

- [x] Test: terminal WS upgrade marks browser connected
- [x] Test: `/api/operations` marks browser connected
- [x] Implement: `mark_browser_cockpit_seen` in those handlers
- [x] Parent add-on: also mark on terminal operator-input sink (keeps TTL
      alive while typing if cockpit polls stall)
- [x] Docs: architecture.md one-liner — presence includes terminal/operate
- [x] Validate focused ajax-web nextest + fmt/clippy

## Validation

```bash
# Round A (parent)
cargo nextest run -p ajax-core try_again_later_alone_is_not_rate_limited rate_limited_waiting_does_not_notify
# 2 passed

# Round B (parent)
cargo nextest run -p ajax-web axum_operations_marks_browser_connected axum_task_terminal_marks_browser_connected_after_origin_ok browser_connected
# 4 passed

cargo fmt --check  # exit 0
```

## Deviations

(none)
