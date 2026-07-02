# Cockpit small refactor — remove behavior that doesn't make sense

Mode: Refactor/Cleanup (behavior-preserving deletions of dead cockpit
behavior). Scope: `crates/ajax-web/src/slices/cockpit.rs` + the browser
cockpit frontend. No lifecycle, registry, terminal, or routing changes.

## Findings (what's messy and why)

1. **`agent_activity` is a duplicate field.** `browser_task_detail_view`
   literally does `let agent_activity = live_status_summary.clone()` and
   serializes the same string twice under two names in every task-detail
   response. No component, guard, or test reads `agent_activity` (only the
   byte-equality fixture contains it). Pure duplication from an old rename.
2. **`BrowserBackend.warning` is always `null` and never read.** It is a
   vestige of the retired Docker/host-native distinction; `host_native_backend()`
   hardcodes `warning: None` and no frontend code reads `backend.warning`
   (or any `backend.*` field — `authority`/`control_enabled` are asserted in
   Rust tests, so those stay for now).
3. **`detailHandle` diagnostics plumbing is unreachable.** `App.svelte`
   always passes `detailHandle={null}` to `SettingsView` (settings is its own
   hash route — there is no "current task" there), so
   `buildDiagnosticsReport(detailHandle)`'s task-probe branch in
   `diagnostics.ts` can never execute. A diagnostics "feature" that cannot
   trigger. No test covers the branch.
4. **(Optional, needs a call) `agent_attempts` is built, validated, and never
   rendered.** Rust maps `BrowserAgentAttempt`s into every detail response,
   `assertDetail` *hard-fails* if the array is missing, `fixtures.test.ts`
   pins its shape — and no component displays attempts anywhere. Removing it
   deletes a passing fixture test along with the unused payload, so it is
   split out as an explicitly-approved slice rather than bundled.

Noted but deliberately NOT in scope (larger or pinned behavior):
- `axum_task_get` path-suffix sniffing and the `/snapshot` 404 special case
  (pinned by tests; changing it alters a response body).
- `browser_task_detail_view` building the full cockpit projection to serve
  one task (consistency-preserving; restructuring touches the core boundary).
- `statusMeta`'s lenient unknown-status fallback (defense-in-depth behind the
  contract guards; harmless).
- Other unused-but-permitted detail diagnostics fields (`lifecycle`,
  `live_status_kind/summary`, `annotations`, timestamps, `tmux`) —
  architecture.md explicitly allows raw values as detail diagnostics.

## Slices

### Slice 1 — drop `agent_activity` (Rust + types + fixture)
- Remove the field from `BrowserTaskDetail`, the clone line in
  `browser_task_detail_view`, `types.ts`, and the
  `web/src/fixtures/task-detail.json` fixture entry.
- Tests: the existing byte-equality test
  `committed_task_detail_fixture_matches_production_serialization` is the
  characterization — update the fixture, run
  `cargo nextest run -p ajax-web -E 'test(fixture)'` + `npm run web:test`.
- No new test: dead-code deletion proven unused (grep shows zero readers).

### Slice 2 — drop `BrowserBackend.warning`
- Remove the field from the struct, `host_native_backend()`, `types.ts`, and
  the `"warning": null` entries in `fixtures/cockpit.json` +
  `fixtures/operation.json`.
- Keep `authority` + `control_enabled` (Rust tests assert them; removing
  them is a separate decision).
- Same validation as slice 1 plus the `operation.json` fixture test in
  `runtime.rs`.

### Slice 3 — delete the unreachable `detailHandle` diagnostics path
- `App.svelte`: stop passing `detailHandle={null}`.
- `SettingsView.svelte`: drop the prop.
- `diagnostics.ts`: drop the parameter and the dead `checks.task` branch.
- No test deletions needed (branch was never covered); `diagnostics.test.ts`
  and `SettingsView.test.ts` must stay green unchanged.

### Slice 4 (OPTIONAL — approve separately) — drop unused `agent_attempts`
- Remove `BrowserAgentAttempt` + mapping in `cockpit.rs`, the `assertDetail`
  array requirement in `contracts.ts`, `types.ts` entries, fixture entries,
  and the `fixtures.test.ts` "agent_attempts is an array" test (deleted with
  the feature it pins), plus `agent_attempts: []` in the `api.test.ts` /
  `TaskDetail.test.ts` fixtures.
- Justification for the test deletion: the payload has no consumer; the test
  pins serialization of data nothing displays. Skipped unless approved.

### Finishing steps (all slices)
- `npm run web:build` (dist snapshot tests in `install.rs` and
  `ajax-cli/web_backend.rs` pin the bundle).
- Full validation: `cargo fmt --check`, `cargo check/clippy --all-targets
  --all-features -- -D warnings`, `cargo nextest run --all-features`,
  `npm run web:check`, `npm run web:test -- --run`.

## Risks
- Fixture JSON must byte-match serde output; the equality tests catch any
  mismatch immediately.
- These are same-binary API shape changes: the only consumer is the bundled
  shell, so there is no version-skew risk, but the browser contract narrows —
  anything external scraping `/api/tasks/{handle}` would lose the removed
  fields (unsupported usage per architecture.md).
- Slice 4 deletes a passing test with its dead feature; excluded by default.

Estimated size: ~60–90 lines removed across slices 1–3, no behavior change
visible in the UI.
