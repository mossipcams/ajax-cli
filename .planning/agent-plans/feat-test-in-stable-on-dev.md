# Feat: Test in Stable on Ajax web dev Settings

## Scope

- Settings on `--profile dev` must show **Test in Stable** (same control as
  stable Settings).
- `GET /api/version` advertises `test_in_stable: true` when restart + wrapper
  scripts exist and the runtime profile is `dev` or `stable`.
- From **dev**: spawn `test-in-stable.sh --profile stable --port 8787` (never
  the current dev port). Do **not** `process::exit` the dev server.
- From **stable**: unchanged (spawn wrapper, then exit this process).
- Settings client: when this instance will not restart (dev), POST then report
  success; do not `waitForServerRestart` / `location.replace`. Stable keeps the
  existing wait/reload path.
- Expose enough version metadata for the client to tell those two cases apart
  (reuse profile if already resolved; do not invent a second policy engine).

## Non-goals

- Worktree deploys onto stable
- Changing Test in Dev task-detail deploy
- Merging stable/dev browser or registry state
- Blue/green restart UX (#873)

## Approval

User requested the missing Settings button immediately. Implement now.

## Task checklist

- [x] Enablement: `dev` or `stable` + scripts; stable port when current profile
      is not stable
- [x] Exit only when this process is the stable instance being replaced
- [x] Version payload + Settings UI/tests (button visible on dev; no self-reload;
      stable POST drop still waits/reloads #850)
- [x] Focused rust + vitest verification
- [x] Parent review

## Validation

```bash
rtk cargo nextest run -p ajax-web -- adapters::server::tests test_in_stable
rtk cargo nextest run -p ajax-web -- test_in_stable axum_router_reports_shell_version
rtk npm run web:test -- --run src/features/settings/SettingsView.test.tsx src/shared/lib/api.test.ts
```
