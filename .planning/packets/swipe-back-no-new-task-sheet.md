# Packet: Swipe-back must not show new-task sheet

```yaml
PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior
```

## Goal

After creating a new task and navigating back to the dashboard (including
swipe-right back), the new-task creation sheet must not be visible at all.

## Allowed files

- `crates/ajax-web/web/src/app/App.tsx`
- `crates/ajax-web/web/src/app/App.test.tsx`
- `.planning/agent-plans/swipe-back-no-new-task-sheet.md` (checklist only if needed)

## Forbidden changes

- Do not change swipe gesture math (`navigateSwipe`, `useSwipePageTransition`)
- Do not change NewTaskSheet form/submit API beyond what App wiring needs
- Do not change architecture docs
- Do not commit, push, merge, rebase, or switch branches

## Code anchors

- `App.tsx`: `const [sheetOpen, setSheetOpen] = useState(false);` (~line 75)
- `App.tsx`: New button `onClick={() => setSheetOpen(true)}` (~360)
- `App.tsx`: `{sheetOpen && (<NewTaskSheet .../>)}` (~479)
- `NewTaskSheet.tsx` success path already calls `onOpenTask` then `onClose`
- Route kinds: `dashboard` | `project` | `task` | `diff` | `settings`

## Implementation sketch

1. In `App.tsx`, when `route.kind` is `task` or `diff`, force
   `setSheetOpen(false)` (effect keyed on route kind / handle as needed).
2. Gate mount: only render `NewTaskSheet` when `sheetOpen` is true **and**
   route is not `task`/`diff` (prevents a one-frame flash if state races).
3. Keep Cancel / Escape / backdrop / successful Start `onClose` behavior.
4. Optional micro-hardening (only if cheap): in `NewTaskSheet` success path,
   call `onClose` before `onOpenTask` so the sheet is gone before hash change.
   Prefer App-side route coupling as the source of truth.

## Acceptance criteria

- Starting a task still navigates to the new task outlet and closes the sheet.
- After start, navigating back to `#/` (simulating swipe-back) shows **zero**
  `[data-testid="new-task-sheet"]` nodes.
- Opening New on the dashboard still opens the sheet.
- Cancel / Escape still close the sheet on the dashboard.
- No unrelated behavior changes.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: rtk npm run web:test -- --run src/app/App.test.tsx src/features/task/NewTaskSheet.test.tsx
      expected: pass
    - type: typecheck
      command: rtk npm run web:check
      expected: pass
  broader_checks: []
  reason: App-level route/sheet coupling is best locked by App vitest; typecheck catches wiring mistakes.
```

Suggested App test shape (adapt to existing helpers in `App.test.tsx`):

1. Mock `/api/cockpit`, `/api/version`, `/api/tasks` start, and task detail fetch.
2. Open New → fill title → Start.
3. Assert task outlet + sheet count 0.
4. `setHash("#/")` (or project hash).
5. Assert dashboard outlet + sheet count still 0.

## Stop conditions

- Need to change history / bfcache / Safari-native back semantics beyond App state
- Fix requires editing swipe transition internals
- Verification cannot run in this environment

## Dispatch

```yaml
dispatch_level: compact
```
