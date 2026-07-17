PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Port `SettingsView.svelte` to `SettingsView.tsx` with RTL port. Move scoped CSS verbatim into `styles.css`. Island-swap App settings outlet. Delete Svelte component + test. Repoint `legacyTerminalRemoval.test.ts` path from `SettingsView.svelte` to `SettingsView.tsx` (same forbidden symbols). Keep using `restartServer` / `waitForServerOnline` from `api.ts` — do not reimplement polling.

## Allowed files

- `crates/ajax-web/web/src/components/SettingsView.tsx` (new)
- `crates/ajax-web/web/src/components/SettingsView.test.tsx` (new)
- `crates/ajax-web/web/src/components/SettingsView.svelte` (delete)
- `crates/ajax-web/web/src/components/SettingsView.test.ts` (delete)
- `crates/ajax-web/web/src/components/App.svelte` (SettingsView → ReactIsland only)
- `crates/ajax-web/web/src/styles.css` (append SettingsView CSS verbatim)
- `crates/ajax-web/web/src/legacyTerminalRemoval.test.ts` (path string only)
- `.planning/agent-plans/react-slice-s3.md` (checklist only)

## Forbidden changes

- No `api.ts` / diagnostics.ts logic changes
- No ResultPanel rework (already React from Task 1)
- No shadcn dependency adds
- No commit/push/branch changes

## Context evidence

- Impl: two-tap restart (`CONFIRM_TIMEOUT_MS`), `restartServer` + `waitForServerOnline`, diagnostics run/copy, reloadApp, debug dl (version/origin/online/ua).
- Tests: `SettingsView.test.ts` — confirm, success, timeout, diagnostics render/copy, debug info, etc.
- App ~280 SettingsView props: `detailHandle`, `onResult`, `onRestarted`, `onBack`.
- Guard: `legacyTerminalRemoval.test.ts:81` greps `SettingsView.svelte` for `surfaceV2` / `Terminal Surface V2`.

## Code anchors

- Props identical to Svelte.
- testids: `dev-settings`, `dev-settings-debug`.
- Bespoke confirm label `Tap to confirm` (no AlertDialog).

## Test-first instructions

1. Port tests to `SettingsView.test.tsx` → RED without tsx.
2. ```bash
   npm run web:test -- --run crates/ajax-web/web/src/components/SettingsView.test.tsx
   ```
3. Implement; move CSS; App swap; delete svelte; repoint guard; green.

## Edit instructions

1. Mechanical React port.
2. Append `<style>` block from svelte into `styles.css`.
3. App ReactIsland swap.
4. Update legacyTerminalRemoval path only.
5. Delete Svelte files; grep `SettingsView.svelte` empty (except TERMINAL docs historical refs OK — do not edit those docs unless required).

## Verification commands

```bash
npm run web:test -- --run crates/ajax-web/web/src/components/SettingsView.test.tsx crates/ajax-web/web/src/legacyTerminalRemoval.test.ts crates/ajax-web/web/src/components/App.test.ts
npm run web:check
npm run web:smoke -- crates/ajax-web/web/e2e/actions.test.ts
```

## Acceptance criteria

- Settings RTL + guard + App green; settings e2e green; no SettingsView.svelte left in src.

## Stop conditions

- Need to change api restart/session behavior.
- e2e requires weakening.
- Diff escapes allowed files.
