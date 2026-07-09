# TDD Packet: Web adaptive polling (PR1)

## 1. Goal

Make Web Cockpit cockpit and version polling adaptive by document visibility and route (terminal-open vs dashboard vs idle), without changing restart polling or terminal I/O.

## 2. Allowed files

**Production**

- `crates/ajax-web/web/src/polling.ts`
- `crates/ajax-web/web/src/components/App.svelte`

**Tests**

- `crates/ajax-web/web/src/polling.test.ts` (create)
- `crates/ajax-web/web/src/components/App.test.ts` (only if needed for timer reschedule behavior; prefer pure unit tests in `polling.test.ts`)

**Plan / packet (parent may update; do not invent new plans)**

- `.planning/agent-plans/web-mobile-power-optimizations.md` (check off items only if you touch it; optional)

## 3. Forbidden changes

- Do not edit Rust crates, terminal connection, Ghostty, resize, scrollback, or binary WS framing.
- Do not change `RESTART_POLL_MS` / `RESTART_TIMEOUT_MS` semantics used by `waitForServerOnline` in `api.ts`.
- Do not add change-only cockpit hashing or in-flight poll guards (PR5).
- Do not batch terminal writes/output (PR2/PR3).
- Do not rename public restart helpers or break Settings/ActionBar imports of `CONFIRM_TIMEOUT_MS` / `RESULT_AUTO_DISMISS_MS`.
- Do not add a user-facing battery-mode UI.
- Do not touch `web/dist/` generated assets unless the build pipeline requires it (prefer not).
- No drive-by refactors or formatting sweeps outside the allowed files.

## 4. Architecture context

Web Cockpit is a presentation adapter over server-authoritative cockpit projections (`architecture.md` Web Cockpit section). Polling refreshes the latest projection; the browser must not become a second task registry. Adaptive intervals only change *when* the shell asks for a refresh, not *what* truth means.

`App.svelte` owns the cockpit/version timers today with fixed `REFRESH_INTERVAL_MS` / `VERSION_POLL_MS`. Task routes mount `TerminalRawView` (terminal open). `document.hidden` already skips `loadCockpit` early-return; visibility resume still forces an immediate poll — keep that for becoming visible.

## 5. Code anchors

Current constants (`polling.ts`):

```ts
export const REFRESH_INTERVAL_MS = 1000;
export const VERSION_POLL_MS = 30000;
export const RESTART_POLL_MS = 500;
```

Current App wiring (`App.svelte`):

```ts
async function loadCockpit() {
  if (document.hidden) return;
  ...
}

$effect(() => {
  void loadCockpit();
  const idleHandle = whenIdle(() => void checkVersion());
  const cockpitTimer = setInterval(loadCockpit, REFRESH_INTERVAL_MS);
  const versionTimer = setInterval(checkVersion, VERSION_POLL_MS);
  const onResume = () => {
    void checkVersion();
    void loadCockpit();
  };
  ...
  document.addEventListener("visibilitychange", onResume);
});
```

Route signal for terminal-open: `route.kind === "task"` (see `taskOpenHandle` / task outlet). Settings = idle. Dashboard/project = active.

Reuse existing Vitest style from `App.test.ts` (fake timers, fetch stubs). Prefer pure functions so tests do not need full App mount.

Keep exporting aliases if anything still imports old names:

- Prefer new names as primary.
- `REFRESH_INTERVAL_MS` may remain as alias of `REFRESH_INTERVAL_ACTIVE_MS` **only if** needed to avoid breaking imports; otherwise update App imports and delete the old single constant.
- Do not break `RESTART_POLL_MS` import from `api.ts`.

## 6. Test-first instructions

Create `crates/ajax-web/web/src/polling.test.ts`.

Add tests (exact names):

1. `cockpitRefreshIntervalMs returns hidden interval when document is not visible`
2. `cockpitRefreshIntervalMs returns terminal interval on task route when visible`
3. `cockpitRefreshIntervalMs returns idle interval on settings route when visible`
4. `cockpitRefreshIntervalMs returns active interval on dashboard or project when visible`
5. `versionPollIntervalMs returns hidden / terminal / default intervals by context`
6. `restart poll constant stays at 500ms`

API under test (implement after tests fail):

```ts
export type PollingRouteKind = "dashboard" | "project" | "task" | "settings";

export function cockpitRefreshIntervalMs(input: {
  visibilityState: DocumentVisibilityState;
  routeKind: PollingRouteKind;
}): number;

export function versionPollIntervalMs(input: {
  visibilityState: DocumentVisibilityState;
  routeKind: PollingRouteKind;
}): number;
```

Constants to assert:

```ts
REFRESH_INTERVAL_ACTIVE_MS = 1000
REFRESH_INTERVAL_TERMINAL_MS = 5000
REFRESH_INTERVAL_IDLE_MS = 10000
REFRESH_INTERVAL_HIDDEN_MS = 60000
VERSION_POLL_MS = 30000
VERSION_POLL_TERMINAL_MS = 120_000
VERSION_POLL_HIDDEN_MS = 300_000
RESTART_POLL_MS = 500
```

Priority: hidden wins over route. Task wins over idle/active when visible. Settings uses idle cockpit interval; version stays at default 30s when visible and not on task.

**Fail first:** write tests, run:

```bash
npm run web:test -- crates/ajax-web/web/src/polling.test.ts
```

Confirm failure (missing exports / wrong values). Then implement.

Optional App test only if pure selectors are insufficient: assert that when hash becomes `#/task/...` the cockpit interval is rescheduled (harder; skip if pure coverage is solid and App wires selectors correctly by source inspection + one light integration test). Prefer not expanding App.test unless necessary.

## 7. Production edit instructions

### `polling.ts`

1. Add the constants listed above.
2. Add `cockpitRefreshIntervalMs` and `versionPollIntervalMs` with the priority rules.
3. Keep `CONFIRM_TIMEOUT_MS`, `RESULT_AUTO_DISMISS_MS`, `RESTART_POLL_MS`, `RESTART_TIMEOUT_MS` unchanged.
4. Remove or alias `REFRESH_INTERVAL_MS` — App must stop using a single fixed cockpit interval.

### `App.svelte`

1. Import the new selectors + any constants still needed.
2. Replace the single mount-time `setInterval(..., REFRESH_INTERVAL_MS)` / `VERSION_POLL_MS` with adaptive scheduling that **reschedules when `route.kind` or visibility changes**.
3. Recommended minimal pattern:
   - Track `route` (already `$state`).
   - On an effect that depends on `route.kind` (and listens for `visibilitychange`), clear previous timers and `setInterval` with `cockpitRefreshIntervalMs({ visibilityState: document.visibilityState, routeKind })` and the matching version interval.
   - Map `route.kind` directly to `PollingRouteKind` (`"dashboard" | "project" | "task" | "settings"` — already matches `Route` kinds).
4. Keep immediate `loadCockpit` / `checkVersion` on focus / pageshow / becoming visible.
5. Fix visibility handler: do **not** treat hide as a resume that should no-op forever; when hidden, intervals should be the slow hidden cadence (or skip cockpit fetch via existing `document.hidden` guard — either is OK if hidden interval is 60s and loadCockpit still early-returns when hidden). Prefer: still schedule hidden intervals, keep `if (document.hidden) return` in `loadCockpit` so hidden ticks are cheap no-ops **or** remove the early return and rely on 60s interval — **choose: keep early return + 60s interval** so accidental wakes stay rare; version poll may still run at hidden cadence (version check is cheap) OR also skip when hidden — prefer still allowing version at 300s when hidden (remove any blanket skip for version unless already present; today version always runs).
6. Do not delay keystrokes or touch terminal code.
7. Preserve pull-to-refresh / retry / mutation-triggered `loadCockpit()` immediate calls.

## 8. Verification commands

```bash
npm run web:test -- crates/ajax-web/web/src/polling.test.ts
npm run web:test -- crates/ajax-web/web/src/components/App.test.ts
```

If App tests fail due to the version banner test hardcoding `30000`, update that test to use `VERSION_POLL_MS` import or keep 30000 if dashboard default is unchanged (it should still be 30000 on dashboard).

## 9. Acceptance criteria

- Tests listed above exist and fail before production edits, then pass after.
- Hidden → cockpit 60s, version 300s.
- Visible task → cockpit 5s, version 120s.
- Visible settings → cockpit 10s, version 30s.
- Visible dashboard/project → cockpit 1s, version 30s.
- `RESTART_POLL_MS === 500`.
- App reschedules intervals when route or visibility changes.
- No terminal/server/binary/hash/in-flight changes.

## 10. Stop conditions

- Need to change files outside Allowed files.
- App timer wiring requires a large rewrite of `$effect` lifecycle and behavior is unclear.
- Existing App tests fail for unrelated reasons you cannot fix within Allowed files.
- Conflict with architecture (browser owning task truth) — should not apply here; stop if asked to cache cockpit as source of truth.
