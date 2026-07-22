# Feat: Test in Stable (stable Settings)

## Scope

- Server advertises `test_in_stable` on `GET /api/version` when profile is
  `stable` and `AJAX_WEB_RESTART_SCRIPT` exists.
- `POST /api/server/test-in-stable` runs `dev-web-restart.sh --profile stable`
  (sync origin/main + cargo install); 404 when disabled.
- Settings: remove Reload app / Restart server; add gated Test in Stable.
- Keep `POST /api/server/restart` endpoint (unused by Settings).

## Non-goals

- Test in Dev / worktree deploys to stable
- Config-file or Cargo feature flags
- architecture.md rewrite unless version API already documented

## Delegation decision

`Delegation decision: delegated via model-router`

- First: `pi-delegate` / `opencode-go/glm-5.2` → FAILED (weekly usage limit)
- Reroute once: `cursor-delegate` / `composer-2.5` → code landed; report
  envelope extract failed (`STATUS: SUCCESS` vs expected schema)
- Parent Review Gate: fixed compile borrow in `test_in_stable_enabled_from_env`,
  install.rs bundle assert (restart string tree-shaken out), settings note
  copy; re-ran validation

## Task checklist

### Task 1: Persistent plan + packet

- [x] Write this ledger
- [x] Build READY TDD packet
- [x] Route via model-router and delegate

### Task 2: Server capability + endpoint

- [x] Failing Rust tests first (delegate)
- [x] Helpers + version field + POST route
- [x] Focused ajax-web nextest green (parent)

### Task 3: Settings UI

- [x] Failing vitest first (delegate)
- [x] Remove Reload/Restart; add Test in Stable
- [x] api client + e2e update + web:build

## Validation

```bash
rtk cargo nextest run -p ajax-web -- adapters::server::tests test_in_stable axum_router_reports_shell_version bundle_targets_the_same_origin_api
rtk npm run web:test -- --run src/features/settings/SettingsView.test.tsx src/shared/lib/api.test.ts src/shared/lib/polling.test.ts
rtk npm run web:build
```

## Deviations

- GLM lane unavailable → Cursor composer-2.5
- Parent fixed E0716 temporary borrow in `server.rs`
- Bundle assert drops `/api/server/restart` (Settings no longer references it;
  tree-shaken from `app.js`); keeps `/api/server/test-in-stable`

## Validation ledger

- `rtk cargo nextest run -p ajax-web -- adapters::server::tests test_in_stable axum_router_reports_shell_version bundle_targets_the_same_origin_api` → PASS (11)
- `rtk npm run web:test -- --run src/features/settings/SettingsView.test.tsx src/shared/lib/api.test.ts src/shared/lib/polling.test.ts` → PASS (40)
- `rtk npm run web:build` → PASS
