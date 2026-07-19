# TDD Implementation Packet: Cockpit connection hardening

PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

1. Hide connection banner for `checking`, `connected`, and `reconnecting` (only error states shout).
2. Change `createInFlightGuard` so overlapping `run()` calls coalesce into one trailing re-run after the current flight (Retry/resume never silently no-op).
3. Add App + Playwright coverage for fail → Retry → connected.

## Allowed files

- `crates/ajax-web/web/src/styles.css`
- `crates/ajax-web/web/src/shared/ui/ConnectionStatus.test.tsx`
- `crates/ajax-web/web/src/shared/lib/cockpitPoll.ts`
- `crates/ajax-web/web/src/shared/lib/cockpitPoll.test.ts`
- `crates/ajax-web/web/src/shared/hooks/useCockpitResource.test.tsx`
- `crates/ajax-web/web/src/app/App.test.tsx`
- `crates/ajax-web/web/e2e/actions.test.ts`
- `crates/ajax-web/web/e2e/fixtures.ts` (only if needed for cockpit success mock helper)

## Forbidden changes

- `TaskTerminal.tsx`, `terminalConnection.ts`, terminal e2e.
- `polling.ts` cadences.
- `useCockpitResource.ts` unless a one-line call-site change is required for the new guard API (prefer keeping `run(fn)` signature).
- `ConnectionStatus.tsx` production markup (CSS hide is enough).
- Commits, pushes, branch changes, `web/dist` rebuilds, drive-by refactors.

## Context evidence

- Desired: approved cockpit connection hardening plan.
- Banner CSS today only hides connected: `styles.css` `.connection-status[data-state="connected"] { display: none; }`.
- Guard today drops overlaps: `cockpitPoll.ts` `if (inFlight) return undefined;`.
- Existing unit asserts skip: `cockpitPoll.test.ts` `"skips a second run while the first is pending"` expects `fetchCount === 1` and `second === undefined` — must be rewritten for trailing behavior.
- Hook test `"collapses concurrent loadCockpit calls"` expects one fetch while pending — update for trailing (1 while pending, 2 after settle if a second call arrived).
- E2e: `actions.test.ts` `failCockpit` + Copy Diagnostics / Reload; no Retry recovery yet.

## Code anchors

- `styles.css` ~L255–270 connection status block
- `cockpitPoll.ts` L27–41 `createInFlightGuard`
- `cockpitPoll.test.ts` L49–66 skip-while-pending test
- `useCockpitResource.test.tsx` L60–84 concurrent collapse test
- `useCockpitResource.ts` L75–90 `loadCockpit` (do not change unless signature forces it)
- `App.tsx` L175–178 `onRetry={() => loadCockpit()}`
- `actions.test.ts` L30–43 `failCockpit`, L206–223 connection tests

## Test-first instructions

### A — Quiet banner

In `ConnectionStatus.test.tsx`, add a styles-source assertion (read `../../styles.css` like other tests, or import `?raw` if already patterned) that:

```css
.connection-status[data-state="checking"],
.connection-status[data-state="connected"],
.connection-status[data-state="reconnecting"]
```

each have `display: none` (can be one combined selector or three rules). Assert error states do **not** get `display: none`.

Red: fail because `checking`/`reconnecting` not hidden.

### B — Trailing in-flight guard

Rewrite `cockpitPoll.test.ts` pending test:

- Start hanging first `run`.
- Call `run` twice more while pending.
- While pending: `fetch` called once.
- After resolve: `fetch` called twice total (one trailing), not three.
- Update settle test if needed; keep “allows next run after first settles.”

Update `useCockpitResource.test.tsx` concurrent test to match trailing semantics (after both awaits complete, `fetchCockpit` times === 2).

Red command:

```bash
npm run web:test -- crates/ajax-web/web/src/shared/lib/cockpitPoll.test.ts crates/ajax-web/web/src/shared/ui/ConnectionStatus.test.tsx
```

### C — Retry recovery

In `App.test.tsx`: first cockpit fetch rejects network → banner text includes `backend unreachable` → next fetch resolves cockpit → click Retry → eventually no `backend unreachable` / connection status `data-state="connected"` (element may still be in DOM, hidden by CSS).

In `actions.test.ts`: add `connection Retry recovers when cockpit becomes reachable` — start with fail, then after banner visible, use `page.route` or reinstall fetch to succeed with `COCKPIT_FIXTURE`, click Retry, expect `.connection-status` not visible or `data-state=connected` and not containing unreachable. Prefer the smallest pattern consistent with existing `mockFetch` / `failCockpit` helpers; extend helpers in `fixtures.ts` only if necessary.

## Edit instructions

1. `styles.css`: extend the hide rule to include `checking` and `reconnecting` (keep error-state color rules unchanged).
2. `cockpitPoll.ts`: implement trailing dirty flag — if `run` while `inFlight`, set dirty and return; when flight finishes, if dirty, clear dirty and run `fn` again once (loop until a flight completes with dirty false). No parallel `fn` executions. Preserve `Promise<T | undefined>` return type; overlapping callers may resolve to `undefined` or the in-flight result — document via tests that trailing fetch happens.
3. Update unit/hook tests as above.
4. Add App + Playwright Retry recovery tests.
5. Do not change polling intervals or terminal code.

## Verification commands

```bash
npm run web:test -- crates/ajax-web/web/src/shared/lib/cockpitPoll.test.ts crates/ajax-web/web/src/shared/hooks/useCockpitResource.test.tsx crates/ajax-web/web/src/shared/ui/ConnectionStatus.test.tsx crates/ajax-web/web/src/app/App.test.tsx
npx playwright test crates/ajax-web/web/e2e/actions.test.ts --config crates/ajax-web/web/playwright.config.mts --grep 'connection'
```

## Acceptance criteria

- Banner CSS hides checking/connected/reconnecting; error states still visible.
- Overlapping loadCockpit/guard runs produce exactly one trailing fetch after the in-flight one.
- App Retry recovers from backend unreachable to connected.
- Playwright connection Retry test green; existing Copy Diagnostics / Reload still pass.
- Focused commands above exit 0.

## Stop conditions

- Need terminal/WS changes.
- Diff exceeds ~400 lines or leaves Allowed files.
- Playwright cannot be made green without rewriting the whole mock stack — stop and report after App unit recovery is green.
