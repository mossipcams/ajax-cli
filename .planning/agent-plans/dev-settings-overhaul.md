# Dev settings page overhaul

## Scope
Reorganize Cockpit Settings into a single **Dev settings** surface:
Surface V2 toggle, live debug info, diagnostics, reload app, restart server.

## Non-goals
- New backend endpoints
- Auth / multi-user settings
- Changing Ghostty / TerminalRawView
- Broad App shell redesign

## Delegation decision
`Delegation decision: delegated via model-router` → READY packet →
cursor-delegate (composer-2.5).

## Tasks
- [x] Restructure SettingsView into Dev settings (Experiments / Debug / Actions)
- [x] Extend diagnostics report with surface V2 flag + last error
- [x] Update SettingsView + diagnostics tests (TDD)
- [x] Rebuild dist; parent validate; open PR

## Validation results
- RED: 5 failed / 18 total (SettingsView 4, diagnostics 1)
- GREEN: 18 passed / 18 total
- `npm run web:build`: passed

## Validation
```bash
cd crates/ajax-web/web && npx vitest run src/components/SettingsView.test.ts src/diagnostics.test.ts
npm run web:build
```
