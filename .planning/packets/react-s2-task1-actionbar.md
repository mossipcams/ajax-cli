PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Port `ActionBar.svelte` to `ActionBar.tsx` with assertion-for-assertion RTL tests. Keep `ActionBar.svelte` in place for `TaskDetail` until S6; add a freeze/removal-condition comment. Do **not** swap TaskDetail or App consumers in this packet. TaskList still imports the Svelte ActionBar until Task 2.

## Allowed files

- `crates/ajax-web/web/src/components/ActionBar.tsx` (new)
- `crates/ajax-web/web/src/components/ActionBar.test.tsx` (new; port from `.test.ts`)
- `crates/ajax-web/web/src/components/ActionBar.svelte` (comment only: freeze until S6)
- `crates/ajax-web/web/src/components/ActionBar.test.ts` (keep running against Svelte until Task 2 decides; do not delete)
- `.planning/agent-plans/react-slice-s2.md` (checklist only)

## Forbidden changes

- No App.svelte / TaskList / TaskDetail consumer swaps
- No deletion of ActionBar.svelte
- No api/polling/types changes
- No commit/push/branch changes

## Context evidence

- Svelte impl: `ActionBar.svelte` — two-tap confirm (`CONFIRM_TIMEOUT_MS`), delayed Drop (`DROP_UNDO_MS` + onUndo/onCommit), `postOperation`, primary first button, remediation class set.
- Tests: `ActionBar.test.ts` — 7 cases (primary, two-tap+undo window, dismiss after drop, undo cancels, confirm expiry, mutate vs dismiss, cockpit forward).
- React pattern: `ConnectionStatus.tsx` default export + props; RTL via `@testing-library/react` as in `ConnectionStatus.test.tsx`.
- Global `.action` styles already in CSS — ActionBar only needs `.action-row` flex wrapper (inline style or small class matching Svelte scoped output — prefer a plain `className="action-row"` that already exists globally or duplicate the 3-line rule in a non-scoped way; check `styles.css` / App for `.action-row`).

## Code anchors

- Props identical to Svelte `Props` interface (actions, handle, onCockpit, onResult, onMutated, onDismiss).
- Mark `ActionBar.svelte` top comment: `// Frozen duplicate for TaskDetail until S6 deletes this file; keep bugfixes in sync with ActionBar.tsx.`
- Port test file to `ActionBar.test.tsx` using `@testing-library/react` (`render`, `fireEvent`, `screen`) — same assertions; do not weaken.
- Leave `ActionBar.test.ts` intact so Svelte bar stays covered.

## Test-first instructions

1. Add `ActionBar.test.tsx` ported from `ActionBar.test.ts` importing `./ActionBar` (tsx). Run while `ActionBar.tsx` missing → RED.
2. Red command:
   ```bash
   npm run web:test -- --run crates/ajax-web/web/src/components/ActionBar.test.tsx
   ```
3. Implement `ActionBar.tsx` to green; add freeze comment on `.svelte`.
4. Also run existing:
   ```bash
   npm run web:test -- --run crates/ajax-web/web/src/components/ActionBar.test.ts
   ```

## Edit instructions

1. Mechanical behavior port of `ActionBar.svelte` → React hooks (`useState`, `useEffect` for timer cleanup, `useRef` for dropResolved if needed).
2. Match DOM: `div.action-row`, buttons with `data-action`, `data-task`, `data-destructive`, classes `action`, `primary`, `confirming`, `is-running`, `remediation-action`.
3. Smallest diff; no redesign.

## Verification commands

```bash
npm run web:test -- --run crates/ajax-web/web/src/components/ActionBar.test.tsx crates/ajax-web/web/src/components/ActionBar.test.ts
npm run web:check
```

## Acceptance criteria

- React ActionBar RTL suite green assertion-for-assertion.
- Svelte ActionBar suite still green.
- Freeze comment on ActionBar.svelte.
- No consumer swaps.

## Stop conditions

- Timer/fake-timer tests require API or polling constant changes.
- Diff escapes allowed files.
- Temptation to delete Svelte ActionBar.
