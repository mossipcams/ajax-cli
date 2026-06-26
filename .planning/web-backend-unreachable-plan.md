# Web Backend Unreachable Plan

## Context

The Svelte browser shell currently sets the connection state to
`backend unreachable` for any dashboard fetch failure. That includes cases where
the backend answered with an HTTP error or an incompatible payload, so the UI
can misreport a reachable backend as unreachable. The shell already has a
`connectionDetail` field and `ConnectionStatus` already renders detail text,
but `App.svelte` never populates or clears it.

## Task 1: Classify cockpit connection failures and surface details

- Failing behavior test to write:
  - Update `crates/ajax-web/web/src/components/App.test.ts` to stub
    `fetch("/api/cockpit")` returning HTTP 503 and assert the connection banner
    shows `disconnected: HTTP 503`, not bare `backend unreachable`.
  - Add a second assertion path for a rejected fetch and assert the banner shows
    `backend unreachable: <network message>`.
- Code to implement:
  - Add a small local error-to-connection helper in
    `crates/ajax-web/web/src/components/App.svelte`.
  - In `loadCockpit`, map `ApiError("network", ...)` to
    `backend unreachable` and other `ApiError` failures to `disconnected`.
  - Populate `connectionDetail` from the error message.
  - Clear `connectionDetail` in `applyCockpit`.
- Verification:
  - Run `rtk npm run web:test -- --run src/components/App.test.ts` and show the
    failure before implementation, then the pass after implementation.

## Task 2: Apply the same connection handling to task detail refreshes

- Failing behavior test to write:
  - Update `crates/ajax-web/web/src/components/App.test.ts` to navigate to a
    task route, stub `fetch("/api/tasks/...")` returning HTTP 500, and assert
    the banner shows `disconnected: HTTP 500`.
  - Include a success-after-failure path proving a later successful detail load
    returns the connection label to `connected` with no stale detail.
- Code to implement:
  - Reuse the helper from Task 1 in `loadDetail`.
  - Set connection state/detail for all `ApiError` detail failures, not only
    network failures.
  - Clear `connectionDetail` on successful detail load.
- Verification:
  - Run `rtk npm run web:test -- --run src/components/App.test.ts` and show the
    failure before implementation, then the pass after implementation.

## Task 3: Rebuild and verify the bundled web shell contract

- Failing behavior test to write:
  - No new failing test; this is a generated asset and contract verification
    task after the source behavior is covered by Tasks 1 and 2.
- Code/assets to implement:
  - Run the web build so `crates/ajax-web/web/dist/app.js`,
    `app.css`, and `index.html` reflect the `App.svelte` change.
- Verification:
  - Run `rtk npm run web:build`.
  - Run `rtk npm run web:dist:check` if available.
  - Run `rtk cargo test -p ajax-web slices::install::tests::bundle_targets_the_same_origin_api_and_never_registers_a_worker`.

## Final validation

- Run the strongest applicable checks:
  - `rtk cargo fmt --check`
  - `rtk cargo check --all-targets --all-features`
  - `rtk cargo clippy --all-targets --all-features -- -D warnings`
  - `rtk cargo nextest run --all-features`
  - `rtk npm run web:check`
  - `rtk npm run web:test -- --run`
  - `rtk npm run web:dist:check`

Plan ready. Approve to proceed.
