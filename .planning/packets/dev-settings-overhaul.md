# TDD Implementation Packet — Dev settings overhaul

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal
Overhaul [`SettingsView.svelte`](crates/ajax-web/web/src/components/SettingsView.svelte) into a single **Dev settings** section with Experiments (Surface V2), live Debug info, and Actions (Reload app, Run/Copy diagnostics, Restart server). Extend diagnostics report with Surface V2 flag + last error.

## Allowed files
- `crates/ajax-web/web/src/components/SettingsView.svelte`
- `crates/ajax-web/web/src/components/SettingsView.test.ts`
- `crates/ajax-web/web/src/diagnostics.ts`
- `crates/ajax-web/web/src/diagnostics.test.ts`
- `crates/ajax-web/web/dist/app.js` / `app.css` / `terminal.js` if `npm run web:build` updates them
- `.planning/agent-plans/dev-settings-overhaul.md` (checklist only)

## Forbidden changes
- Do not edit TerminalRawView, WtermTerminalView, or `/api/server/restart` backend
- Do not rename Restart/Diagnostics button accessible names (e2e depends on them)
- Keep class `.settings-section` for e2e visual tests
- No commit/push/branch changes

## Context evidence
- Graphify: `NOT_REQUIRED` — single settings component
- Serena: `NOT_REQUIRED`
- ast-grep: `NOT_REQUIRED`
- Existing: `restartServer` / `waitForServerOnline`, `buildDiagnosticsReport`, `isTerminalSurfaceV2Enabled`
- sessionStorage key: `ajax.terminal.surfaceV2.lastError`
- localStorage key: `ajax.terminal.surfaceV2`
- meta: `meta[name="ajax-app-version"]`

## Code anchors
- Current flat sections: Web server / Diagnostics / Experimental in SettingsView.svelte
- Tests asserting `"Experimental"` in SettingsView.test.ts

## Test-first instructions
1. Update/add failing tests first:
   - Renders `data-testid="dev-settings"` and heading "Dev settings"
   - Surface V2 toggle still present (`setting-terminal-surface-v2`)
   - Live debug shows origin (and version if meta stubbed)
   - Shows last Surface V2 error when sessionStorage set
   - Reload app button calls `location.reload` (spy)
   - `buildDiagnosticsReport` includes `terminal_surface_v2` boolean and `surface_v2_last_error`
   - Keep restart confirm + diagnostics copy tests working (button names unchanged)
2. RED: `cd crates/ajax-web/web && npx vitest run src/components/SettingsView.test.ts src/diagnostics.test.ts`

## Edit instructions
1. **SettingsView.svelte** — one `.settings-section` with `data-testid="dev-settings"` titled Dev settings:
   - **Experiments:** Surface V2 checkbox + note (existing setter)
   - **Debug info:** always-visible `<dl>` or pre lines: app version, origin, online, Surface V2 on/off, last error, truncated UA
   - **Actions:** Reload app (`location.reload()`), Run diagnostics, Copy diagnostics, Restart server (existing two-tap flow)
2. **diagnostics.ts** — add to report:
   - `terminal_surface_v2: localStorage.getItem("ajax.terminal.surfaceV2") === "true"` (or call isTerminalSurfaceV2Enabled if importing is fine — avoid circular deps; reading storage is OK)
   - `surface_v2_last_error: sessionStorage.getItem("ajax.terminal.surfaceV2.lastError")`
3. Rebuild dist via `npm run web:build` from repo root.

## Verification commands
```bash
cd crates/ajax-web/web && npx vitest run src/components/SettingsView.test.ts src/diagnostics.test.ts
cd <repo-root> && npm run web:build
```

## Acceptance criteria
- Dev settings layout matches plan
- All listed tests green; RED→GREEN proven
- Restart / Diagnostics button names preserved for e2e
- V2-only files in allowed scope

## Stop conditions
- Touching Ghostty/wterm terminal components
- Changing restart API contract
- Scope into App shell / nav redesign
