# Web battery wave 2 — cut unnecessary polls and boot JS

## Scope

Follow-up to landed PR1–PR7 power work and `#750` refresh-thrash cuts.
Target remaining battery drains in Web Cockpit (especially iOS Safari):

1. Dashboard still polls `/api/cockpit` every **1s** while visible
2. Quiet fleets (all idle) still use the active cadence
3. `DiffReview` + `SettingsView` are eager App imports (parse/compile cost on every boot)
4. `TaskList` relative-time ticker keeps firing while the document is hidden

## Non-goals

- Terminal library changes / scrollback policy
- Server RefreshTier / notify tick changes
- User-facing “battery mode” UI
- Focus/pageshow debounce (recovery-sensitive; defer)
- Seed capture sizing (separate backend task)

## Delegation decision

`Delegation decision: delegated via model-router` (cursor-delegate / composer-2.5)

## Design

### Cadence

| Context | Cockpit today | Target |
| --- | --- | --- |
| hidden | 60s | unchanged |
| task | 5s | unchanged |
| settings / diff | 10s | unchanged |
| dashboard / project with live work | **1s** | **3s** |
| dashboard / project, all cards idle (or empty) | 1s | **10s** (idle) |

`fleetQuiet` = projection loaded and every card status is `idle` (case-insensitive).
While `cockpit.data === null`, keep the active cadence (do not treat as quiet).

`REFRESH_INTERVAL_ACTIVE_MS = 3000`.

### Lazy routes

Mirror `TaskTerminal`: `lazy(() => import(...))` + `Suspense` for Diff and Settings
outlets in `App.tsx`. Fallback `null`.

### Hidden ticker

`TaskList` `setInterval(60_000)` for `nowSecs` must not run while
`document.visibilityState !== "visible"`; restart on become-visible.

## Checklist

### Task A — cadence
- [x] Update constants + `cockpitRefreshIntervalMs` (+ tests)
- [x] Wire `fleetQuiet` from App
- [x] Update App cadence tests (1000 → 3000; quiet path)
- [x] Parent validation + accept — **Accepted**

### Task B — lazy Diff/Settings + pause ticker
- [x] Lazy DiffReview + SettingsView
- [x] Pause TaskList ticker when hidden (+ test)
- [x] Parent validation + accept — **Accepted** (parent fixed import order)

## Validation

### Task A
```bash
npm run web:test -- crates/ajax-web/web/src/shared/lib/polling.test.ts crates/ajax-web/web/src/app/App.test.tsx --run
# EXIT 0 — 60 passed
```

### Task B (+ regression)
```bash
npm run web:test -- \
  crates/ajax-web/web/src/shared/lib/polling.test.ts \
  crates/ajax-web/web/src/app/App.test.tsx \
  crates/ajax-web/web/src/features/task/TaskList.test.tsx \
  crates/ajax-web/web/src/features/diff/DiffReview.test.tsx \
  crates/ajax-web/web/src/features/settings/SettingsView.test.tsx \
  --run
# EXIT 0 — 101 passed
```

## Deviations

- Both Cursor delegates returned `MISSING_STRUCTURED_REPORT`; parent gated on git diff + vitest.
- Parent hardened `fleetQuiet` with `(card.status || "").toLowerCase()`.
- Parent moved lazy consts below all static imports in `App.tsx`.

## Results

Wave 2 complete in worktree (uncommitted). Not committed/pushed.

---

## Codebase audit (2026-08-04) — remaining drains

Fresh scan of live `ajax-web` (not prior plans). Ranked by phone battery impact.

### HIGH

1. **`cursorBlink: true` on xterm** (`mountTaskTerminalSession.ts:660`)
   - Continuous blink timer/repaint while a task terminal is open, even when PTY is idle.
   - iOS Safari keeps compositing for blink. Static cursor (or blink only while focused) is a big win.

2. **Terminal WS reconnect dials while hidden** (`terminalConnection.ts:175–177`)
   - `scheduleReconnect` always `connect(false)` after backoff; only the *delay* checks visibility.
   - Backgrounded Safari may still fire reconnect attempts when JS briefly runs → radio wake.
   - Fix: skip dial when `document.hidden`; rely on existing `visibilitychange` → `redialNow`.

3. **Resume fan-out: `focus` + `pageshow` + `online` + `visibilitychange`** (`App.tsx:136–139`)
   - Each calls `checkVersion` + `loadCockpit({ trailing: true })`.
   - Trailing coalesces *overlap*, not back-to-back sequential resumes (iOS often fires focus then visibility).
   - Fix: debounce shell resume ~500–1000ms into one poll.

4. **Task-route cockpit poll still 5s** while terminal WS already marks presence
   - Presence: `live.rs` terminal open + PTY input bump `mark_browser_cockpit_seen`.
   - Task page only needs cockpit for header “N running”; WS is the live path.
   - Fix: slow task route to idle (10s) or 15–30s; optional presence heartbeat via cheap endpoint later.

5. **Infinite CSS pulse** (`.live-dot.is-live`, `.status-dot.tone-running`)
   - Always-on opacity animation whenever connected / any running card.
   - `prefers-reduced-motion` kills it only for a11y users, not default iOS.
   - Fix: static accent when live; pulse only briefly on status *change*, or CSS that pauses when `document.hidden` via a class.

### MEDIUM

6. **Full `JSON.stringify(cockpit)` hash every poll** (`cockpitPoll.ts`)
   - Even unchanged bodies pay stringify CPU on phone every 3–10s.
   - No ETag / revision field on `BrowserCockpitView`. Server revision exists in-process only.
   - Fix: server `ETag`/`X-Ajax-Revision` + client short-circuit, or hash cheaper stable fields.

7. **Server cockpit cache TTL = 750ms** (`COCKPIT_REFRESH_CACHE_TTL`)
   - Client now polls ≥3s, so almost every GET misses cache → Live `refresh_runtime_context` on Mac.
   - Hurts host CPU more than phone radio, but still wakes LAN. Align TTL with min client cadence (~2–3s).

8. **Version poll never stops after banner** (`useVersionMonitor` + App interval)
   - Once `updateAvailable`, still hits `/api/version` every 30s/120s/300s.
   - Fix: skip interval (and resume checks) after banner is up.

9. **Eager boot imports still in App**
   - `TaskDetail`, `TaskList`, `NewTaskSheet` static. Diff/Settings already lazy.
   - `NewTaskSheet` only needed when sheet opens → easy lazy.
   - `TaskDetail` only on task route → lazy like Diff.

10. **Sticky header + bottom-nav `backdrop-filter: blur(12px)`** (`styles.css`)
    - Continuous GPU blur on iOS Safari while scrolling.
    - Fix: opaque paper background on narrow phones; keep blur on desktop only.

### LOWER / situational

11. **Speech pause countdown `setInterval(200)`** — only in `pause_pending`; OK.
12. **TestInDevPanel 1.5s poll** — only while deploy active; OK.
13. **Hidden cockpit 60s** — needed for ≤90s `BROWSER_CONNECTED_TTL` presence; don’t slow past ~75s without another presence path.
14. **StrictMode double-mount** — already guarded; not a production drain.

### Proposed wave 3 (codebase-driven)

| # | Change | Risk |
| --- | --- | --- |
| 1 | Disable xterm `cursorBlink` (or blink only while focused) | Low UX (static cursor) |
| 2 | No WS reconnect dial while `document.hidden` | Low |
| 3 | Debounce shell resume polls | Medium (recovery timing) |
| 4 | Slow task-route cockpit to ≥10s | Low |
| 5 | Stop infinite live/running pulse by default | Low visual |
| 6 | Stop version polls after update banner | Low |
| 7 | Lazy `NewTaskSheet` (+ optional `TaskDetail`) | Low |
| 8 | Raise server cockpit cache TTL toward 2–3s | Medium (staleness) |

## Wave 3 — HIGH only (authorized)

**Delegation decision: delegated via model-router** (cursor-delegate / composer-2.5)

Scope: items 1–5 only. Non-goals: version-after-banner, lazy NewTaskSheet, server TTL, JSON ETag.

### Design

1. `cursorBlink: false` in `mountTaskTerminalSession.ts` (static cursor; no focus-toggle complexity).
2. In `scheduleReconnect` timer callback: if `document.hidden`, do **not** `connect`; leave status `reconnecting` so existing `visibilitychange` → `redialNow(false)` recovers.
3. Debounce shell resume (`focus` / `pageshow` / `online` / become-visible) to **750ms** single trailing `checkVersion` + `loadCockpit({ trailing: true })`. Mount load stays immediate.
4. `REFRESH_INTERVAL_TERMINAL_MS = 10000` (task route matches idle).
5. Remove infinite `animation: pulse` from `.live-dot.is-live`, `.status-dot.tone-running`, and `.interact-pill.tone-running::before`. Keep solid accent color. Update App.test CSS pin.

### Checklist

#### W3a — terminal blink + hidden reconnect
- [x] `cursorBlink: false` + tests that pin it
- [x] No `connect` while `document.hidden` in reconnect timer; visible still dials / visibility redials
- [x] Parent validation + accept — **Accepted** (66 vitest)

#### W3b — resume debounce + task poll 10s
- [x] 750ms resume debounce in App
- [x] `REFRESH_INTERVAL_TERMINAL_MS = 10000` + tests
- [x] Parent validation + accept — **Accepted** (60 vitest; parent hoisted `RESUME_DEBOUNCE_MS`)

#### W3c — kill infinite pulse
- [x] CSS + App.test pin
- [x] Parent validation + accept — **Accepted** (removed `@keyframes pulse`)

### Deviations (wave 2/3)

- Lazy DiffReview/SettingsView **reverted**: Vite embed contract allows only
  `app.js` + `terminal.js`; extra async chunks fail `web:build`. Keep them eager
  until the chunk allowlist is expanded intentionally.

## Wave 3 validation

```bash
npm run web:test -- polling App terminalConnection TaskTerminal TaskList --run
# EXIT 0 — 142 passed
npm run web:build
# EXIT 0 — app.js + terminal.js only
```

Wave 2 (cadence/quiet/ticker; Diff/Settings lazy dropped) + Wave 3 HIGH complete in worktree.
