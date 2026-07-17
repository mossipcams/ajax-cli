PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Port dashboard `TaskList` to React with a `useSwipeReveal` hook (native touch listeners matching `swipeRevealAction.ts` passive flags). Island-swap the App dashboard/project outlet to `<ReactIsland component={TaskList} …>`. Delete `TaskList.svelte` + `TaskList.test.ts`. Delete `swipeRevealAction.ts` (+test) if TaskList was the sole consumer (confirmed: only TaskList imports it). Keep pure `swipeReveal.ts`. Move TaskList scoped CSS verbatim into `styles.css`. Use React `ActionBar` inside TaskList.

## Allowed files

- `crates/ajax-web/web/src/react/useSwipeReveal.ts` (new)
- `crates/ajax-web/web/src/react/useSwipeReveal.test.ts` (new)
- `crates/ajax-web/web/src/components/TaskList.tsx` (new)
- `crates/ajax-web/web/src/components/TaskList.test.tsx` (new; port from `.test.ts`)
- `crates/ajax-web/web/src/components/TaskList.svelte` (delete)
- `crates/ajax-web/web/src/components/TaskList.test.ts` (delete)
- `crates/ajax-web/web/src/gestures/swipeRevealAction.ts` (delete if sole consumer)
- `crates/ajax-web/web/src/gestures/swipeRevealAction.test.ts` (delete with action)
- `crates/ajax-web/web/src/components/App.svelte` (import swap only for TaskList → ReactIsland)
- `crates/ajax-web/web/src/styles.css` (append TaskList CSS verbatim)
- `.planning/agent-plans/react-slice-s2.md` (checklist only)

## Forbidden changes

- No ActionBar.svelte deletion; TaskDetail keeps Svelte ActionBar
- No frozen module logic changes (`state.ts`, `api.ts`, `swipeReveal.ts` pure helpers stay)
- No e2e assertion weakening (swipe-reveal.test.ts must keep passing)
- No commit/push/branch changes

## Context evidence

- App consumer: `App.svelte` ~321–329 `<TaskList …>` inside dashboard/project outlet.
- ReactIsland: `src/react/ReactIsland.svelte` + ConnectionStatus/Skeleton pattern.
- Gesture: `swipeRevealAction.ts` — `touchstart/move` `{passive:true}`, `touchend/cancel` non-passive; logic in `swipeReveal.ts`.
- ActionBar React already at `ActionBar.tsx` (Task 1).
- TaskList tests include `?raw` CSS contract checks for `.is-inbox` / `.project-pill.is-active` — after CSS move, point those assertions at `styles.css?raw` (or the appended section), not deleted svelte.
- Characterization e2e: `e2e/swipe-reveal.test.ts` must stay green on mobile-webkit.

## Code anchors

- `useSwipeReveal(ref, { onOffset, onOpenChange })` — attach/detach listeners in `useEffect`; mirror action API.
- `TaskList.tsx` — same props as Svelte; import React ActionBar; per-row offsets state; useSwipeReveal on each row button ref (or callback ref map by handle).
- App: `import TaskList from "./TaskList"` (tsx) + `ReactIsland` with props object matching current callbacks.
- Delete Svelte TaskList; grep for `TaskList.svelte` / `swipeRevealAction` must be empty after.

## Test-first instructions

1. Add failing `useSwipeReveal.test.ts` (settled open at 88; vertical closes) and `TaskList.test.tsx` port while `TaskList.tsx` absent → RED.
2. Red:
   ```bash
   npm run web:test -- --run crates/ajax-web/web/src/react/useSwipeReveal.test.ts crates/ajax-web/web/src/components/TaskList.test.tsx
   ```
3. Implement to green; swap App; delete Svelte files; move CSS.
4. Green + e2e:
   ```bash
   npm run web:test -- --run crates/ajax-web/web/src/react/useSwipeReveal.test.ts crates/ajax-web/web/src/components/TaskList.test.tsx crates/ajax-web/web/src/components/App.test.ts
   npm run web:check
   npm run web:smoke -- --project=mobile-webkit crates/ajax-web/web/e2e/swipe-reveal.test.ts
   ```

## Edit instructions

1. Implement hook from `swipeRevealAction.ts` behavior using `swipeReveal.ts`.
2. Port TaskList markup/behavior; wire React ActionBar in reveal slot.
3. Move `<style>` block from TaskList.svelte into `styles.css` unchanged (selectors stay global class names).
4. Update CSS source-contract tests to read `styles.css`.
5. Swap App.svelte TaskList → ReactIsland; delete Svelte TaskList + swipeRevealAction (+tests).

## Verification commands

```bash
npm run web:test -- --run crates/ajax-web/web/src/react/useSwipeReveal.test.ts crates/ajax-web/web/src/components/TaskList.test.tsx crates/ajax-web/web/src/components/App.test.ts
npm run web:check
npm run web:smoke -- --project=mobile-webkit crates/ajax-web/web/e2e/swipe-reveal.test.ts crates/ajax-web/web/e2e/smoke.test.ts
```

## Acceptance criteria

- TaskList React + hook tests green; App tests green; swipe e2e + smoke mobile-webkit green.
- No `TaskList.svelte` / `swipeRevealAction` references remain.
- ActionBar.svelte remains for TaskDetail.
- Diff limited to allowed files.

## Stop conditions

- layout-scroll e2e fails for reasons needing invariant changes.
- Touch passive flags diverge from Svelte action.
- Scope grows into TaskDetail/settings/shell.
