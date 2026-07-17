PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Port `ResultPanel.svelte` to `ResultPanel.tsx` with assertion-for-assertion RTL tests. Island-swap the App consumer. Delete the Svelte component and its test. Styles already live in `styles.css` (`.result-panel`) — do not duplicate.

## Allowed files

- `crates/ajax-web/web/src/components/ResultPanel.tsx` (new)
- `crates/ajax-web/web/src/components/ResultPanel.test.tsx` (new)
- `crates/ajax-web/web/src/components/ResultPanel.svelte` (delete)
- `crates/ajax-web/web/src/components/ResultPanel.test.ts` (delete)
- `crates/ajax-web/web/src/components/App.svelte` (import + ReactIsland swap only for ResultPanel)
- `.planning/agent-plans/react-slice-s3.md` (checklist only)

## Forbidden changes

- No SettingsView changes in this packet
- No api/polling constant edits
- No styles.css edits (panel styles already global)
- No commit/push/branch changes

## Context evidence

- Impl: `ResultPanel.svelte` — undo-armed toast uses `DROP_UNDO_MS` then `onCommit`+`onDismiss`; success `RESULT_SUCCESS_DISMISS_MS` (4s); error `RESULT_AUTO_DISMISS_MS` (longer); Undo/Dismiss both call undo-then-dismiss when armed.
- Tests: `ResultPanel.test.ts` — 8 cases (message/output, error class, dismiss, 4s success, longer error, aria roles, Undo click, onCommit after DROP_UNDO_MS).
- App: `App.svelte` ~353–361 `<ResultPanel message=… output=… isError=… onUndo=… onCommit=… onDismiss=… />`.
- React island pattern: TaskList / ConnectionStatus via `ReactIsland`.

## Code anchors

- Props identical: `message`, `output`, `isError`, `onDismiss`, `onUndo`, `onCommit`.
- DOM: `div.result-panel` + `is-error`, `role`/`aria-live`, `.result-message`, `.result-output`, Undo `pill is-primary`, Dismiss `pill`.
- useEffect timer cleanup on message/deps change (match Svelte `$effect`).

## Test-first instructions

1. Add `ResultPanel.test.tsx` ported from `.test.ts` importing `./ResultPanel` while tsx missing → RED.
2. ```bash
   npm run web:test -- --run crates/ajax-web/web/src/components/ResultPanel.test.tsx
   ```
3. Implement; swap App; delete svelte+test; green + `npm run web:check`.

## Edit instructions

1. Mechanical React port with hooks.
2. App: `import ResultPanel from "./ResultPanel"` + `<ReactIsland component={ResultPanel} props={{…}} />`.
3. Delete Svelte files; grep `ResultPanel.svelte` empty.

## Verification commands

```bash
npm run web:test -- --run crates/ajax-web/web/src/components/ResultPanel.test.tsx crates/ajax-web/web/src/components/App.test.ts
npm run web:check
```

## Acceptance criteria

- RTL suite green assertion-for-assertion; App tests green; no Svelte ResultPanel left.

## Stop conditions

- Timer semantics need polling constant changes.
- Diff escapes allowed files / SettingsView touched.
