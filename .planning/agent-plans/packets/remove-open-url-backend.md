# TDD Implementation Packet: Remove backend `open_url` / Open Dev

PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Remove the Open Dev / `open_url` surface from the ajax-web Test in Dev API and shared TS type. Keep all Test in Dev deploy, poll, phase, error, and restart-script behavior (`DEV_PORT` stays).

## Allowed files

- `crates/ajax-web/src/slices/dev_deploy.rs`
- `crates/ajax-web/src/runtime.rs`
- `crates/ajax-web/web/src/shared/lib/types.ts`
- `crates/ajax-web/web/src/features/task/TestInDevPanel.test.tsx`
- `crates/ajax-web/web/src/features/task/TaskDetail.test.tsx`

## Forbidden changes

- Removing `/api/dev-deploy` GET/POST, slot lifecycle, `DEV_PORT`, restart script spawn, or Test in Dev UI panel.
- Touching `TaskDetail.tsx` / `TestInDevPanel.tsx` (Open Dev UI already removed).
- Architecture docs, commits, pushes, branch changes, unrelated formatting.

## Context evidence

- Desired behavior: user asked to remove backend open-in-dev functionality; keep Test in Dev only.
- Backend constant/field: `DEV_OPEN_URL` and `DevDeployStatus.open_url` in `dev_deploy.rs`; filled in `DevDeploySlot::status()`.
- Unit test solely for Open Dev URL: `open_url_is_fixed_ajaxdev_endpoint` in `dev_deploy.rs`.
- HTTP integration assert: `runtime.rs` test `dev_deploy_status_and_reject_non_ajax_paths` asserts `status_body["deploy"]["open_url"] == "https://ajaxdev.mossyhome.net:8788"`.
- Frontend type: `DevDeployStatus.open_url: string` in `types.ts`; mocks in panel/detail tests still include `open_url` keys (harmless once optional/gone — remove keys to match API).
- `DEV_PORT` is used by `spawn_test_in_dev` / `test_in_dev_command_args` — keep it (restart args, not Open Dev URL).

## Code anchors

- `dev_deploy.rs` L17: `pub const DEV_OPEN_URL: &str = "https://ajaxdev.mossyhome.net:8788";`
- `dev_deploy.rs` L74: `pub open_url: String,` inside `DevDeployStatus`
- `dev_deploy.rs` L144: `open_url: DEV_OPEN_URL.to_string(),` in `status()`
- `dev_deploy.rs` L489–491: `fn open_url_is_fixed_ajaxdev_endpoint`
- `runtime.rs` L2956–2959: assert on `status_body["deploy"]["open_url"]`
- `types.ts` L162: `open_url: string;`
- Mock objects in `TestInDevPanel.test.tsx` / `TaskDetail.test.tsx` with `open_url: "https://ajaxdev.mossyhome.net:8788"`

## Test-first instructions

1. Delete or rewrite `open_url_is_fixed_ajaxdev_endpoint` so it fails if `DEV_OPEN_URL` / `open_url` remain — preferred: replace with a small test that `DevDeploySlot::default().status()` JSON (via `serde_json::to_value`) does **not** contain key `"open_url"`, and remove the old constant assertion test.
2. In `runtime.rs` `dev_deploy_status_and_reject_non_ajax_paths`, replace the `open_url` equality assert with:
   `assert!(status_body["deploy"].get("open_url").is_none());`
3. In `types.ts`, remove `open_url` from `DevDeployStatus`.
4. Strip `open_url` from all deploy mocks in the two allowed frontend test files.
5. Red commands (expect failure while production still emits `open_url`):

```bash
cargo test -p ajax-web --lib -- open_url
cargo test -p ajax-web --lib -- dev_deploy_status_and_reject_non_ajax_paths
```

If the first filter matches nothing after renaming the unit test, use the new test name from step 1.

## Edit instructions

1. `dev_deploy.rs`: delete `DEV_OPEN_URL`; delete `open_url` field from `DevDeployStatus`; delete the `open_url:` line in `status()`; implement the replacement unit test from Test-first.
2. `runtime.rs`: change the open_url assert as above; keep shared_slot / phase asserts.
3. `types.ts`: remove `open_url: string;`.
4. Frontend test mocks: delete `open_url` properties only.

## Verification commands

```bash
cargo test -p ajax-web --lib -- open_url
cargo test -p ajax-web --lib -- dev_deploy_status_and_reject_non_ajax_paths
cargo test -p ajax-web --lib -- slices::dev_deploy
npm run web:test -- crates/ajax-web/web/src/features/task/TestInDevPanel.test.tsx crates/ajax-web/web/src/features/task/TaskDetail.test.tsx
```

## Acceptance criteria

- `/api/dev-deploy` JSON `deploy` object has no `open_url` field.
- `DEV_OPEN_URL` is gone; `DEV_PORT` and Test in Dev start/status remain.
- TS `DevDeployStatus` has no `open_url`.
- Focused cargo + web tests above exit 0.

## Stop conditions

- Diff needs routes outside Allowed files.
- Removing `DEV_PORT` or restart spawn to “fix” compile errors.
- Scope grows beyond open_url removal.
- Patch exceeds ~400 lines.
