# TDD Implementation Packet: Raw-First Mobile Terminal Selective Revert

## 1. Goal

Make the Web Cockpit task terminal raw xterm/tmux-first on mobile and desktop by selectively reverting the Live/snapshot/composer detour while preserving all raw terminal hardening already added on this branch. The terminal panel should render the raw terminal directly, and the snapshot/composer frontend and unused backend routes should be removed unless execution discovers another approved caller.

## 2. Allowed files

Test files:

- `crates/ajax-web/web/src/components/TerminalPanel.test.ts`
- `crates/ajax-web/web/src/components/TerminalRawView.test.ts`
- `crates/ajax-web/web/src/components/TerminalSnapshotView.test.ts`, only to replace obsolete snapshot/composer assertions with removal/absence coverage; remove the obsolete test file only after replacement coverage exists or the user explicitly approves deletion
- `crates/ajax-web/src/runtime.rs`
- `crates/ajax-web/src/slices/terminal.rs`

Production files:

- `crates/ajax-web/web/src/components/TerminalPanel.svelte`
- `crates/ajax-web/web/src/components/TerminalRawView.svelte`, only if the raw input test proves printable text or Enter is not sent
- `crates/ajax-web/web/src/components/TerminalSnapshotView.svelte`
- `crates/ajax-web/web/src/api.ts`
- `crates/ajax-web/src/runtime.rs`
- `crates/ajax-web/src/slices/terminal.rs`

Documentation files:

- `README.md`
- `architecture.md`
- `AGENTS.md`
- `docs/plans/2026-07-01-raw-first-mobile-terminal.md`
- `docs/plans/2026-07-01-fix-web-composer-literal-send-keys.md`
- `docs/plans/2026-07-01-tdd-packet-web-composer-literal-send-keys.md`
- `docs/plans/2026-07-01-selective-revert-live-terminal.md`

Generated files:

- `crates/ajax-web/web/dist/app.js`
- `crates/ajax-web/web/dist/app.css`

## 3. Forbidden Changes

- Do not use blind `git revert` for `d491f4f`; it would remove `TerminalRawView` and discard later raw reconnect-overlay and scroll-interception fixes.
- Do not remove or weaken raw terminal hardening from these commits:
  - `e6823fd` grouped tmux client sessions
  - `95510cf` orphaned ephemeral session reaper
  - `3e1a3ba` keyboard resize suppression/debounce
  - `46a5305` readable raw terminal font
  - `a950997` sticky Ctrl behavior
  - `b530d32` raw reconnect/foreground resume
  - `24cfb5e` reconnect overlay cleanup
  - merge-resolution touch/wheel scroll interception in `TerminalRawView`
- Do not edit `crates/ajax-cli/tests/smoke_user_flows.rs`.
- Do not change task lifecycle, registry truth, Cockpit projection, action policy, or WebSocket frame shapes.
- Do not accept raw tmux session names from the browser.
- Do not add dependencies.
- Do not reintroduce `Live` or snapshot/composer as the default terminal mode.
- Do not remove the raw `/api/tasks/{handle}/terminal` WebSocket route.
- Do not alter unrelated pane/cockpit status capture code outside the browser task-terminal routes.

## 4. Architecture Context

Architecture source: `architecture.md`. Graphify was not available in this session, so `architecture.md` is the authoritative architecture map per repository instructions. Stop if a later Graphify map contradicts these boundaries.

- `ajax-web` is a browser presentation adapter over shared Ajax backend contracts, not a second task domain.
- `ajax-web::runtime` owns Axum routing and should remain a thin adapter from HTTP/WebSocket routes into slices/adapters.
- `ajax-web::slices::terminal` owns browser task-terminal capability planning: it resolves an Ajax task handle to the registered `tmux_session` and fixed `worktrunk` target.
- `ajax-web::adapters::terminal_pty` owns the raw PTY/tmux attach mechanism behind `/api/tasks/{handle}/terminal`.
- `ajax-core` and tmux remain authoritative for task and terminal substrate truth.
- The raw terminal bridge is the primary task-terminal surface. Pane snapshots and composer/send-keys routes were introduced for the Live detour and should be removed unless another approved caller is found.

Serena context:

- `prepare_task_terminal` declaration: `crates/ajax-web/src/slices/terminal.rs`, body lines 28-48.
- `send_task_keys` declaration: `crates/ajax-web/src/slices/terminal.rs`, body lines 60-84.
- Serena did not return declarations for TypeScript browser API helpers in this Rust-activated project; use the ast-grep/text anchors below.

## 4.5 Review Findings And Solutions

These findings came from an architecture/roadmap review and an AST/text
inventory before execution. Treat the solutions below as part of this packet.

1. Product-roadmap alignment:
   - Finding: The raw-first direction matches `architecture.md`: Web Cockpit is
     dashboard-first, then opens an authenticated terminal bridge that resolves
     a browser-submitted Ajax task handle to the registered `tmux_session` and
     fixed `worktrunk` target.
   - Solution: Keep this plan raw terminal bridge first, but preserve dashboard
     pane snapshots and guarded pane approval under `ajax-web::slices::pane`.
     This packet removes only the terminal Live/snapshot/composer detour.

2. Codeman alignment:
   - Finding: Codeman's mobile terminal model is raw xterm/tmux first, with
     zero-lag input, keyboard/touch affordances, reconnect, and quick commands.
     `TerminalRawView` already carries the Ajax equivalents for zero-lag input,
     readable font, local scrollback, keyboard resize suppression, sticky Ctrl,
     and reconnect.
   - Solution: Preserve those raw hardening behaviors and make them the default.
     Do not claim full Codeman parity from this slice alone; quick slash-command
     buttons such as `/clear`, `/init`, or `/compact` are a follow-up unless an
     approved plan adds them.

3. Unsupported terminal subroutes:
   - Finding: A broad `404` assertion for `/snapshot` can pass accidentally if
     the request falls through to task-detail lookup for handle
     `web/fix-login/snapshot`, returning `task not found` rather than proving the
     terminal subroute is intentionally unsupported.
   - Solution: Add exact route-removal tests that distinguish removed terminal
     subroutes from ordinary task-detail misses. Unsupported `/keys` and
     `/snapshot` subroutes must return the chosen generic API not-found shape
     (`{ "ok": false, "error": "not found" }`) and must not execute task-detail,
     pane capture, or tmux send-keys logic.

4. Test assertion preservation:
   - Finding: `TerminalSnapshotView.test.ts` and the terminal-slice
     `send_task_keys`/`task_pane_snapshot` tests assert real behavior. Removing
     them is a capability removal, not harmless cleanup.
   - Solution: Replace obsolete tests with explicit absence/removal coverage
     wherever possible. Delete test files or behavior tests only if the approved
     implementation removes that capability and there is replacement coverage
     proving the capability is unavailable. If there is any doubt, stop and ask
     for explicit user approval.

5. Pane approval boundary:
   - Finding: The architecture still uses pane snapshots and `send-keys` for
     guarded approval workflows outside the task terminal Live detour.
   - Solution: Do not remove or rewrite `ajax-web::slices::pane`,
     browser prompt/answer routes, `operate` remediation behavior, or generic
     tmux input adapter behavior that is still used outside terminal
     snapshot/composer routes.

6. Docs search precision:
   - Finding: Broad searches for `Live` or `snapshot` catch legitimate terms
     such as `Live Status`, Cockpit projection snapshots, and guarded pane
     snapshots.
   - Solution: Verification searches must target terminal-detour identifiers and
     phrases (`TerminalSnapshotView`, `sendTaskKeys`, `fetchTaskSnapshot`,
     `Terminal mode`, `snapshot/composer`, `mobile lands on the snapshot viewer`,
     `Raw terminal` tab) rather than banning all uses of `Live` or `snapshot`.

## 5. Code Anchors

Frontend test anchors:

```bash
ast-grep -p 'it($NAME, async () => { $$$BODY })' --lang ts crates/ajax-web/web/src/components/TerminalPanel.test.ts
```

Matches:

- `TerminalPanel.test.ts:89` `lands in the snapshot viewer on mobile and never opens the raw socket`
- `TerminalPanel.test.ts:99` `defaults to the raw terminal on desktop and opens the socket`
- `TerminalPanel.test.ts:108` `only opens the raw socket on mobile after an explicit opt-in`
- `TerminalPanel.test.ts:121` `persists the chosen mode across mounts`

Browser API anchors:

```bash
ast-grep -p 'export async function sendTaskKeys($$$ARGS): Promise<$RET> { $$$BODY }' --lang ts crates/ajax-web/web/src/api.ts
ast-grep -p 'export async function fetchTaskSnapshot($$$ARGS): Promise<$RET> { $$$BODY }' --lang ts crates/ajax-web/web/src/api.ts
```

Matches:

- `api.ts:261` `sendTaskKeys(...)` posts to `/api/tasks/${handle}/keys`
- `api.ts:284` `fetchTaskSnapshot(...)` gets `/api/tasks/${handle}/snapshot`

Runtime route anchors:

- `crates/ajax-web/src/runtime.rs:550` routes `/terminal`
- `crates/ajax-web/src/runtime.rs:553` routes `/snapshot`
- `crates/ajax-web/src/runtime.rs:622` defines `axum_task_snapshot`
- `crates/ajax-web/src/runtime.rs:663` defines `SendKeysRequest`
- `crates/ajax-web/src/runtime.rs:670` defines `axum_task_post`
- `crates/ajax-web/src/runtime.rs:679` routes `/keys`
- `crates/ajax-web/src/runtime.rs:688` defines `axum_task_keys`

Runtime test/helper anchors:

- `crates/ajax-web/src/runtime.rs:1398` `authenticated_request(cookie, uri)`
- `crates/ajax-web/src/runtime.rs:1483` `axum_api_access_policy_classifies_public_and_protected_routes`
- `crates/ajax-web/src/runtime.rs:2998` `axum_task_terminal_requires_browser_session_cookie`
- `crates/ajax-web/src/runtime.rs:3027` `axum_task_terminal_rejects_non_upgrade_requests`

Terminal slice anchors:

```bash
ast-grep -p 'pub struct TaskPaneSnapshotView { $$$FIELDS }' --lang rust crates/ajax-web/src/slices/terminal.rs
ast-grep -p 'pub enum SnapshotRouteError { $$$VARIANTS }' --lang rust crates/ajax-web/src/slices/terminal.rs
ast-grep -p 'pub enum SendKeysRouteError { $$$VARIANTS }' --lang rust crates/ajax-web/src/slices/terminal.rs
```

Matches:

- `terminal.rs:29` `prepare_task_terminal`
- `terminal.rs:54` `SendKeysRouteError`
- `terminal.rs:64` `send_task_keys`
- `terminal.rs:90` `TaskPaneSnapshotView`
- `terminal.rs:103` `SnapshotRouteError`
- `terminal.rs:109` `fingerprint_lines`
- `terminal.rs:122` `task_pane_snapshot`
- `terminal.rs:277` `send_task_keys_sends_literal_text_and_enter`
- `terminal.rs:342` `task_pane_snapshot_returns_lines_and_marks_change_on_first_capture`

Svelte source anchors from text search:

- `crates/ajax-web/web/src/components/TerminalPanel.svelte:1` imports `TerminalRawView`
- `TerminalPanel.svelte:3` imports `TerminalSnapshotView`
- `TerminalPanel.svelte:12` defines `STORAGE_KEY = "ajax.terminal.mode"`
- `TerminalPanel.svelte:21` comment says mobile lands on snapshot viewer/composer
- `TerminalPanel.svelte:26` defines `initialMode`
- `TerminalPanel.svelte:49` renders the `Terminal mode` tablist
- `TerminalPanel.svelte:66` renders `TerminalRawView` only when `mode === "raw"`
- `TerminalPanel.svelte:69` renders `TerminalSnapshotView` otherwise

Raw input test anchors:

- `crates/ajax-web/web/src/components/TerminalRawView.test.ts` already mocks `openTaskTerminalSocket`, `MockWebSocket`, `onDataHandler`, and asserts input frames in existing raw terminal tests.
- Reuse the existing `onDataHandler?.("a")` pattern from the raw input tests; add Enter coverage if missing.

## 6. Test-First Instructions

Task A: raw terminal is the default and only panel mode.

1. Edit `crates/ajax-web/web/src/components/TerminalPanel.test.ts` first.
2. Replace `lands in the snapshot viewer on mobile and never opens the raw socket` with:
   `defaults_to_raw_terminal_on_mobile_and_opens_the_socket`.
3. Test body:
   - `stubMatchMedia(true)`
   - `render(TerminalPanel, { props: { handle: "web/fix-login" } })`
   - `await tick()`
   - assert `openTaskTerminalSocket` was called with `"web/fix-login"`
   - assert `fetchTaskSnapshot` was not called
   - assert `queryByRole("tablist", { name: "Terminal mode" })` is not in the document
4. Remove or replace the mobile opt-in and persistence tests so they no longer assert mode-toggle behavior. Replace them with explicit absence assertions if useful:
   - no `Raw terminal` tab
   - no `Live` tab
   - no `localStorage` mode write
5. Run and confirm failure before production edits:

```bash
npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts
```

Expected failure before implementation: mobile still defaults to snapshot, `fetchTaskSnapshot` is called, the raw socket is not opened, and the tablist exists.

Task B: raw terminal sends `clear` as real typed input.

1. Edit `crates/ajax-web/web/src/components/TerminalRawView.test.ts`.
2. Add `sends_clear_command_text_and_enter_over_the_raw_socket` unless equivalent coverage already exists.
3. Use existing raw socket/onData test helpers:
   - render `TerminalRawView`
   - get first `MockWebSocket`
   - emit `open`
   - call `onDataHandler?.("c")`, `onDataHandler?.("l")`, `onDataHandler?.("e")`, `onDataHandler?.("a")`, `onDataHandler?.("r")`, `onDataHandler?.("\r")`
   - assert socket sends JSON input frames for each character and `"\r"`
   - assert `/keys` or `sendTaskKeys` is not involved
4. Run:

```bash
npm run web:test -- --run crates/ajax-web/web/src/components/TerminalRawView.test.ts -t "clear command"
```

If this test already passes before production edits, record it as existing raw coverage and do not modify `TerminalRawView.svelte`.

Task C: frontend snapshot/composer is removed from the task terminal surface.

1. In `TerminalPanel.test.ts`, keep the absence assertions from Task A.
2. Remove `TerminalSnapshotView` imports/usages from the panel source after the failing test exists.
3. Replace `TerminalSnapshotView.test.ts` with absence/removal coverage rather
   than silently deleting assertions. Acceptable replacements include:
   - a `TerminalPanel` test proving the snapshot component is not imported or
     rendered by the terminal surface,
   - an API/module-level test proving `fetchTaskSnapshot` and `sendTaskKeys` are
     absent from the browser terminal path,
   - or a route-level test from Task D proving the obsolete backend capability is
     unavailable.
4. Delete `TerminalSnapshotView.test.ts` only when the component is deleted, the
   capability is intentionally removed by this approved packet, and replacement
   removal coverage exists. If there is any doubt under the repo assertion
   policy, stop and ask for explicit user approval.
5. Run:

```bash
rg -n "TerminalSnapshotView|sendTaskKeys|fetchTaskSnapshot|Terminal mode|mobile lands on the snapshot viewer|Raw terminal" crates/ajax-web/web/src || true
npm run web:test -- --run
```

Expected after implementation: no matches in `web/src` except any explicitly documented obsolete test pointer if retained.

Task D: backend snapshot/composer routes are removed if no callers remain.

1. Add runtime route-removal tests in `crates/ajax-web/src/runtime.rs` before removing implementation:
   - `axum_task_keys_route_is_not_supported`
   - `axum_task_snapshot_route_is_not_supported`
2. Follow existing `authenticated_request` pattern. For `/keys`, send an authenticated `POST /api/tasks/web%2Ffix-login/keys` with `{}` body and expect `404` JSON `{ "ok": false, "error": "not found" }`.
3. For `/snapshot`, send an authenticated `GET /api/tasks/web%2Ffix-login/snapshot` and expect `404` JSON `{ "ok": false, "error": "not found" }`.
4. The `/snapshot` test must prove the request is treated as an unsupported
   terminal subroute, not as task-detail lookup for a synthetic handle
   `web/fix-login/snapshot`. Add one of these guards:
   - assert the body is the generic API not-found shape, not
     `{ "ok": false, "error": "task not found" }`,
   - assert the test runner did not execute pane capture commands,
   - or route unsupported `/api/tasks/{handle}/{subroute}` paths through an
     explicit generic not-found branch before task-detail fallback.
5. Run and confirm failure before production removal:

```bash
cargo nextest run -p ajax-web axum_task_keys_route_is_not_supported --all-features
cargo nextest run -p ajax-web axum_task_snapshot_route_is_not_supported --all-features
```

Expected failure before implementation: routes currently exist and return non-404 behavior.

6. Replace obsolete terminal-slice route tests with tests that preserve
   `prepare_task_terminal` raw attach planning only. Treat the existing
   `send_task_keys` and `task_pane_snapshot` tests as real behavior coverage:
   remove them only after the route-removal tests are red, the implementation
   removes the terminal snapshot/composer capability, and replacement
   route-level absence coverage is green.
7. Do not remove `ajax-web::slices::pane`, prompt answer routes, `operate`
   remediation behavior, or `TmuxInputAdapter` if they are still used outside
   the terminal snapshot/composer path.

Task E: documentation is raw-first.

Markdown-only; no failing test required.

1. Update `README.md`, `architecture.md`, and `AGENTS.md` after code tests pass.
2. Mark superseded plan files as pointers to this file only.
3. Verify:

```bash
rg -n "TerminalSnapshotView|sendTaskKeys|fetchTaskSnapshot|snapshot/composer|mobile lands on the snapshot viewer|terminal mode|Raw terminal tab" README.md architecture.md AGENTS.md docs/plans || true
```

Expected docs result: remaining matches should be this packet, short
supersession pointers, or explicit raw-first guidance. They must not describe
the removed terminal snapshot/composer path as the primary mobile terminal.

## 7. Production Edit Instructions

Frontend panel:

1. In `TerminalPanel.svelte`, remove:
   - `import TerminalSnapshotView from "./TerminalSnapshotView.svelte";`
   - `type Mode = "live" | "raw"`
   - `STORAGE_KEY`
   - `isMobileViewport`
   - snapshot/composer default comment
   - `initialMode`
   - `mode` state
   - `setMode`
   - `<div class="terminal-mode-toggle"...>`
   - conditional `{#if mode === "raw"} ... {:else} ... {/if}`
   - mode-tab CSS
2. Render `<TerminalRawView {handle} />` directly inside `<section class="terminal-host-shell" data-testid="task-terminal">`.
3. Keep the host shell layout CSS unless tests or Svelte check require a minimal adjustment.
4. Delete `TerminalSnapshotView.svelte` only after no imports remain and
   replacement removal coverage exists.
5. In `api.ts`, remove `sendTaskKeys`, `fetchTaskSnapshot`, and
   `TaskPaneSnapshot` only after `rg` shows no frontend production callers
   remain and tests no longer import them except as explicit removal coverage.
6. Run `npm run web:build` so generated assets match source.

Raw terminal:

1. Do not edit `TerminalRawView.svelte` unless Task B fails.
2. If Task B fails, edit only the existing `term.onData` branch so printable characters and `"\r"` send `{ type: "input", data }` frames over the raw socket.
3. Do not route raw typing through `/keys`.

Backend:

1. In `runtime.rs`, keep `/terminal` routing at `handle.strip_suffix("/terminal")`.
2. Remove `/snapshot` routing and `axum_task_snapshot` if no approved caller remains.
3. Remove `SendKeysRequest`, `/keys` routing, and `axum_task_keys` if no approved caller remains.
4. Keep generic `axum_task_post` not-found behavior for unsupported task POST subroutes.
5. Add an explicit unsupported-subroute path for removed task GET subroutes if
   needed so `GET /api/tasks/web%2Ffix-login/snapshot` returns generic
   `{ "ok": false, "error": "not found" }` instead of falling through to task
   detail lookup and returning `"task not found"` for handle
   `web/fix-login/snapshot`.
6. In `slices/terminal.rs`, keep:
   - `TerminalAttachPlan`
   - `TerminalRouteError`
   - `prepare_task_terminal`
   - tests for registered session, unknown task, and missing tmux session
7. Remove composer/snapshot-only items if no caller remains:
   - `TmuxInputAdapter` import
   - `pane` import if used only for snapshot
   - `std::hash::{Hash, Hasher}`
   - `PANE_SNAPSHOT_LIMIT`
   - `SendKeysRouteError`
   - `send_task_keys`
   - `TaskPaneSnapshotView`
   - `SnapshotRouteError`
   - `fingerprint_lines`
   - `task_pane_snapshot`
8. Do not remove `ajax-web::slices::pane`, prompt answer routes,
   `ajax-web::slices::operate` remediation behavior, or shared tmux-input
   behavior that remains used outside this terminal route.
9. Do not alter `prepare_task_terminal` semantics.

Docs:

1. `README.md`: document raw xterm/tmux as the mobile task terminal default; remove language that says free-text composer escalates or Live is primary.
2. `architecture.md`: document browser terminal bridge as primary, with pane snapshots not part of the default mobile terminal strategy.
3. `AGENTS.md`: add guidance that future web terminal work must preserve raw-first mobile terminal strategy and not reintroduce Live/snapshot/composer as default without explicit approval.
4. Superseded plan files should contain only short pointers to this packet.
5. Do not remove legitimate architecture language about `Live Status`, Cockpit
   projection snapshots, or guarded pane snapshots.

## 8. Verification Commands

Focused red/green commands:

```bash
npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts
npm run web:test -- --run crates/ajax-web/web/src/components/TerminalRawView.test.ts -t "clear command"
cargo nextest run -p ajax-web axum_task_keys_route_is_not_supported --all-features
cargo nextest run -p ajax-web axum_task_snapshot_route_is_not_supported --all-features
```

Focused post-edit checks:

```bash
rg -n "TerminalSnapshotView|sendTaskKeys|fetchTaskSnapshot|Terminal mode|mobile lands on the snapshot viewer|Raw terminal" crates/ajax-web/web/src || true
cargo nextest run -p ajax-web --all-features terminal
npm run web:check
npm run web:build
```

Docs verification:

```bash
rg -n "TerminalSnapshotView|sendTaskKeys|fetchTaskSnapshot|snapshot/composer|mobile lands on the snapshot viewer|terminal mode|Raw terminal tab" README.md architecture.md AGENTS.md docs/plans || true
```

Expected docs result: remaining matches should be this packet, short
supersession pointers, or explicit raw-first guidance. They must not describe
the removed terminal snapshot/composer path as the primary mobile terminal.

Final validation:

```bash
rg '<<<<<<<|=======|>>>>>>>' crates/ajax-web README.md architecture.md AGENTS.md docs/plans
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
npm run web:test -- --run
gh pr view 263 --json mergeable,mergeStateStatus,statusCheckRollup
```

## 9. Acceptance Criteria

- Before production edits, the new mobile raw-default panel test fails because mobile still opens snapshot mode.
- After production edits, mobile and desktop both mount the raw terminal socket by default.
- The terminal mode tablist is absent.
- `TerminalSnapshotView` is not rendered or imported by the task terminal panel.
- Raw terminal typing sends `clear` and Enter through raw socket input frames.
- Raw terminal reconnect, keyboard resize, grouped tmux sessions, sticky Ctrl, readable font, and scroll interception tests still pass.
- `/api/tasks/{handle}/terminal` remains protected and functional.
- If `/keys` and `/snapshot` are removed, route tests assert they return generic
  not found and do not fall through to task-detail lookup or execute tmux/pane
  commands.
- Obsolete snapshot/composer tests are replaced by explicit removal coverage, or
  deletion is explicitly approved by the user after the replacement coverage is
  identified.
- Guarded pane approval snapshots and non-terminal `send-keys` behavior remain
  intact where still used outside this terminal Live detour.
- README, `architecture.md`, and `AGENTS.md` state raw-first terminal strategy and do not describe Live/snapshot/composer as the primary mobile terminal.
- Docs verification does not ban legitimate `Live Status`, Cockpit projection
  snapshot, or guarded pane snapshot language.
- Generated web assets are rebuilt if frontend source changes.
- Full validation commands pass, or any failure is reported with exact command and cause.
- PR #263 remains mergeable or any merge/CI blocker is reported.

## 10. Stop Conditions

- Stop if a Graphify architecture map is required and contradicts `architecture.md`.
- Stop if any approved caller outside `TerminalSnapshotView` still uses `sendTaskKeys`, `fetchTaskSnapshot`, `/keys`, or `/snapshot`.
- Stop if removing `TerminalSnapshotView.test.ts` or backend
  `send_task_keys`/`task_pane_snapshot` tests would delete assertions without
  replacement removal coverage and explicit user approval.
- Stop if raw terminal `clear` input does not work and the fix would require changing WebSocket frame shapes.
- Stop if changes outside the allowed files are required.
- Stop if any existing raw terminal hardening behavior must be removed to make tests pass.
- Stop if `npm run web:build` changes unexpected generated files beyond `web/dist/app.js` or `web/dist/app.css`.
- Stop if backend route removal breaks cockpit/agent prompt handling outside the browser task-terminal surface.
- Stop if making `/snapshot` return generic not found requires weakening normal
  task-detail lookup for real task handles.
- Stop if a docs cleanup would remove valid architecture references to
  `Live Status`, Cockpit projection snapshots, or guarded pane snapshots.
