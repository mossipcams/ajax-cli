# Remove backend Open Dev (`open_url`); keep Test in Dev

## Scope

- Remove API/status field `open_url` and constant `DEV_OPEN_URL` from ajax-web Test in Dev.
- Update Rust + TS types/tests that still assert or mock `open_url`.
- Keep deploy/poll/restart (`Test in Dev`) including `DEV_PORT` for the restart script.

## Non-goals

- No UI placement changes (already done).
- No removal of `/api/dev-deploy` endpoints or slot lifecycle.
- No docs cleanup unless a test requires it.

## Delegation decision

`Delegation decision: delegated via model-router` (backend API field removal → `pi-delegate` / `opencode-go/glm-5.2`).

## Task checklist

- [x] Failing tests: drop/assert-absent `open_url` in Rust unit + runtime API test; strip from TS type + mocks
- [x] Remove `DEV_OPEN_URL` + `DevDeployStatus.open_url` from `dev_deploy.rs`
- [x] Remove `open_url` from `types.ts`
- [x] Parent review + focused cargo/web tests

## Approval

Not required (explicit follow-up).

## Deviations

- Delegate report schema invalid; parent accepted via delta inspect + independent verify.

## Validation

```bash
cargo test -p ajax-web --lib -- open_url
# exit 0 — dev_deploy_status_has_no_open_url_field
cargo test -p ajax-web --lib -- dev_deploy_status_and_reject_non_ajax_paths
# exit 0
cargo test -p ajax-web --lib -- slices::dev_deploy
# exit 0 — 12 passed
npm run web:test -- …/TestInDevPanel.test.tsx …/TaskDetail.test.tsx
# exit 0 — 24 passed
```

## Review Gate

`VERDICT: ACCEPT` — `open_url` / `DEV_OPEN_URL` gone; `DEV_PORT` + Test in Dev retained; scope clean.
